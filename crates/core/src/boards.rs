//! Document boards: the canvases of the Documents section.
//!
//! A board is one JSON file, `.ibproject/boards/<id>.json`, holding the
//! blocks placed on it (their shapes belong to the frontend; this module only
//! keeps the files safe). Images and attachments dropped on a board are saved
//! under `.ibproject/boards/files/`. Markdown documents are not stored here:
//! a document block points at a card in `.ibproject/context/`, which the AI
//! reads and writes like any other.

use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail, ensure};
use serde::{Deserialize, Serialize};

pub const BOARDS_DIR: &str = ".ibproject/boards";
pub const FILES_DIR: &str = ".ibproject/boards/files";
const MAX_BOARD_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_FILE_BYTES: usize = 25 * 1024 * 1024;

/// A board as stored: its id and the JSON text.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredBoard {
    pub id: String,
    pub json: String,
}

fn check_id(id: &str) -> Result<()> {
    ensure!(
        !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
        "That isn't a board id."
    );
    Ok(())
}

fn board_path(project: &Path, id: &str) -> Result<PathBuf> {
    check_id(id)?;
    Ok(project.join(BOARDS_DIR).join(format!("{id}.json")))
}

/// Every board in the project, by id.
pub fn read_all(project: &Path) -> Result<Vec<StoredBoard>> {
    let dir = project.join(BOARDS_DIR);
    let Ok(entries) = fs::read_dir(&dir) else { return Ok(Vec::new()) };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(id) = path.file_stem().and_then(|s| s.to_str()).map(str::to_string) else { continue };
        if check_id(&id).is_err() {
            continue;
        }
        let json = fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        out.push(StoredBoard { id, json });
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

/// Writes a board (atomically: a temp file renamed over it).
pub fn write(project: &Path, id: &str, json: &str) -> Result<()> {
    let path = board_path(project, id)?;
    ensure!(json.len() <= MAX_BOARD_BYTES, "That board is too big to save.");
    serde_json::from_str::<serde_json::Value>(json).context("That board isn't valid JSON.")?;
    let dir = project.join(BOARDS_DIR);
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let tmp = dir.join(format!(".{id}.json.tmp"));
    {
        let mut f = fs::File::create(&tmp).with_context(|| format!("creating {}", tmp.display()))?;
        f.write_all(json.as_bytes())?;
    }
    fs::rename(&tmp, &path).with_context(|| format!("saving {}", path.display()))?;
    Ok(())
}

pub fn delete(project: &Path, id: &str) -> Result<()> {
    let path = board_path(project, id)?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).with_context(|| format!("deleting {}", path.display())),
    }
}

/// Saves a dropped, pasted or picked file; returns its project-relative path.
pub fn save_file(project: &Path, name: &str, bytes: &[u8]) -> Result<String> {
    ensure!(!bytes.is_empty(), "That file is empty.");
    ensure!(bytes.len() <= MAX_FILE_BYTES, "That file is too big (the limit is {} MB).", MAX_FILE_BYTES / (1024 * 1024));
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let mut clean: String = base
        .chars()
        .map(|c| if c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ') { c } else { '_' })
        .collect();
    clean = clean.trim_matches(|c: char| c == '.' || c == ' ').to_string();
    if clean.is_empty() {
        clean = "file".into();
    }
    if clean.chars().count() > 80 {
        let ext = Path::new(&clean).extension().and_then(|e| e.to_str()).unwrap_or("").to_string();
        let stem: String = clean.chars().take(60).collect();
        clean = if ext.is_empty() || ext.len() > 10 { stem } else { format!("{stem}.{ext}") };
    }
    let dir = project.join(FILES_DIR);
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
    for n in 0u32.. {
        let file = if n == 0 { format!("{stamp}-{clean}") } else { format!("{stamp}-{n}-{clean}") };
        let path = dir.join(&file);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut f) => {
                f.write_all(bytes).with_context(|| format!("writing {}", path.display()))?;
                return Ok(format!("{FILES_DIR}/{file}"));
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e).with_context(|| format!("creating {}", path.display())),
        }
    }
    bail!("unreachable")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn boards_round_trip_and_ids_are_checked() {
        let dir = TempDir::new().unwrap();
        assert!(read_all(dir.path()).unwrap().is_empty());
        write(dir.path(), "root", r#"{"id":"root","blocks":[]}"#).unwrap();
        write(dir.path(), "b-2", r#"{"id":"b-2"}"#).unwrap();
        let all = read_all(dir.path()).unwrap();
        assert_eq!(all.iter().map(|b| b.id.as_str()).collect::<Vec<_>>(), ["b-2", "root"]);
        assert!(write(dir.path(), "../x", "{}").is_err());
        assert!(write(dir.path(), "", "{}").is_err());
        assert!(write(dir.path(), "ok", "not json").is_err());
        delete(dir.path(), "b-2").unwrap();
        delete(dir.path(), "b-2").unwrap();
        assert_eq!(read_all(dir.path()).unwrap().len(), 1);
        // Temp files and stray names are ignored.
        fs::write(dir.path().join(BOARDS_DIR).join("notes.txt"), "x").unwrap();
        assert_eq!(read_all(dir.path()).unwrap().len(), 1);
    }

    #[test]
    fn files_get_safe_unique_names() {
        let dir = TempDir::new().unwrap();
        let a = save_file(dir.path(), "../../a b?.png", b"one").unwrap();
        let b = save_file(dir.path(), "../../a b?.png", b"two").unwrap();
        assert_ne!(a, b);
        assert!(a.starts_with(".ibproject/boards/files/") && a.ends_with("a b_.png"), "{a}");
        assert_eq!(fs::read(dir.path().join(&a)).unwrap(), b"one");
        assert!(save_file(dir.path(), "x", b"").is_err());
    }
}
