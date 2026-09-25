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
//! Never writes git config; never resets or rewrites history. The only
//! working-directory overwrite (`restore_to`) happens after every
//! uncommitted change has been saved into its own snapshot first.

use std::path::Path;

use anyhow::{Context, Result};
use git2::{
    Commit, Config, ErrorCode, IndexAddOption, Oid, Repository, Signature, Sort,
    build::CheckoutBuilder,
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

/// Commits everything (respecting `.gitignore`). `None` when nothing
/// changed. `origin` is the (thread id, turn number) that produced it.
/// Initializes the repository if the project isn't one yet.
pub fn create_snapshot(
    project: &Path,
    title: &str,
    origin: Option<(&str, u32)>,
) -> Result<Option<Snapshot>> {
    let repo = open_or_init(project)?;
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
    let repo = match Repository::open(project) {
        Ok(repo) => repo,
        Err(e) if e.code() == ErrorCode::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(e).with_context(|| {
                format!("failed to open Git repository at '{}'", project.display())
            });
        }
    };
    if head_commit(&repo)?.is_none() {
        return Ok(Vec::new());
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

    let mut snapshots = Vec::new();
    for oid in revwalk {
        if snapshots.len() >= limit {
            break;
        }
        let oid = oid.context("failed to read commit id while walking history")?;
        let commit = repo
            .find_commit(oid)
            .with_context(|| format!("failed to look up commit {oid}"))?;
        if let Some(snapshot) = to_snapshot(&repo, &commit)? {
            snapshots.push(snapshot);
        }
    }
    Ok(snapshots)
}

/// Makes the project look exactly like `snapshot_id` again, as a new
/// commit (after auto-saving any uncommitted work). Returns that commit.
///
/// If the project already matches the target once uncommitted work is
/// saved, no empty commit is made and the current HEAD snapshot is returned.
pub fn restore_to(project: &Path, snapshot_id: &str) -> Result<Snapshot> {
    let repo = Repository::open(project)
        .with_context(|| format!("failed to open Git repository at '{}'", project.display()))?;
    let target = resolve_commit(&repo, snapshot_id)?;

    // 1. Nothing uncommitted is ever lost: save it as its own snapshot.
    create_snapshot(project, AUTO_SAVE_TITLE, None)
        .context("failed to auto-save uncommitted changes before going back")?;

    let head = head_commit(&repo)?
        .context("the project has no history yet, so there is nothing to go back to")?;
    if head.tree_id() == target.tree_id() {
        return snapshot_of_any(&repo, &head);
    }

    // 2. A new commit whose tree is the target's tree, on top of HEAD.
    let title = format!("Went back to: {}", commit_title(&target));
    let trailers = [
        (TRAILER_SNAPSHOT, "1".to_string()),
        (TRAILER_RESTORES, target.id().to_string()),
    ];
    let target_tree = target.tree().context("failed to read the target's files")?;
    let oid = commit_tree(&repo, target_tree.id(), Some(&head), &title, &trailers)?;

    // 3. Only now, with everything committed, overwrite the working
    // directory. The index still describes the old HEAD tree, so checkout
    // removes files the target doesn't have; untracked and ignored files
    // are left alone.
    let mut checkout = CheckoutBuilder::new();
    checkout.force();
    repo.checkout_tree(target_tree.as_object(), Some(&mut checkout))
        .context("failed to update the project's files to the restored snapshot")?;

    let commit = repo.find_commit(oid)?;
    snapshot_of_any(&repo, &commit)
}

/// Restores to the snapshot before the latest one. `None` when there is
/// nothing to undo.
///
/// "Before the latest one" is the latest snapshot's first parent commit.
/// That one rule covers both cases in the plan: undoing a normal change
/// goes back to how things were before it, and undoing a "Went back to"
/// commit goes back to how things were before *that* — so pressing Undo
/// twice redoes. The latest snapshot is chosen before `restore_to`
/// auto-saves, so an auto-save made by this very call is never what gets
/// undone.
pub fn undo_last(project: &Path) -> Result<Option<Snapshot>> {
    let Some(latest) = list_snapshots(project, 1)?.into_iter().next() else {
        return Ok(None);
    };
    let repo = Repository::open(project)
        .with_context(|| format!("failed to open Git repository at '{}'", project.display()))?;
    let latest = repo.find_commit(Oid::from_str(&latest.id)?)?;
    if latest.parent_count() == 0 {
        return Ok(None);
    }
    let parent = latest
        .parent(0)
        .context("failed to read the previous snapshot")?;
    restore_to(project, &parent.id().to_string()).map(Some)
}

fn open_or_init(project: &Path) -> Result<Repository> {
    match Repository::open(project) {
        Ok(repo) => Ok(repo),
        Err(e) if e.code() == ErrorCode::NotFound => Repository::init(project).with_context(|| {
            format!(
                "failed to initialize a Git repository at '{}'",
                project.display()
            )
        }),
        Err(e) => Err(e)
            .with_context(|| format!("failed to open Git repository at '{}'", project.display())),
    }
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

/// `git add -A`: new and modified files (respecting `.gitignore`) plus
/// deletions. Writes the index and returns its tree.
fn stage_everything(repo: &Repository) -> Result<Oid> {
    let mut index = repo.index().context("failed to open the Git index")?;
    index
        .add_all(["*"], IndexAddOption::DEFAULT, None)
        .context("failed to stage changed files")?;
    index
        .update_all(["*"], None)
        .context("failed to stage deleted files")?;
    index.write().context("failed to write the Git index")?;
    index
        .write_tree()
        .context("failed to write the staged tree")
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

/// Parses a commit into a `Snapshot`, or `None` when it doesn't carry the
/// snapshot trailer.
fn to_snapshot(repo: &Repository, commit: &Commit) -> Result<Option<Snapshot>> {
    let message = commit.message().unwrap_or("");
    let trailers = parse_trailers(message);
    if !trailers
        .iter()
        .any(|(k, v)| k == TRAILER_SNAPSHOT && v == "1")
    {
        return Ok(None);
    }
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
    Ok(diff.deltas().len())
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
}
