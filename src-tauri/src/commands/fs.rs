//! Phase 1, milestone 3: the file browser, wired to a real project folder.

use std::path::Path;

use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    /// `None` for files. `Some(children)` for directories, always present
    /// (possibly empty) so the frontend never has to special-case it.
    pub children: Option<Vec<FileEntry>>,
}

/// Returns the full recursive file tree rooted at `path`, skipping `.git`.
/// Directories sort before files, then alphabetically within each group —
/// matching how a project actually reads in Finder.
#[tauri::command]
pub fn list_directory(path: String) -> Result<Vec<FileEntry>, String> {
    let root = Path::new(&path);
    if !root.is_dir() {
        return Err(format!("'{path}' is not a directory"));
    }
    read_dir_recursive(root).map_err(|e| format!("failed to read '{path}': {e}"))
}

fn read_dir_recursive(dir: &Path) -> std::io::Result<Vec<FileEntry>> {
    let mut entries = Vec::new();

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let file_name = entry.file_name().to_string_lossy().into_owned();

        if file_name == ".git" {
            continue;
        }

        let entry_path = entry.path();
        let is_dir = entry_path.is_dir();
        let children = if is_dir {
            Some(read_dir_recursive(&entry_path)?)
        } else {
            None
        };

        entries.push(FileEntry {
            name: file_name,
            path: entry_path.to_string_lossy().into_owned(),
            is_dir,
            children,
        });
    }

    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    Ok(entries)
}

/// Temporary until a real "open project" flow (folder picker) exists —
/// points at the Phase 0 test fixture so the file browser has something
/// real to show today.
#[tauri::command]
pub fn get_default_project_path() -> String {
    "/Users/raoulkaleba/Developer/hollow-meridian-test".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_the_hollow_meridian_fixture_and_skips_git() {
        let entries = list_directory(get_default_project_path()).expect("fixture should exist");
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();

        assert!(names.contains(&"scenes"), "expected a scenes/ dir, got {names:?}");
        assert!(names.contains(&"scripts"), "expected a scripts/ dir, got {names:?}");
        assert!(names.contains(&"project.godot"), "expected project.godot, got {names:?}");
        assert!(!names.contains(&".git"), ".git must not be listed");

        let scripts = entries
            .iter()
            .find(|e| e.name == "scripts")
            .expect("scripts dir present");
        assert!(scripts.is_dir);
        let script_children = scripts.children.as_ref().expect("dir has children");
        let script_names: Vec<&str> = script_children.iter().map(|e| e.name.as_str()).collect();
        assert!(
            script_names.contains(&"ElevatorLogic.gd"),
            "expected renamed ElevatorLogic.gd, got {script_names:?}"
        );
        assert!(
            !script_names.contains(&"elevator_logic.gd"),
            "old pre-rename name should not exist on disk anymore"
        );
    }
}
