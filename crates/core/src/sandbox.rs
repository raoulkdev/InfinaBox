//! Trial changes: the AI works on a separate copy of the game, and the
//! person decides whether the result becomes part of the real game.
//!
//! A sandbox is a plain folder copy of the project (without its git history,
//! chat, engine cache or other sandboxes) kept outside the project, plus a
//! manifest of what every file was when it was copied. The AI's turn runs
//! in the copy, so the real game, its history and the running game are
//! untouched. Afterwards `changes` compares the copy with that manifest —
//! what was added, edited or removed — and says which of those files the real
//! game has also changed since (conflicts). `apply` brings the others over
//! (the caller then snapshots, so Undo works as for any change); `discard`
//! deletes the copy.
//!
//! The copy is not a git worktree on purpose: the real project's history,
//! chat and settings live in the same repository, and a branch would have to
//! merge them. Comparing two folders needs no merge and can't touch history.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// A project bigger than this isn't copied.
const MAX_COPY_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Folders (relative, `/`-separated) never copied and never compared.
fn skipped(rel: &str) -> bool {
    const TOP: &[&str] = &[".git", ".godot", ".import", "node_modules"];
    const NESTED: &[&str] = &[".ibproject/chat", ".ibproject/playtest", ".ibproject/sandboxes"];
    let first = rel.split('/').next().unwrap_or("");
    TOP.contains(&first) || NESTED.iter().any(|n| rel == *n || rel.starts_with(&format!("{n}/")))
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Sandbox {
    pub id: String,
    /// What the AI was asked, for the list.
    pub title: String,
    /// The copy's folder (the AI works here, and the game runs from here).
    pub path: PathBuf,
    pub created: i64,
}

#[derive(Serialize, Deserialize, Default)]
struct Manifest {
    sandbox: Option<Sandbox>,
    /// The project this was copied from.
    origin: PathBuf,
    /// Path -> content hash, as copied.
    files: BTreeMap<String, String>,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Added,
    Modified,
    Removed,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Change {
    pub path: String,
    pub kind: ChangeKind,
    /// The real game's copy of this file changed after the sandbox was made,
    /// so bringing this over would overwrite that change.
    pub conflict: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq, Default)]
pub struct ApplyReport {
    pub applied: Vec<String>,
    /// Left alone because the real game changed them too.
    pub conflicts: Vec<String>,
}

fn hash_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).with_context(|| format!("couldn't read {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Every file under `root` that counts: relative path -> absolute path.
fn walk(root: &Path) -> Result<BTreeMap<String, PathBuf>> {
    fn go(root: &Path, dir: &Path, out: &mut BTreeMap<String, PathBuf>) -> Result<()> {
        for entry in fs::read_dir(dir).with_context(|| format!("couldn't read {}", dir.display()))? {
            let entry = entry?;
            let path = entry.path();
            let rel = path.strip_prefix(root)?.to_string_lossy().replace('\\', "/");
            if skipped(&rel) {
                continue;
            }
            let kind = entry.file_type()?;
            if kind.is_dir() {
                go(root, &path, out)?;
            } else if kind.is_file() {
                out.insert(rel, path);
            }
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    go(root, root, &mut out)?;
    Ok(out)
}

fn sandbox_dir(base: &Path, id: &str) -> PathBuf {
    base.join(id)
}

fn manifest_path(base: &Path, id: &str) -> PathBuf {
    base.join(format!("{id}.manifest.json"))
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn read_manifest(base: &Path, id: &str) -> Result<Manifest> {
    if !valid_id(id) {
        bail!("That isn't a sandbox.");
    }
    let text = fs::read_to_string(manifest_path(base, id)).context("That trial copy no longer exists.")?;
    serde_json::from_str(&text).context("That trial copy's record is damaged.")
}

/// Copies `project` into a new sandbox under `base` (a folder outside the
/// project) and records what it copied.
pub fn create(base: &Path, project: &Path, title: &str) -> Result<Sandbox> {
    if !project.is_dir() {
        bail!("The project folder {} doesn't exist.", project.display());
    }
    let files = walk(project)?;
    let total: u64 = files.values().filter_map(|p| fs::metadata(p).ok()).map(|m| m.len()).sum();
    if total > MAX_COPY_BYTES {
        bail!("This game is too big to try in a separate copy ({} MB).", total / 1024 / 1024);
    }
    let id = uuid::Uuid::new_v4().simple().to_string();
    let path = sandbox_dir(base, &id);
    fs::create_dir_all(&path)?;
    let mut manifest = Manifest { sandbox: None, origin: project.to_path_buf(), files: BTreeMap::new() };
    let copied = (|| -> Result<()> {
        for (rel, src) in &files {
            let dest = path.join(rel);
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(src, &dest).with_context(|| format!("couldn't copy {rel}"))?;
            manifest.files.insert(rel.clone(), hash_file(&dest)?);
        }
        Ok(())
    })();
    if let Err(e) = copied {
        let _ = fs::remove_dir_all(&path);
        return Err(e);
    }
    let title: String = title.trim().chars().take(80).collect();
    let sandbox = Sandbox {
        id: id.clone(),
        title: if title.is_empty() { "Trial change".into() } else { title },
        path,
        created: chrono::Utc::now().timestamp(),
    };
    manifest.sandbox = Some(sandbox.clone());
    fs::write(manifest_path(base, &id), serde_json::to_vec(&manifest)?)?;
    Ok(sandbox)
}

/// The sandboxes made from `project` that still exist, newest first.
pub fn list(base: &Path, project: &Path) -> Result<Vec<Sandbox>> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(base) else { return Ok(out) };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(id) = name.strip_suffix(".manifest.json") else { continue };
        let Ok(manifest) = read_manifest(base, id) else { continue };
        if manifest.origin == project {
            if let Some(sandbox) = manifest.sandbox.filter(|s| s.path.is_dir()) {
                out.push(sandbox);
            }
        }
    }
    out.sort_by(|a, b| b.created.cmp(&a.created));
    Ok(out)
}

/// One sandbox by id.
pub fn get(base: &Path, id: &str) -> Result<Sandbox> {
    let sandbox = read_manifest(base, id)?.sandbox.context("That trial copy's record is damaged.")?;
    if !sandbox.path.is_dir() {
        bail!("That trial copy no longer exists.");
    }
    Ok(sandbox)
}

/// What the AI changed in the copy, and which of that the real game also
/// changed since.
pub fn changes(base: &Path, id: &str) -> Result<Vec<Change>> {
    let manifest = read_manifest(base, id)?;
    let copy = sandbox_dir(base, id);
    if !copy.is_dir() {
        bail!("That trial copy no longer exists.");
    }
    let now = walk(&copy)?;
    let mut out = Vec::new();
    // The real game's file differs from what was copied (or appeared/vanished).
    let real_changed = |rel: &str, was: Option<&String>| -> bool {
        let real = manifest.origin.join(rel);
        match (was, real.is_file()) {
            (None, false) => false,
            (None, true) | (Some(_), false) => true,
            (Some(h), true) => hash_file(&real).map(|now| &now != h).unwrap_or(true),
        }
    };
    for (rel, path) in &now {
        match manifest.files.get(rel) {
            None => out.push(Change { path: rel.clone(), kind: ChangeKind::Added, conflict: real_changed(rel, None) }),
            Some(was) if hash_file(path)? != *was => {
                out.push(Change { path: rel.clone(), kind: ChangeKind::Modified, conflict: real_changed(rel, Some(was)) })
            }
            Some(_) => {}
        }
    }
    for (rel, was) in &manifest.files {
        if !now.contains_key(rel) {
            out.push(Change { path: rel.clone(), kind: ChangeKind::Removed, conflict: real_changed(rel, Some(was)) });
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// A relative path that stays inside its folder.
fn safe_rel(rel: &str) -> bool {
    !rel.is_empty() && !rel.starts_with('/') && !rel.split('/').any(|p| p.is_empty() || p == ".." || p == ".") && !rel.contains('\\')
}

/// Brings the sandbox's changes into the real game, except those the real
/// game also changed (reported, left as they are). `only` limits it to some
/// paths. The sandbox stays until `discard`.
pub fn apply(base: &Path, id: &str, only: Option<&[String]>) -> Result<ApplyReport> {
    let manifest = read_manifest(base, id)?;
    let copy = sandbox_dir(base, id);
    let mut report = ApplyReport::default();
    for change in changes(base, id)? {
        if only.is_some_and(|o| !o.contains(&change.path)) {
            continue;
        }
        if change.conflict {
            report.conflicts.push(change.path);
            continue;
        }
        if !safe_rel(&change.path) || skipped(&change.path) {
            continue;
        }
        let real = manifest.origin.join(&change.path);
        match change.kind {
            ChangeKind::Added | ChangeKind::Modified => {
                if let Some(parent) = real.parent() {
                    fs::create_dir_all(parent)?;
                }
                // Through a temp file in the same folder, so a crash leaves
                // the old file or the new one, never half of it.
                let tmp = real.with_extension("ibtrial.tmp");
                fs::copy(copy.join(&change.path), &tmp)?;
                fs::rename(&tmp, &real)?;
            }
            ChangeKind::Removed => {
                if real.is_file() {
                    fs::remove_file(&real)?;
                }
            }
        }
        report.applied.push(change.path);
    }
    Ok(report)
}

/// Deletes a sandbox and its record.
pub fn discard(base: &Path, id: &str) -> Result<()> {
    if !valid_id(id) {
        bail!("That isn't a sandbox.");
    }
    let dir = sandbox_dir(base, id);
    if dir.is_dir() {
        fs::remove_dir_all(&dir).context("couldn't remove the trial copy")?;
    }
    match fs::remove_file(manifest_path(base, id)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        _tmp: tempfile::TempDir,
        base: PathBuf,
        project: PathBuf,
    }

    fn write(root: &Path, rel: &str, text: &str) {
        let p = root.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }

    fn fixture() -> Fixture {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("game");
        let base = tmp.path().join("sandboxes");
        write(&project, "project.godot", "config_version=5\n");
        write(&project, "scripts/player.gd", "extends Node\n");
        write(&project, "scripts/old.gd", "extends Node\n");
        write(&project, ".ibproject/context/idea.md", "# Idea\n");
        write(&project, ".ibproject/chat/t.jsonl", "{}\n");
        write(&project, ".godot/cache.bin", "cache");
        write(&project, ".git/HEAD", "ref");
        Fixture { base, project, _tmp: tmp }
    }

    #[test]
    fn a_sandbox_copies_the_game_but_not_history_chat_or_cache() {
        let f = fixture();
        let s = create(&f.base, &f.project, "  Add a double jump  ").unwrap();
        assert_eq!(s.title, "Add a double jump");
        assert!(s.path.join("scripts/player.gd").is_file());
        assert!(s.path.join(".ibproject/context/idea.md").is_file());
        for no in [".git/HEAD", ".godot/cache.bin", ".ibproject/chat/t.jsonl"] {
            assert!(!s.path.join(no).exists(), "{no}");
        }
        assert!(changes(&f.base, &s.id).unwrap().is_empty());
        assert_eq!(list(&f.base, &f.project).unwrap(), vec![s.clone()]);
        assert!(list(&f.base, Path::new("/elsewhere")).unwrap().is_empty());
    }

    #[test]
    fn changes_are_found_and_applied_and_the_real_game_is_untouched_until_then() {
        let f = fixture();
        let s = create(&f.base, &f.project, "t").unwrap();
        write(&s.path, "scripts/player.gd", "extends Node\nvar jumps = 2\n");
        write(&s.path, "scripts/enemy.gd", "extends Node\n");
        fs::remove_file(s.path.join("scripts/old.gd")).unwrap();
        write(&s.path, ".ibproject/context/jump.md", "# Jump\n");
        // The engine's cache in the copy isn't a change.
        write(&s.path, ".godot/imported.bin", "x");

        let c = changes(&f.base, &s.id).unwrap();
        let summary: Vec<_> = c.iter().map(|c| (c.path.as_str(), c.kind, c.conflict)).collect();
        assert_eq!(
            summary,
            vec![
                (".ibproject/context/jump.md", ChangeKind::Added, false),
                ("scripts/enemy.gd", ChangeKind::Added, false),
                ("scripts/old.gd", ChangeKind::Removed, false),
                ("scripts/player.gd", ChangeKind::Modified, false),
            ]
        );
        // Nothing reached the real game yet.
        assert_eq!(fs::read_to_string(f.project.join("scripts/player.gd")).unwrap(), "extends Node\n");
        assert!(f.project.join("scripts/old.gd").exists());

        let r = apply(&f.base, &s.id, None).unwrap();
        assert_eq!(r.applied.len(), 4);
        assert!(r.conflicts.is_empty());
        assert_eq!(fs::read_to_string(f.project.join("scripts/player.gd")).unwrap(), "extends Node\nvar jumps = 2\n");
        assert!(f.project.join("scripts/enemy.gd").is_file());
        assert!(f.project.join(".ibproject/context/jump.md").is_file());
        assert!(!f.project.join("scripts/old.gd").exists());
        // The chat and git folders of the real game were never touched.
        assert!(f.project.join(".ibproject/chat/t.jsonl").is_file());
        assert!(f.project.join(".git/HEAD").is_file());
    }

    #[test]
    fn a_file_changed_in_both_places_is_a_conflict_and_is_left_alone() {
        let f = fixture();
        let s = create(&f.base, &f.project, "t").unwrap();
        write(&s.path, "scripts/player.gd", "from the copy\n");
        write(&s.path, "scripts/enemy.gd", "new\n");
        // The person edits the real game meanwhile, and adds the same new file.
        write(&f.project, "scripts/player.gd", "from the person\n");
        write(&f.project, "scripts/enemy.gd", "also new\n");
        let c = changes(&f.base, &s.id).unwrap();
        assert!(c.iter().all(|c| c.conflict), "{c:?}");
        let r = apply(&f.base, &s.id, None).unwrap();
        assert!(r.applied.is_empty());
        assert_eq!(r.conflicts, vec!["scripts/enemy.gd", "scripts/player.gd"]);
        assert_eq!(fs::read_to_string(f.project.join("scripts/player.gd")).unwrap(), "from the person\n");
    }

    #[test]
    fn apply_can_take_only_some_files() {
        let f = fixture();
        let s = create(&f.base, &f.project, "t").unwrap();
        write(&s.path, "scripts/a.gd", "a");
        write(&s.path, "scripts/b.gd", "b");
        let r = apply(&f.base, &s.id, Some(&["scripts/a.gd".to_string()])).unwrap();
        assert_eq!(r.applied, vec!["scripts/a.gd"]);
        assert!(f.project.join("scripts/a.gd").exists() && !f.project.join("scripts/b.gd").exists());
    }

    #[test]
    fn discard_removes_the_copy_and_its_record() {
        let f = fixture();
        let s = create(&f.base, &f.project, "t").unwrap();
        discard(&f.base, &s.id).unwrap();
        assert!(!s.path.exists());
        assert!(list(&f.base, &f.project).unwrap().is_empty());
        assert!(changes(&f.base, &s.id).is_err());
        // Doing it again is fine; odd ids are refused.
        discard(&f.base, &s.id).unwrap();
        assert!(discard(&f.base, "../x").is_err());
        assert!(apply(&f.base, "../x", None).is_err());
    }

    #[test]
    fn unsafe_paths_never_leave_the_project() {
        assert!(safe_rel("scripts/a.gd"));
        for bad in ["", "/etc/passwd", "../a", "a/../b", "a//b", "a\\b", "./a"] {
            assert!(!safe_rel(bad), "{bad}");
        }
    }
}
