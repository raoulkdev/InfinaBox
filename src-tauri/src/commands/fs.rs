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

/// Phase 1, milestone 6: reads a file's contents so the GDD editor can open
/// a real markdown file from the project's docs folder.
#[tauri::command]
pub fn read_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| format!("failed to read '{path}': {e}"))
}

/// Writes `contents` to `path`, overwriting it — the GDD editor's Save.
/// The file already being a real, Git-tracked file in the project is what
/// makes "save it, see a normal Git diff" true; this command does nothing
/// Git-specific itself, on purpose.
#[tauri::command]
pub fn write_file(path: String, contents: String) -> Result<(), String> {
    std::fs::write(&path, contents).map_err(|e| format!("failed to write '{path}': {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `list_directory` has no Git dependency at all (see its implementation
    /// above — plain `std::fs::read_dir`), so it must keep working fine for
    /// a folder a user picks via the new Open Project dialog that isn't a
    /// Git repository — unlike `refresh_project_graph`, which genuinely
    /// needs `.git` history and fails for such folders (see
    /// commands/project.rs's non-git-folder test).
    #[test]
    fn lists_a_plain_non_git_folder_just_fine() {
        let dir = std::env::temp_dir().join(format!(
            "infinabox-fs-non-git-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "hello").unwrap();
        std::fs::write(dir.join("b.txt"), "world").unwrap();

        let entries = list_directory(dir.to_string_lossy().into_owned())
            .expect("a plain, non-Git folder should list just fine");
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"a.txt"));
        assert!(names.contains(&"b.txt"));

        std::fs::remove_dir_all(&dir).ok();
    }

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

    #[test]
    fn reads_a_real_gdd_doc_from_the_fixture() {
        let path = format!("{}/docs/gdd/sector-3-verticality.md", get_default_project_path());
        let contents = read_file(path).expect("real GDD doc should be readable");

        assert!(contents.contains("status: in-progress"), "frontmatter should survive intact");
        assert!(contents.contains("[[scripts/ElevatorLogic.gd]]"), "wiki-link syntax should survive intact");
        assert!(contents.contains("# Sector 3 — Verticality"));
    }

    #[test]
    fn write_then_read_round_trips_and_only_touches_the_given_path() {
        let dir = std::env::temp_dir().join(format!("infinabox-fs-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("scratch.md").to_string_lossy().into_owned();

        write_file(path.clone(), "# Draft\n\nSome content.".to_string())
            .expect("write should succeed");
        let round_tripped = read_file(path.clone()).expect("read after write should succeed");
        assert_eq!(round_tripped, "# Draft\n\nSome content.");

        // Overwrite, confirm the new content replaces the old rather than appending.
        write_file(path.clone(), "# Revised".to_string()).expect("overwrite should succeed");
        let revised = read_file(path).expect("read after overwrite should succeed");
        assert_eq!(revised, "# Revised");

        std::fs::remove_dir_all(&dir).ok();
    }
}
