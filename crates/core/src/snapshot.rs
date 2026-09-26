//! Snapshots (spec §7.5): every AI change becomes a plain-titled git commit
//! carrying `InfinaBox-*` trailers; going back is always a new commit, never
//! a history rewrite.
//!
//! Trailers written (all in the commit message body):
//! - `InfinaBox-Snapshot: 1` — on every commit this module makes; it's what
//!   `list_snapshots` keys on, so ordinary commits made in Advanced mode (or
//!   before InfinaBox) are walked past but never listed.
//! - `InfinaBox-Thread: <id>` / `InfinaBox-Turn: <n>` — the chat turn that
//!   produced the change, when there was one.
//! - `InfinaBox-Restores: <sha>` — on "Went back to" commits, the commit whose
//!   tree was restored. Informational: undo doesn't need it (see `undo_last`).
//!
//! Safety rules this module keeps:
//! - Never writes git config; never resets or rewrites history.
//! - The only working-directory overwrite (`restore_to`) happens after every
//!   uncommitted change has been saved into its own snapshot first.
//! - Going back never rewinds the chat: `.ibproject/chat/` is a record of
//!   what happened, so a restore keeps the current chat files as they are.
//! - Going back never overwrites a file that exists on disk but was never
//!   committed (an ignored local file such as a config or `.env`).
//! - It refuses to act while git is mid-merge/rebase, or when the project
//!   sits inside some other git repository.
//! - Everything here that commits or checks out (`create_snapshot`,
//!   `restore_to`, `undo_last`) holds one process-wide lock, so an AI turn's
//!   snapshot can never land between a restore's commit and its checkout
//!   (which would commit the pre-restore files on top of the "Went back to"
//!   commit and mislabel history). It also serializes their use of the git
//!   index. `list_snapshots` only reads, so it doesn't take it.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use anyhow::{Context, Result};
use git2::{
    Commit, Config, ErrorCode, Index, IndexAddOption, ObjectType, Oid, Repository, RepositoryState,
    Signature, Sort, Tree, TreeWalkMode, TreeWalkResult, build::CheckoutBuilder,
};
use serde::Serialize;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Snapshot {
    /// The commit sha.
    pub id: String,
    pub title: String,
    /// Unix seconds.
    pub timestamp: i64,
    pub thread_id: Option<String>,
    pub turn: Option<u32>,
    pub files_changed: usize,
}

const TRAILER_SNAPSHOT: &str = "InfinaBox-Snapshot";
const TRAILER_THREAD: &str = "InfinaBox-Thread";
const TRAILER_TURN: &str = "InfinaBox-Turn";
const TRAILER_RESTORES: &str = "InfinaBox-Restores";

/// Used when neither the repo nor the user's git config has an identity.
const FALLBACK_NAME: &str = "InfinaBox";
const FALLBACK_EMAIL: &str = "infinabox@localhost";

const AUTO_SAVE_TITLE: &str = "Auto-save before going back";

/// Where chat threads live (see `chat_store`); kept as-is by every restore.
const CHAT_DIR: [&str; 2] = [".ibproject", "chat"];

/// The project marker (`src/lib/project-picker.ts`): the project's identity,
/// not game content, so a restore never removes or rewinds it.
const PROJECT_MARKER: [&str; 2] = [".ibproject", ".ibx"];

/// Ignore rules applied in memory on top of the project's own `.gitignore`
/// (never written to disk), so a project without one — or with an
/// incomplete one — doesn't commit Godot's cache, OS clutter, local secrets
/// or `chat_store`'s temp files. Export output folders are deliberately not
/// listed: their names vary and a user may well keep sources in `build/`.
const DEFAULT_IGNORES: &str = "\
.godot/
.import/
.env
.env.*
*.tmp
.DS_Store
Thumbs.db
desktop.ini
*.swp
";

/// Held by every public function that commits or checks out (see the
/// module docs). One lock for all projects: these calls are short and rare,
/// and a single lock can't be taken in the wrong order. `std`'s `Mutex`
/// isn't reentrant, so the public functions lock once and from then on only
/// call the `*_locked` functions.
static SNAPSHOT_LOCK: Mutex<()> = Mutex::new(());

fn snapshot_lock() -> MutexGuard<'static, ()> {
    // Poisoned only if a snapshot call panicked while holding it. The lock
    // guards no data of its own (git's state is on disk and every step
    // re-reads it), so carrying on is safe.
    SNAPSHOT_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Commits everything (respecting `.gitignore`). `None` when nothing
/// changed. `origin` is the (thread id, turn number) that produced it.
/// Initializes the repository if the project isn't one yet.
pub fn create_snapshot(
    project: &Path,
    title: &str,
    origin: Option<(&str, u32)>,
) -> Result<Option<Snapshot>> {
    let _guard = snapshot_lock();
    create_snapshot_locked(project, title, origin)
}

/// `create_snapshot`, for a caller already holding `SNAPSHOT_LOCK`.
fn create_snapshot_locked(
    project: &Path,
    title: &str,
    origin: Option<(&str, u32)>,
) -> Result<Option<Snapshot>> {
    let repo = match open_project_repo(project)? {
        Some(repo) => repo,
        None => Repository::init(project).with_context(|| {
            format!(
                "failed to initialize a Git repository at '{}'",
                project.display()
            )
        })?,
    };
    ensure_clean_state(&repo)?;
    let head = head_commit(&repo)?;

    let tree_id = stage_everything(&repo)?;
    let unchanged = match &head {
        Some(commit) => commit.tree_id() == tree_id,
        // Unborn HEAD: only an empty project counts as "nothing changed".
        None => repo.find_tree(tree_id)?.is_empty(),
    };
    if unchanged {
        return Ok(None);
    }

    let mut trailers = vec![(TRAILER_SNAPSHOT, "1".to_string())];
    if let Some((thread_id, turn)) = origin {
        trailers.push((TRAILER_THREAD, one_line(thread_id)));
        trailers.push((TRAILER_TURN, turn.to_string()));
    }

    let oid = commit_tree(&repo, tree_id, head.as_ref(), title, &trailers)?;
    let commit = repo.find_commit(oid)?;
    Ok(Some(
        to_snapshot(&repo, &commit)?.expect("just-written snapshot carries its trailer"),
    ))
}

/// Newest first. At most `limit` snapshots; commits without the snapshot
/// trailer are skipped (not counted). An empty list when the project has no
/// repository or no commits yet.
pub fn list_snapshots(project: &Path, limit: usize) -> Result<Vec<Snapshot>> {
    let Some(repo) = open_project_repo(project)? else {
        return Ok(Vec::new());
    };
    let mut snapshots = Vec::new();
    for_each_snapshot_commit(&repo, |commit| {
        if let Some(snapshot) = to_snapshot(&repo, commit)? {
            snapshots.push(snapshot);
        }
        Ok(snapshots.len() < limit)
    })?;
    Ok(snapshots)
}

/// Makes the project look like `snapshot_id` again, as a new commit (after
/// auto-saving any uncommitted work). Returns that commit.
///
/// Three things are deliberately *not* taken from the target:
/// - `.ibproject/chat/` stays as it is now (chat history is a record, and
///   the conversation about going back must survive going back);
/// - `.ibproject/.ibx` stays as it is now (when HEAD has one), so going
///   back to before InfinaBox was set up never un-makes the project;
/// - a file the target has but which currently exists on disk without
///   being committed (e.g. an ignored local `config.cfg` or `.env`) is left
///   untouched — and left out of the new commit, so history matches disk.
///   Such skipped paths are not reported back to the caller (the
///   `Snapshot` contract has no field for them).
///
/// Only paths that actually differ between HEAD and the restored tree are
/// written, so anything else — notably a chat append that lands while this
/// runs — is never overwritten.
///
/// If nothing would change, no empty commit is made and the current HEAD
/// snapshot is returned.
pub fn restore_to(project: &Path, snapshot_id: &str) -> Result<Snapshot> {
    let _guard = snapshot_lock();
    restore_to_locked(project, snapshot_id)
}

/// `restore_to`, for a caller already holding `SNAPSHOT_LOCK`.
fn restore_to_locked(project: &Path, snapshot_id: &str) -> Result<Snapshot> {
    let repo = open_project_repo(project)?.with_context(|| {
        format!(
            "'{}' has no history yet, so there is nothing to go back to",
            project.display()
        )
    })?;
    ensure_clean_state(&repo)?;
    let target = resolve_commit(&repo, snapshot_id)?;

    // 1. Nothing uncommitted is ever lost: save it as its own snapshot.
    create_snapshot_locked(project, AUTO_SAVE_TITLE, None)
        .context("failed to auto-save uncommitted changes before going back")?;

    let head = head_commit(&repo)?
        .context("the project has no history yet, so there is nothing to go back to")?;
    let head_tree = head.tree()?;
    let target_tree = target.tree().context("failed to read the target's files")?;

    // 2. The tree to go back to: the target's, with today's chat and project
    // marker grafted in and never-committed local files left out.
    let head_entry = |path: &[&str]| {
        head_tree
            .get_path(&path.iter().collect::<PathBuf>())
            .ok()
            .map(|e| (e.id(), e.filemode()))
    };
    let mut restore_id = set_path(&repo, &target_tree, &CHAT_DIR, head_entry(&CHAT_DIR))?;
    if let Some(marker) = head_entry(&PROJECT_MARKER) {
        restore_id = set_path(
            &repo,
            &repo.find_tree(restore_id)?,
            &PROJECT_MARKER,
            Some(marker),
        )?;
    }
    let index = repo.index().context("failed to open the Git index")?;
    for path in uncommitted_files_in_the_way(&repo, &repo.find_tree(restore_id)?, &index)? {
        let parts: Vec<&str> = path.split('/').collect();
        restore_id = set_path(&repo, &repo.find_tree(restore_id)?, &parts, None)?;
    }
    if restore_id == head.tree_id() {
        return snapshot_of_any(&repo, &head);
    }

    // 3. A new commit with that tree, on top of HEAD.
    let title = format!("Went back to: {}", commit_title(&target));
    let trailers = [
        (TRAILER_SNAPSHOT, "1".to_string()),
        (TRAILER_RESTORES, target.id().to_string()),
    ];
    let oid = commit_tree(&repo, restore_id, Some(&head), &title, &trailers)?;

    // 4. Only now, with everything committed, overwrite the working
    // directory — but only the paths that differ between HEAD and the
    // restore tree. Everything else (the chat, in particular) is never
    // touched, so a write that lands between the auto-save and here
    // survives. Every path holding a never-committed file was removed from
    // the restore tree above, so it can't be in this list.
    checkout_changed_paths(&repo, &head_tree, &repo.find_tree(restore_id)?)?;

    let commit = repo.find_commit(oid)?;
    snapshot_of_any(&repo, &commit)
}

/// Restores to the state before the latest snapshot. `None` when there is
/// nothing to undo.
///
/// "Before the latest snapshot" is that snapshot's first parent commit.
/// That one rule covers both cases in the plan: undoing a normal change
/// goes back to how things were before it, and undoing a "Went back to"
/// commit goes back to how things were before *that* — so pressing Undo
/// twice redoes. The latest snapshot is chosen before `restore_to`
/// auto-saves, so an auto-save made by this very call is never what gets
/// undone.
///
/// Snapshots that changed nothing but chat (a turn that only talked) are
/// skipped, since restoring never rewinds chat and undoing one would do
/// nothing.
///
/// If ordinary commits were made after the latest snapshot (e.g. in
/// Advanced mode), undo still restores the latest *snapshot's* parent, so
/// those commits' changes are reverted in the working tree too — they stay
/// in history and can be gone back to like any other commit.
pub fn undo_last(project: &Path) -> Result<Option<Snapshot>> {
    // Held from choosing the target to the end of the restore, so a
    // snapshot made in between can't change which one is "the latest".
    let _guard = snapshot_lock();
    let Some(repo) = open_project_repo(project)? else {
        return Ok(None);
    };
    let mut target = None;
    for_each_snapshot_commit(&repo, |commit| {
        if !is_snapshot(commit) {
            return Ok(true);
        }
        if commit.parent_count() == 0 {
            // The very first snapshot: nothing before it to go back to.
            return Ok(false);
        }
        if changes_outside_chat(&repo, commit)? {
            target = Some(commit.parent_id(0)?);
            return Ok(false);
        }
        Ok(true)
    })?;
    match target {
        Some(parent) => restore_to_locked(project, &parent.to_string()).map(Some),
        None => Ok(None),
    }
}

/// Opens the project's own repository. `None` when the project isn't in a
/// repository at all; an error when it's inside someone else's (InfinaBox
/// must never commit a game's changes into a parent repository, nor quietly
/// nest a second one inside it).
fn open_project_repo(project: &Path) -> Result<Option<Repository>> {
    let repo = match Repository::discover(project) {
        Ok(repo) => repo,
        Err(e) if e.code() == ErrorCode::NotFound => return Ok(None),
        Err(e) => {
            return Err(e).with_context(|| {
                format!("failed to open Git repository at '{}'", project.display())
            });
        }
    };
    let project_dir = project
        .canonicalize()
        .with_context(|| format!("project folder '{}' is not accessible", project.display()))?;
    let workdir = repo.workdir().and_then(|w| w.canonicalize().ok());
    if workdir.as_deref() != Some(project_dir.as_path()) {
        let outer = workdir
            .map(|w| w.display().to_string())
            .unwrap_or_else(|| repo.path().display().to_string());
        anyhow::bail!(
            "This project is inside another git repository ({outer}), so InfinaBox can't keep its own history for it."
        );
    }
    Ok(Some(repo))
}

/// Mid-merge, mid-rebase, mid-cherry-pick etc.: committing or checking out
/// now would tangle InfinaBox's snapshots into an operation someone else
/// (the user or their agent, in a terminal) started.
fn ensure_clean_state(repo: &Repository) -> Result<()> {
    let state = repo.state();
    if state != RepositoryState::Clean {
        anyhow::bail!(
            "The project's git history is in the middle of another operation ({state:?}). Finish or cancel it (e.g. in Advanced mode) before saving or going back."
        );
    }
    Ok(())
}

/// Walks history from HEAD, newest first, calling `visit` on each commit
/// until it returns `false`. Does nothing for an unborn HEAD.
fn for_each_snapshot_commit(
    repo: &Repository,
    mut visit: impl FnMut(&Commit) -> Result<bool>,
) -> Result<()> {
    if head_commit(repo)?.is_none() {
        return Ok(());
    }
    let mut revwalk = repo.revwalk().context("failed to create revwalk")?;
    revwalk
        .push_head()
        .context("failed to start walk from HEAD")?;
    // Same order as `git_indexer::walk_commits`: newest first, topological
    // tie-break for commits made within the same second (common here — an
    // auto-save and its "Went back to" commit land back to back).
    revwalk
        .set_sorting(Sort::TIME | Sort::TOPOLOGICAL)
        .context("failed to set revwalk sort order")?;
    for oid in revwalk {
        let oid = oid.context("failed to read commit id while walking history")?;
        let commit = repo
            .find_commit(oid)
            .with_context(|| format!("failed to look up commit {oid}"))?;
        if !visit(&commit)? {
            break;
        }
    }
    Ok(())
}

/// `None` for an unborn HEAD (fresh repo, no commits yet).
fn head_commit(repo: &Repository) -> Result<Option<Commit<'_>>> {
    match repo.head() {
        Ok(head) => Ok(Some(
            head.peel_to_commit()
                .context("HEAD does not point at a commit")?,
        )),
        Err(e) if matches!(e.code(), ErrorCode::UnbornBranch | ErrorCode::NotFound) => Ok(None),
        Err(e) => Err(e).context("failed to read repository HEAD"),
    }
}

/// `git add -A`: new and modified files (respecting `.gitignore` plus
/// `DEFAULT_IGNORES`) and deletions. Nested git repositories (e.g. an addon
/// cloned into `addons/foo/`) are skipped rather than failing the whole
/// snapshot. Writes the index and returns its tree.
fn stage_everything(repo: &Repository) -> Result<Oid> {
    repo.add_ignore_rule(DEFAULT_IGNORES)
        .context("failed to apply default ignore rules")?;
    let workdir = repo
        .workdir()
        .context("the project's repository has no working folder")?
        .to_path_buf();
    let mut index = repo.index().context("failed to open the Git index")?;
    // Called with file paths (and untracked directory paths): skip the path
    // if it, or any folder above it inside the project, is its own repo —
    // which also covers folders the outer repo already tracked before
    // someone ran `git init`/`git clone` there.
    let mut skip_nested_repos = |path: &Path, _spec: &[u8]| -> i32 {
        let nested = path
            .ancestors()
            .filter(|a| !a.as_os_str().is_empty())
            .any(|a| workdir.join(a).join(".git").exists());
        if nested { 1 } else { 0 }
    };
    index
        .add_all(["*"], IndexAddOption::DEFAULT, Some(&mut skip_nested_repos))
        .context("failed to stage changed files")?;
    index
        .update_all(["*"], Some(&mut skip_nested_repos))
        .context("failed to stage deleted files")?;
    index.write().context("failed to write the Git index")?;
    index
        .write_tree()
        .context("failed to write the staged tree")
}

/// Returns a copy of `base` with the entry at `path` (split into
/// components) replaced by `entry`, or removed when `entry` is `None`.
/// Directories left empty by a removal are dropped, as git would.
fn set_path(
    repo: &Repository,
    base: &Tree,
    path: &[&str],
    entry: Option<(Oid, i32)>,
) -> Result<Oid> {
    match set_path_in(repo, Some(base), path, entry)? {
        Some(id) => Ok(id),
        None => Ok(repo.treebuilder(None)?.write()?),
    }
}

/// `None` when the resulting tree is empty.
fn set_path_in(
    repo: &Repository,
    base: Option<&Tree>,
    path: &[&str],
    entry: Option<(Oid, i32)>,
) -> Result<Option<Oid>> {
    let (name, rest) = path.split_first().context("empty tree path")?;
    let mut builder = repo.treebuilder(base)?;
    let new_entry = if rest.is_empty() {
        entry
    } else {
        let child = match base.and_then(|b| b.get_name(name)) {
            Some(e) if e.kind() == Some(ObjectType::Tree) => Some(repo.find_tree(e.id())?),
            _ => None,
        };
        set_path_in(repo, child.as_ref(), rest, entry)?.map(|id| (id, 0o040000))
    };
    match new_entry {
        Some((id, mode)) => {
            builder.insert(name, id, mode)?;
        }
        None => {
            if builder.get(name)?.is_some() {
                builder.remove(name)?;
            }
        }
    }
    if builder.len() == 0 {
        Ok(None)
    } else {
        Ok(Some(builder.write()?))
    }
}

/// Paths in `tree` whose place on disk is taken by something the index
/// doesn't know about — after the auto-save that can only be an ignored
/// (or nested-repo) file that was never committed, so a checkout would
/// destroy it for good. Also catches an uncommitted *file* sitting where
/// the tree needs a directory.
fn uncommitted_files_in_the_way(
    repo: &Repository,
    tree: &Tree,
    index: &Index,
) -> Result<Vec<String>> {
    let workdir = repo
        .workdir()
        .context("the project's repository has no working folder")?;
    // A path is "in the way" when something on disk there isn't committed.
    // A directory counts only if something inside it isn't in the index; a
    // fully tracked directory is safe for checkout to replace with a file.
    let untracked_on_disk = |rel: &str| {
        if index.get_path(Path::new(rel), 0).is_some() {
            return false;
        }
        match workdir.join(rel).symlink_metadata() {
            Ok(meta) if meta.is_dir() => dir_has_uncommitted(workdir, rel, index),
            Ok(_) => true,
            Err(_) => false,
        }
    };
    let mut blocked = Vec::new();
    tree.walk(TreeWalkMode::PreOrder, |root, entry| {
        if entry.kind() == Some(ObjectType::Tree) {
            return TreeWalkResult::Ok;
        }
        let Ok(name) = entry.name() else {
            return TreeWalkResult::Ok;
        };
        let rel = format!("{root}{name}");
        if untracked_on_disk(&rel) {
            blocked.push(rel);
            return TreeWalkResult::Ok;
        }
        // A never-committed file standing where a parent folder should be.
        let mut prefix = String::new();
        for part in root
            .trim_end_matches('/')
            .split('/')
            .filter(|p| !p.is_empty())
        {
            prefix.push_str(part);
            let is_file = workdir
                .join(&prefix)
                .symlink_metadata()
                .map(|m| !m.is_dir())
                .unwrap_or(false);
            if is_file && index.get_path(Path::new(&prefix), 0).is_none() {
                blocked.push(rel);
                break;
            }
            prefix.push('/');
        }
        TreeWalkResult::Ok
    })
    .context("failed to inspect the snapshot's files")?;
    Ok(blocked)
}

/// Whether any file under the on-disk directory `rel` (recursively, and
/// counting empty-to-git things like symlinks as files) is missing from the
/// index. Unreadable directories count as uncommitted — err on the side of
/// not overwriting.
fn dir_has_uncommitted(workdir: &Path, rel: &str, index: &Index) -> bool {
    let Ok(entries) = std::fs::read_dir(workdir.join(rel)) else {
        return true;
    };
    for entry in entries {
        let Ok(entry) = entry else {
            return true;
        };
        let child = format!("{rel}/{}", entry.file_name().to_string_lossy());
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let uncommitted = if is_dir {
            dir_has_uncommitted(workdir, &child, index)
        } else {
            index.get_path(Path::new(&child), 0).is_none()
        };
        if uncommitted {
            return true;
        }
    }
    false
}

/// Force-checks-out `new` over the working directory, limited to the paths
/// that differ from `old` (the tree the index and disk currently match).
fn checkout_changed_paths(repo: &Repository, old: &Tree, new: &Tree) -> Result<()> {
    let changed = changed_paths(repo, old, new)?;
    // An empty path list would mean "everything" to libgit2.
    if changed.is_empty() {
        return Ok(());
    }
    let mut checkout = CheckoutBuilder::new();
    checkout.force().disable_pathspec_match(true);
    for path in &changed {
        checkout.path(path);
    }
    repo.checkout_tree(new.as_object(), Some(&mut checkout))
        .context("failed to update the project's files to the restored snapshot")
}

/// Every path (old and new side of each delta) that differs between two
/// trees, for a checkout limited to exactly those paths.
fn changed_paths(repo: &Repository, old: &Tree, new: &Tree) -> Result<Vec<PathBuf>> {
    let diff = repo
        .diff_tree_to_tree(Some(old), Some(new), None)
        .context("failed to compare the current and restored files")?;
    let mut paths = Vec::new();
    for delta in diff.deltas() {
        for path in [delta.old_file().path(), delta.new_file().path()]
            .into_iter()
            .flatten()
        {
            if !paths.iter().any(|p: &PathBuf| p == path) {
                paths.push(path.to_path_buf());
            }
        }
    }
    Ok(paths)
}

/// Whether `commit` changed anything besides `.ibproject/chat/` relative to
/// its first parent.
fn changes_outside_chat(repo: &Repository, commit: &Commit) -> Result<bool> {
    Ok(files_changed(repo, commit)? > 0)
}

fn commit_tree(
    repo: &Repository,
    tree_id: Oid,
    parent: Option<&Commit>,
    title: &str,
    trailers: &[(&str, String)],
) -> Result<Oid> {
    let tree = repo.find_tree(tree_id)?;
    let sig = signature(repo)?;
    let mut message = one_line(title);
    if message.is_empty() {
        message = "Snapshot".to_string();
    }
    message.push_str("\n\n");
    for (key, value) in trailers {
        message.push_str(&format!("{key}: {value}\n"));
    }
    let parents: Vec<&Commit> = parent.into_iter().collect();
    repo.commit(Some("HEAD"), &sig, &sig, &message, &tree, &parents)
        .context("failed to create snapshot commit")
}

/// The repo/user identity from git config when both name and email are set,
/// else `InfinaBox <infinabox@localhost>`. Reads config only.
fn signature(repo: &Repository) -> Result<Signature<'static>> {
    let config = repo.config().context("failed to read git config")?;
    let (name, email) = identity_from_config(&config);
    Signature::now(&name, &email).context("failed to build commit signature")
}

fn identity_from_config(config: &Config) -> (String, String) {
    let get = |key: &str| {
        config
            .get_string(key)
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    match (get("user.name"), get("user.email")) {
        (Some(name), Some(email)) => (name, email),
        _ => (FALLBACK_NAME.to_string(), FALLBACK_EMAIL.to_string()),
    }
}

fn resolve_commit<'r>(repo: &'r Repository, id: &str) -> Result<Commit<'r>> {
    repo.revparse_single(id)
        .and_then(|obj| obj.peel_to_commit())
        .with_context(|| format!("no snapshot '{id}' in this project's history"))
}

fn commit_title(commit: &Commit) -> String {
    commit
        .summary()
        .ok()
        .flatten()
        .unwrap_or("<invalid utf-8>")
        .to_string()
}

fn is_snapshot(commit: &Commit) -> bool {
    parse_trailers(commit.message().unwrap_or(""))
        .iter()
        .any(|(k, v)| k == TRAILER_SNAPSHOT && v == "1")
}

/// Parses a commit into a `Snapshot`, or `None` when it doesn't carry the
/// snapshot trailer.
fn to_snapshot(repo: &Repository, commit: &Commit) -> Result<Option<Snapshot>> {
    if !is_snapshot(commit) {
        return Ok(None);
    }
    let trailers = parse_trailers(commit.message().unwrap_or(""));
    Ok(Some(build_snapshot(repo, commit, &trailers)?))
}

/// Like `to_snapshot` but for any commit (e.g. a HEAD that was made outside
/// InfinaBox), so `restore_to` always has something real to return.
fn snapshot_of_any(repo: &Repository, commit: &Commit) -> Result<Snapshot> {
    let trailers = parse_trailers(commit.message().unwrap_or(""));
    build_snapshot(repo, commit, &trailers)
}

fn build_snapshot(
    repo: &Repository,
    commit: &Commit,
    trailers: &[(String, String)],
) -> Result<Snapshot> {
    let find = |key: &str| {
        trailers
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    };
    Ok(Snapshot {
        id: commit.id().to_string(),
        title: commit_title(commit),
        timestamp: commit.time().seconds(),
        thread_id: find(TRAILER_THREAD),
        turn: find(TRAILER_TURN).and_then(|t| t.parse().ok()),
        files_changed: files_changed(repo, commit)?,
    })
}

/// Trailers from the message's last paragraph, via libgit2's own parser
/// (same rules as `git interpret-trailers`).
fn parse_trailers(message: &str) -> Vec<(String, String)> {
    git2::message_trailers_strs(message)
        .map(|t| {
            t.iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Files differing between the commit and its first parent (or an empty
/// tree, for a root commit) — the same diff `git_indexer` uses, counted.
/// Chat threads under `.ibproject/chat/` are committed but not counted:
/// this number is shown to users as "N files changed" and means their
/// game's files.
fn files_changed(repo: &Repository, commit: &Commit) -> Result<usize> {
    let new_tree = commit.tree().context("failed to get commit tree")?;
    let old_tree = if commit.parent_count() > 0 {
        Some(
            commit
                .parent(0)?
                .tree()
                .context("failed to get parent tree")?,
        )
    } else {
        None
    };
    let diff = repo
        .diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), None)
        .context("failed to compute tree-to-tree diff")?;
    let chat_prefix = Path::new(CHAT_DIR[0]).join(CHAT_DIR[1]);
    Ok(diff
        .deltas()
        .filter(|d| {
            let path = d.new_file().path().or_else(|| d.old_file().path());
            !path.is_some_and(|p| p.starts_with(&chat_prefix))
        })
        .count())
}

/// Titles and trailer values must stay on one line, or they'd break the
/// message's summary/trailer structure.
fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write(dir: &Path, rel: &str, content: &str) {
        let path = dir.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn read(dir: &Path, rel: &str) -> String {
        fs::read_to_string(dir.join(rel)).unwrap()
    }

    fn commit_count(dir: &Path) -> usize {
        let repo = Repository::open(dir).unwrap();
        let mut walk = repo.revwalk().unwrap();
        walk.push_head().unwrap();
        walk.count()
    }

    #[test]
    fn create_initializes_repo_and_list_returns_it() {
        let dir = TempDir::new().unwrap();
        assert!(list_snapshots(dir.path(), 10).unwrap().is_empty());

        write(dir.path(), "player.gd", "extends Node\n");
        write(dir.path(), "scenes/main.tscn", "[gd_scene]\n");
        let snap = create_snapshot(dir.path(), "Add the player", None)
            .unwrap()
            .expect("files were added");
        assert!(dir.path().join(".git").exists());
        assert_eq!(snap.title, "Add the player");
        assert_eq!(snap.files_changed, 2);
        assert_eq!(snap.thread_id, None);
        assert_eq!(snap.turn, None);

        let listed = list_snapshots(dir.path(), 10).unwrap();
        assert_eq!(listed, vec![snap]);
    }

    #[test]
    fn empty_project_and_unchanged_tree_are_no_ops() {
        let dir = TempDir::new().unwrap();
        assert_eq!(create_snapshot(dir.path(), "Nothing", None).unwrap(), None);

        write(dir.path(), "a.txt", "a");
        create_snapshot(dir.path(), "First", None).unwrap().unwrap();
        assert_eq!(create_snapshot(dir.path(), "Again", None).unwrap(), None);
        assert_eq!(commit_count(dir.path()), 1);
    }

    #[test]
    fn respects_gitignore_and_stages_deletions() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), ".gitignore", ".godot/\n");
        write(dir.path(), ".godot/cache.bin", "cache");
        write(dir.path(), "a.txt", "a");
        write(dir.path(), "b.txt", "b");
        let first = create_snapshot(dir.path(), "First", None).unwrap().unwrap();
        assert_eq!(first.files_changed, 3, ".gitignore, a.txt, b.txt only");

        // Only ignored content changed: nothing to snapshot.
        write(dir.path(), ".godot/cache.bin", "changed");
        assert_eq!(create_snapshot(dir.path(), "Ignored", None).unwrap(), None);

        fs::remove_file(dir.path().join("b.txt")).unwrap();
        let second = create_snapshot(dir.path(), "Remove b", None)
            .unwrap()
            .unwrap();
        assert_eq!(second.files_changed, 1);
        let repo = Repository::open(dir.path()).unwrap();
        let tree = repo.head().unwrap().peel_to_tree().unwrap();
        assert!(tree.get_name("b.txt").is_none());
    }

    #[test]
    fn trailers_are_written_and_parsed() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "a.txt", "a");
        let snap = create_snapshot(
            dir.path(),
            "Make the jump higher",
            Some(("20260925T142233123-a1b2c3d4", 7)),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            snap.thread_id.as_deref(),
            Some("20260925T142233123-a1b2c3d4")
        );
        assert_eq!(snap.turn, Some(7));

        let repo = Repository::open(dir.path()).unwrap();
        let message = repo
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .message()
            .unwrap()
            .to_string();
        assert_eq!(
            message,
            "Make the jump higher\n\nInfinaBox-Snapshot: 1\nInfinaBox-Thread: 20260925T142233123-a1b2c3d4\nInfinaBox-Turn: 7\n"
        );
        assert_eq!(list_snapshots(dir.path(), 5).unwrap()[0], snap);
    }

    #[test]
    fn list_skips_non_snapshot_commits_and_honors_limit() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "a.txt", "1");
        create_snapshot(dir.path(), "One", None).unwrap().unwrap();

        // A plain commit made outside InfinaBox (e.g. Advanced mode).
        write(dir.path(), "a.txt", "2");
        let repo = Repository::open(dir.path()).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("a.txt")).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        let sig = Signature::now("Someone", "someone@example.com").unwrap();
        repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            "Manual commit\n\nInfinaBox-Snapshot: 0\n",
            &tree,
            &[&head],
        )
        .unwrap();

        write(dir.path(), "a.txt", "3");
        create_snapshot(dir.path(), "Three", None).unwrap().unwrap();

        let titles: Vec<_> = list_snapshots(dir.path(), 10)
            .unwrap()
            .into_iter()
            .map(|s| s.title)
            .collect();
        assert_eq!(titles, ["Three", "One"]);
        assert_eq!(list_snapshots(dir.path(), 1).unwrap().len(), 1);
    }

    #[test]
    fn restore_keeps_later_history_and_restores_content_exactly() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "player.gd", "var speed = 10\n");
        let v1 = create_snapshot(dir.path(), "Add player", None)
            .unwrap()
            .unwrap();

        write(dir.path(), "player.gd", "var speed = 20\n");
        write(dir.path(), "enemy.gd", "extends Node\n");
        write(dir.path(), "levels/one.tscn", "[gd_scene]\n");
        let v2 = create_snapshot(dir.path(), "Faster player, add enemy", None)
            .unwrap()
            .unwrap();

        let restored = restore_to(dir.path(), &v1.id).unwrap();
        assert_eq!(restored.title, "Went back to: Add player");
        assert_eq!(read(dir.path(), "player.gd"), "var speed = 10\n");
        assert!(!dir.path().join("enemy.gd").exists());
        assert!(!dir.path().join("levels/one.tscn").exists());

        // History kept: the restore sits on top of v2, nothing rewritten.
        let repo = Repository::open(dir.path()).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head.id().to_string(), restored.id);
        assert_eq!(head.parent(0).unwrap().id().to_string(), v2.id);
        assert_eq!(
            head.tree_id(),
            repo.find_commit(Oid::from_str(&v1.id).unwrap())
                .unwrap()
                .tree_id()
        );
        assert!(
            head.message()
                .unwrap()
                .contains(&format!("InfinaBox-Restores: {}", v1.id))
        );
        let ids: Vec<_> = list_snapshots(dir.path(), 10)
            .unwrap()
            .into_iter()
            .map(|s| s.id)
            .collect();
        assert_eq!(ids, [restored.id.clone(), v2.id.clone(), v1.id.clone()]);

        // Working directory is clean against the new HEAD.
        assert_eq!(create_snapshot(dir.path(), "Nothing", None).unwrap(), None);

        // And v2 is still reachable to go forward again.
        restore_to(dir.path(), &v2.id).unwrap();
        assert_eq!(read(dir.path(), "player.gd"), "var speed = 20\n");
        assert_eq!(read(dir.path(), "enemy.gd"), "extends Node\n");
    }

    #[test]
    fn restore_auto_saves_uncommitted_work_first() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "notes.md", "v1\n");
        let v1 = create_snapshot(dir.path(), "First", None).unwrap().unwrap();
        write(dir.path(), "notes.md", "v2\n");
        create_snapshot(dir.path(), "Second", None)
            .unwrap()
            .unwrap();

        // Uncommitted edits, including a brand-new file.
        write(dir.path(), "notes.md", "unsaved work\n");
        write(dir.path(), "new.md", "also unsaved\n");

        restore_to(dir.path(), &v1.id).unwrap();
        assert_eq!(read(dir.path(), "notes.md"), "v1\n");
        assert!(!dir.path().join("new.md").exists());

        let list = list_snapshots(dir.path(), 10).unwrap();
        let titles: Vec<_> = list.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(
            titles,
            ["Went back to: First", AUTO_SAVE_TITLE, "Second", "First"]
        );

        // The unsaved work is intact in the auto-save commit.
        let repo = Repository::open(dir.path()).unwrap();
        let auto = repo
            .find_commit(Oid::from_str(&list[1].id).unwrap())
            .unwrap();
        let tree = auto.tree().unwrap();
        let blob = |name: &str| {
            let entry = tree.get_name(name).unwrap();
            String::from_utf8(repo.find_blob(entry.id()).unwrap().content().to_vec()).unwrap()
        };
        assert_eq!(blob("notes.md"), "unsaved work\n");
        assert_eq!(blob("new.md"), "also unsaved\n");
    }

    #[test]
    fn restore_to_current_state_makes_no_empty_commit() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "a.txt", "a");
        let v1 = create_snapshot(dir.path(), "Only", None).unwrap().unwrap();
        let again = restore_to(dir.path(), &v1.id).unwrap();
        assert_eq!(again, v1);
        assert_eq!(commit_count(dir.path()), 1);
        assert!(restore_to(dir.path(), "0000000000000000000000000000000000000000").is_err());
    }

    #[test]
    fn undo_twice_redoes() {
        let dir = TempDir::new().unwrap();
        assert_eq!(
            undo_last(dir.path()).unwrap(),
            None,
            "no repo, nothing to undo"
        );

        write(dir.path(), "player.gd", "var speed = 10\n");
        create_snapshot(dir.path(), "Add player", Some(("t1", 1)))
            .unwrap()
            .unwrap();
        assert_eq!(
            undo_last(dir.path()).unwrap(),
            None,
            "first snapshot has no parent"
        );

        write(dir.path(), "player.gd", "var speed = 99\n");
        create_snapshot(dir.path(), "Make player fast", Some(("t1", 2)))
            .unwrap()
            .unwrap();

        let undo = undo_last(dir.path()).unwrap().unwrap();
        assert_eq!(undo.title, "Went back to: Add player");
        assert_eq!(read(dir.path(), "player.gd"), "var speed = 10\n");

        let redo = undo_last(dir.path()).unwrap().unwrap();
        assert_eq!(redo.title, "Went back to: Make player fast");
        assert_eq!(read(dir.path(), "player.gd"), "var speed = 99\n");

        // Toggles consistently, and every step is a new commit.
        undo_last(dir.path()).unwrap().unwrap();
        assert_eq!(read(dir.path(), "player.gd"), "var speed = 10\n");
        assert_eq!(commit_count(dir.path()), 5);
    }

    #[test]
    fn undo_with_unsaved_edits_undoes_the_last_snapshot_and_keeps_the_edits_in_history() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "a.txt", "one");
        create_snapshot(dir.path(), "One", None).unwrap().unwrap();
        write(dir.path(), "a.txt", "two");
        create_snapshot(dir.path(), "Two", None).unwrap().unwrap();
        write(dir.path(), "a.txt", "unsaved");

        let undo = undo_last(dir.path()).unwrap().unwrap();
        assert_eq!(undo.title, "Went back to: One");
        assert_eq!(read(dir.path(), "a.txt"), "one");
        let titles: Vec<_> = list_snapshots(dir.path(), 10)
            .unwrap()
            .into_iter()
            .map(|s| s.title)
            .collect();
        assert_eq!(titles, ["Went back to: One", AUTO_SAVE_TITLE, "Two", "One"]);
    }

    #[test]
    fn signature_uses_repo_identity_when_configured() {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        // Test setup only: the module itself never writes config.
        let mut cfg = repo
            .config()
            .unwrap()
            .open_level(git2::ConfigLevel::Local)
            .unwrap();
        cfg.set_str("user.name", "Ada Dev").unwrap();
        cfg.set_str("user.email", "ada@example.com").unwrap();

        write(dir.path(), "a.txt", "a");
        create_snapshot(dir.path(), "First", None).unwrap().unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head.author().name().ok(), Some("Ada Dev"));
        assert_eq!(head.author().email().ok(), Some("ada@example.com"));
        assert_eq!(head.committer().name().ok(), Some("Ada Dev"));
    }

    #[test]
    fn identity_falls_back_without_config() {
        // A config holding only an empty file: no identity anywhere.
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config");
        fs::write(&path, "").unwrap();
        let empty = Config::open(&path).unwrap();
        assert_eq!(
            identity_from_config(&empty),
            (FALLBACK_NAME.to_string(), FALLBACK_EMAIL.to_string())
        );

        // Name without email is incomplete, so it also falls back.
        fs::write(&path, "[user]\n\tname = Only Name\n").unwrap();
        let partial = Config::open(&path).unwrap();
        assert_eq!(identity_from_config(&partial).1, FALLBACK_EMAIL);

        fs::write(&path, "[user]\n\tname = Ada\n\temail = ada@example.com\n").unwrap();
        let full = Config::open(&path).unwrap();
        assert_eq!(
            identity_from_config(&full),
            ("Ada".to_string(), "ada@example.com".to_string())
        );
    }

    #[test]
    fn multiline_titles_are_flattened() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "a.txt", "a");
        let snap = create_snapshot(dir.path(), "Line one\nInfinaBox-Turn: 99", None)
            .unwrap()
            .unwrap();
        assert_eq!(snap.title, "Line one InfinaBox-Turn: 99");
        assert_eq!(snap.turn, None);
    }

    // ---- Review regressions -------------------------------------------

    fn user(text: &str, at: i64) -> crate::chat_store::ChatRecord {
        crate::chat_store::ChatRecord::User {
            text: text.into(),
            at,
        }
    }

    #[test]
    fn going_back_never_rewinds_the_chat() {
        use crate::chat_store::{append, create_thread, list_threads, load_thread};
        let dir = TempDir::new().unwrap();
        write(dir.path(), "player.gd", "var speed = 1\n");
        create_snapshot(dir.path(), "Start", None).unwrap().unwrap();

        let thread = create_thread(dir.path(), "Speed", "claude-code").unwrap();
        append(dir.path(), &thread.id, &user("make it 2", 1)).unwrap();
        write(dir.path(), "player.gd", "var speed = 2\n");
        create_snapshot(dir.path(), "make it 2", Some((&thread.id, 1)))
            .unwrap()
            .unwrap();

        // Talk more after the snapshot (uncommitted chat), then undo.
        append(dir.path(), &thread.id, &user("undo that", 2)).unwrap();
        undo_last(dir.path()).unwrap().unwrap();
        assert_eq!(read(dir.path(), "player.gd"), "var speed = 1\n");
        let (_, records) = load_thread(dir.path(), &thread.id).unwrap();
        assert_eq!(records, vec![user("make it 2", 1), user("undo that", 2)]);
        assert_eq!(list_threads(dir.path()).unwrap().len(), 1);

        // Redo, and an explicit restore to before the thread existed.
        append(dir.path(), &thread.id, &user("redo", 3)).unwrap();
        undo_last(dir.path()).unwrap().unwrap();
        assert_eq!(read(dir.path(), "player.gd"), "var speed = 2\n");
        let first = list_snapshots(dir.path(), 100).unwrap().pop().unwrap();
        assert_eq!(first.title, "Start");
        append(dir.path(), &thread.id, &user("back to start", 4)).unwrap();
        restore_to(dir.path(), &first.id).unwrap();
        assert_eq!(read(dir.path(), "player.gd"), "var speed = 1\n");
        let (_, records) = load_thread(dir.path(), &thread.id).unwrap();
        assert_eq!(records.len(), 4);
        assert_eq!(records[3], user("back to start", 4));

        // The committed tree carries the chat too, so disk and HEAD agree.
        assert_eq!(create_snapshot(dir.path(), "Nothing", None).unwrap(), None);
    }

    #[test]
    fn files_changed_excludes_chat_but_chat_is_still_committed() {
        use crate::chat_store::{append, create_thread};
        let dir = TempDir::new().unwrap();
        write(dir.path(), "player.gd", "var speed = 1\n");
        let thread = create_thread(dir.path(), "Speed", "claude-code").unwrap();
        append(dir.path(), &thread.id, &user("add a player", 1)).unwrap();
        let first = create_snapshot(dir.path(), "Add player", Some((&thread.id, 1)))
            .unwrap()
            .unwrap();
        assert_eq!(first.files_changed, 1, "player.gd only, not the chat file");

        append(dir.path(), &thread.id, &user("make it 2", 2)).unwrap();
        write(dir.path(), "player.gd", "var speed = 2\n");
        let second = create_snapshot(dir.path(), "make it 2", Some((&thread.id, 2)))
            .unwrap()
            .unwrap();
        assert_eq!(second.files_changed, 1);
        assert_eq!(list_snapshots(dir.path(), 10).unwrap()[0], second);

        let repo = Repository::open(dir.path()).unwrap();
        let tree = repo.head().unwrap().peel_to_tree().unwrap();
        let chat_file = format!(".ibproject/chat/{}.jsonl", thread.id);
        assert!(
            tree.get_path(Path::new(&chat_file)).is_ok(),
            "chat is committed"
        );
    }

    #[test]
    fn undo_skips_chat_only_snapshots() {
        use crate::chat_store::{append, create_thread};
        let dir = TempDir::new().unwrap();
        write(dir.path(), "a.txt", "1");
        create_snapshot(dir.path(), "One", None).unwrap().unwrap();
        write(dir.path(), "a.txt", "2");
        create_snapshot(dir.path(), "Two", None).unwrap().unwrap();
        let thread = create_thread(dir.path(), "Just talking", "claude-code").unwrap();
        append(dir.path(), &thread.id, &user("hi", 1)).unwrap();
        let chat_only = create_snapshot(dir.path(), "Chat only", Some((&thread.id, 1)))
            .unwrap()
            .unwrap();
        assert_eq!(chat_only.files_changed, 0);

        let undo = undo_last(dir.path()).unwrap().unwrap();
        assert_eq!(undo.title, "Went back to: One");
        assert_eq!(read(dir.path(), "a.txt"), "1");
        assert!(
            dir.path()
                .join(format!(".ibproject/chat/{}.jsonl", thread.id))
                .exists()
        );
    }

    #[test]
    fn nested_git_repositories_are_skipped_not_fatal() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "main.gd", "extends Node\n");
        // An addon cloned with git inside the project.
        let addon = dir.path().join("addons/foo");
        fs::create_dir_all(&addon).unwrap();
        Repository::init(&addon).unwrap();
        write(dir.path(), "addons/foo/plugin.gd", "extends EditorPlugin\n");

        let snap = create_snapshot(dir.path(), "With addon", None)
            .unwrap()
            .unwrap();
        assert_eq!(snap.files_changed, 1);
        let repo = Repository::open(dir.path()).unwrap();
        let tree = repo.head().unwrap().peel_to_tree().unwrap();
        assert!(tree.get_name("main.gd").is_some());
        assert!(tree.get_path(Path::new("addons/foo")).is_err());

        write(dir.path(), "main.gd", "extends Node2D\n");
        assert!(
            create_snapshot(dir.path(), "Again", None)
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn going_back_never_clobbers_an_ignored_local_file() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "config.cfg", "committed defaults\n");
        write(dir.path(), "game.gd", "v1\n");
        let v1 = create_snapshot(dir.path(), "v1", None).unwrap().unwrap();

        fs::remove_file(dir.path().join("config.cfg")).unwrap();
        write(dir.path(), ".gitignore", "config.cfg\n");
        write(dir.path(), "game.gd", "v2\n");
        create_snapshot(dir.path(), "v2 ignores config", None)
            .unwrap()
            .unwrap();

        write(dir.path(), "config.cfg", "my local settings, never saved\n");
        restore_to(dir.path(), &v1.id).unwrap();
        assert_eq!(read(dir.path(), "game.gd"), "v1\n");
        assert_eq!(
            read(dir.path(), "config.cfg"),
            "my local settings, never saved\n"
        );

        // It was never force-added, and the commit matches what's on disk.
        let repo = Repository::open(dir.path()).unwrap();
        let tree = repo.head().unwrap().peel_to_tree().unwrap();
        assert!(tree.get_name("config.cfg").is_none());
        assert!(
            tree.get_name(".gitignore").is_none(),
            "v1 had no .gitignore"
        );
    }

    #[test]
    fn a_project_inside_another_repository_is_refused() {
        let outer = TempDir::new().unwrap();
        Repository::init(outer.path()).unwrap();
        let project = outer.path().join("games/my-game");
        write(&project, "main.gd", "extends Node\n");

        let err = create_snapshot(&project, "First", None)
            .unwrap_err()
            .to_string();
        assert!(err.contains("inside another git repository"), "{err}");
        assert!(!project.join(".git").exists(), "no nested repo was created");
        assert!(list_snapshots(&project, 10).is_err());
        assert!(undo_last(&project).is_err());
    }

    #[test]
    fn refuses_to_act_mid_merge() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "a.txt", "1");
        let v1 = create_snapshot(dir.path(), "One", None).unwrap().unwrap();
        write(dir.path(), "a.txt", "2");
        create_snapshot(dir.path(), "Two", None).unwrap().unwrap();

        // What `git merge` leaves behind while a conflict is unresolved.
        fs::write(dir.path().join(".git/MERGE_HEAD"), format!("{}\n", v1.id)).unwrap();
        write(dir.path(), "a.txt", "conflicted");
        assert_eq!(
            Repository::open(dir.path()).unwrap().state(),
            RepositoryState::Merge
        );
        let err = create_snapshot(dir.path(), "Three", None)
            .unwrap_err()
            .to_string();
        assert!(err.contains("middle of another operation"), "{err}");
        assert!(restore_to(dir.path(), &v1.id).is_err());
        assert!(undo_last(dir.path()).is_err());
        assert_eq!(
            read(dir.path(), "a.txt"),
            "conflicted",
            "nothing was touched"
        );
        assert_eq!(commit_count(dir.path()), 2);
    }

    #[test]
    fn default_ignores_apply_without_a_gitignore() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "project.godot", "[application]\n");
        write(dir.path(), ".godot/editor/cache.cfg", "cache");
        write(dir.path(), ".import/icon.png-abc.stex", "import");
        write(dir.path(), ".env", "STRIPE_SECRET_KEY=sk_live_abc\n");
        write(dir.path(), ".DS_Store", "junk");
        write(dir.path(), ".ibproject/chat/t.jsonl.deadbeef.tmp", "temp");

        let snap = create_snapshot(dir.path(), "First", None).unwrap().unwrap();
        assert_eq!(snap.files_changed, 1);
        let repo = Repository::open(dir.path()).unwrap();
        let tree = repo.head().unwrap().peel_to_tree().unwrap();
        assert!(tree.get_name("project.godot").is_some());
        for ignored in [".godot", ".import", ".env", ".DS_Store", ".ibproject"] {
            assert!(tree.get_name(ignored).is_none(), "{ignored} was committed");
        }
        assert!(
            !dir.path().join(".gitignore").exists(),
            "nothing written to disk"
        );
    }

    // ---- Re-review regressions ----------------------------------------

    fn manual_commit(dir: &Path, message: &str) {
        let repo = Repository::open(dir).unwrap();
        let mut index = repo.index().unwrap();
        index.add_all(["*"], IndexAddOption::DEFAULT, None).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = Signature::now("Dev", "dev@example.com").unwrap();
        let parent = repo.head().ok().map(|h| h.peel_to_commit().unwrap());
        let parents: Vec<&Commit> = parent.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
            .unwrap();
    }

    #[test]
    fn a_tracked_folder_can_be_replaced_by_a_file_again() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "levels", "v1 level list\n");
        let v1 = create_snapshot(dir.path(), "v1", None).unwrap().unwrap();
        fs::remove_file(dir.path().join("levels")).unwrap();
        write(dir.path(), "levels/a.txt", "level a\n");
        create_snapshot(dir.path(), "v2", None).unwrap().unwrap();

        restore_to(dir.path(), &v1.id).unwrap();
        assert_eq!(read(dir.path(), "levels"), "v1 level list\n");
        let repo = Repository::open(dir.path()).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(
            head.tree_id(),
            repo.find_commit(Oid::from_str(&v1.id).unwrap())
                .unwrap()
                .tree_id()
        );
    }

    #[test]
    fn a_folder_holding_an_uncommitted_file_is_not_replaced() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "levels", "v1 level list\n");
        write(dir.path(), "game.gd", "v1\n");
        let v1 = create_snapshot(dir.path(), "v1", None).unwrap().unwrap();
        fs::remove_file(dir.path().join("levels")).unwrap();
        write(dir.path(), "levels/a.txt", "level a\n");
        write(dir.path(), "game.gd", "v2\n");
        create_snapshot(dir.path(), "v2", None).unwrap().unwrap();
        // Ignored by the default `*.tmp` rule, so never committed.
        write(dir.path(), "levels/scratch.tmp", "local only\n");

        restore_to(dir.path(), &v1.id).unwrap();
        assert_eq!(read(dir.path(), "game.gd"), "v1\n");
        assert_eq!(read(dir.path(), "levels/scratch.tmp"), "local only\n");
        assert!(dir.path().join("levels").is_dir());
    }

    #[test]
    fn checkout_only_touches_paths_that_change() {
        use crate::chat_store::{append, create_thread, load_thread};
        let dir = TempDir::new().unwrap();
        write(dir.path(), "player.gd", "var speed = 1\n");
        let thread = create_thread(dir.path(), "Speed", "claude-code").unwrap();
        append(dir.path(), &thread.id, &user("first", 1)).unwrap();
        create_snapshot(dir.path(), "Start", None).unwrap().unwrap();

        // A chat append landing after the auto-save, before the checkout.
        append(dir.path(), &thread.id, &user("late", 2)).unwrap();

        let repo = Repository::open(dir.path()).unwrap();
        let head_tree = repo.head().unwrap().peel_to_tree().unwrap();
        let blob = repo.blob(b"var speed = 9\n").unwrap();
        let new_id = set_path(&repo, &head_tree, &["player.gd"], Some((blob, 0o100644))).unwrap();
        checkout_changed_paths(&repo, &head_tree, &repo.find_tree(new_id).unwrap()).unwrap();

        assert_eq!(read(dir.path(), "player.gd"), "var speed = 9\n");
        let (_, records) = load_thread(dir.path(), &thread.id).unwrap();
        assert_eq!(records, vec![user("first", 1), user("late", 2)]);
    }

    #[test]
    fn undoing_the_first_turn_keeps_the_project_marker() {
        let dir = TempDir::new().unwrap();
        // A game that already had git history before InfinaBox opened it.
        Repository::init(dir.path()).unwrap();
        write(dir.path(), "main.gd", "extends Node\n");
        manual_commit(dir.path(), "Initial commit");

        write(dir.path(), ".ibproject/.ibx", "{\"version\":1}\n");
        write(
            dir.path(),
            ".ibproject/context/concept.md",
            "# A platformer\n",
        );
        write(dir.path(), "main.gd", "extends Node2D\n");
        create_snapshot(dir.path(), "First AI turn", Some(("t", 1)))
            .unwrap()
            .unwrap();

        let undo = undo_last(dir.path()).unwrap().unwrap();
        assert_eq!(undo.title, "Went back to: Initial commit");
        assert_eq!(read(dir.path(), "main.gd"), "extends Node\n");
        assert_eq!(read(dir.path(), ".ibproject/.ibx"), "{\"version\":1}\n");
        assert!(
            !dir.path().join(".ibproject/context/concept.md").exists(),
            "context stays restorable"
        );
        let repo = Repository::open(dir.path()).unwrap();
        let tree = repo.head().unwrap().peel_to_tree().unwrap();
        assert!(tree.get_path(Path::new(".ibproject/.ibx")).is_ok());
    }

    #[test]
    fn folders_turned_into_repos_after_being_tracked_are_skipped() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "main.gd", "extends Node\n");
        write(dir.path(), "addons/foo/plugin.gd", "v1\n");
        create_snapshot(dir.path(), "First", None).unwrap().unwrap();

        // Someone runs `git init` / clones over the already-tracked addon.
        Repository::init(dir.path().join("addons/foo")).unwrap();
        write(dir.path(), "addons/foo/plugin.gd", "v2 from upstream\n");
        write(dir.path(), "addons/foo/extra.gd", "new\n");
        write(dir.path(), "main.gd", "extends Node2D\n");

        let snap = create_snapshot(dir.path(), "Second", None)
            .unwrap()
            .unwrap();
        assert_eq!(snap.files_changed, 1, "main.gd only");
        let repo = Repository::open(dir.path()).unwrap();
        let tree = repo.head().unwrap().peel_to_tree().unwrap();
        assert!(tree.get_path(Path::new("addons/foo/extra.gd")).is_err());
        let entry = tree.get_path(Path::new("addons/foo/plugin.gd")).unwrap();
        assert_eq!(repo.find_blob(entry.id()).unwrap().content(), b"v1\n");
    }

    /// The race the lock exists for: an AI turn's snapshot landing between
    /// a restore's commit and its checkout would commit the pre-restore
    /// `game.gd` on top of the "Went back to" commit, silently undoing it
    /// under the turn's title. With snapshots and restores racing from two
    /// threads, no turn commit may ever touch `game.gd` (the turn thread only
    /// writes chat files, which restores keep), every restore commit must
    /// hold its target's `game.gd`, and the disk must end matching HEAD.
    #[test]
    fn concurrent_snapshots_and_restores_keep_history_consistent() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let dir = TempDir::new().unwrap();
        write(dir.path(), "game.gd", "v1\n");
        let a = create_snapshot(dir.path(), "A", None).unwrap().unwrap();
        write(dir.path(), "game.gd", "v2\n");
        let b = create_snapshot(dir.path(), "B", None).unwrap().unwrap();

        let project = dir.path().to_path_buf();
        let done = Arc::new(AtomicBool::new(false));
        let turns = {
            let project = project.clone();
            let done = done.clone();
            std::thread::spawn(move || {
                let mut i = 0;
                while !done.load(Ordering::SeqCst) {
                    write(&project, &format!(".ibproject/chat/turn-{i}.jsonl"), "{}\n");
                    create_snapshot(&project, &format!("Turn {i}"), Some(("t", i))).unwrap();
                    i += 1;
                }
            })
        };
        for round in 0..40 {
            let target = if round % 2 == 0 { &a.id } else { &b.id };
            restore_to(&project, target).unwrap();
        }
        done.store(true, Ordering::SeqCst);
        turns.join().unwrap();
        create_snapshot(&project, "Last turn", None).unwrap();

        let repo = Repository::open(&project).unwrap();
        let game = |commit: &Commit| -> Vec<u8> {
            let entry = commit
                .tree()
                .unwrap()
                .get_path(Path::new("game.gd"))
                .unwrap();
            repo.find_blob(entry.id()).unwrap().content().to_vec()
        };
        let mut walk = repo.revwalk().unwrap();
        walk.push_head().unwrap();
        let (mut restores, mut turn_commits) = (0, 0);
        for oid in walk {
            let commit = repo.find_commit(oid.unwrap()).unwrap();
            let title = commit_title(&commit);
            let restored = parse_trailers(commit.message().unwrap())
                .into_iter()
                .find(|(k, _)| k == TRAILER_RESTORES);
            if let Some((_, target)) = restored {
                restores += 1;
                let target = repo.find_commit(Oid::from_str(&target).unwrap()).unwrap();
                assert_eq!(game(&commit), game(&target), "{title}");
            } else if title.starts_with("Turn ") || title == "Last turn" {
                turn_commits += 1;
                let parent = commit.parent(0).unwrap();
                assert_eq!(game(&commit), game(&parent), "{title} changed game.gd");
            }
        }
        assert_eq!(restores, 40);
        assert!(turn_commits > 0);
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(read(&project, "game.gd").as_bytes(), game(&head).as_slice());
    }
}
