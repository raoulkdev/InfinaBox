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

/// Returns the full recursive file tree rooted at `path`, skipping dotfiles
/// and dotfolders (`.git`, `.ibproject`, `.DS_Store`, etc.) — matching
/// Finder's own "hide dotfiles by default" behavior. This is also what
/// keeps `.ibproject` (where GDD docs live, see `DesignSection`) out of the
/// Files panel's browser while still being directly readable by its own
/// dedicated `list_directory` call scoped to `.ibproject/docs/gdd`.
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

        if file_name.starts_with('.') {
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

/// Creates an empty new file at `path`. Errors rather than silently
/// truncating if something is already there — a name collision is a user
/// mistake worth surfacing, not something to paper over. Missing parent
/// directories are created along the way (`create_dir_all`), so creating
/// the first document under a folder that doesn't exist yet — e.g. the
/// Design tab's `.ibproject/docs` before a project has adopted it — just
/// works, no separate "initialize" step needed.
#[tauri::command]
pub fn create_file(path: String) -> Result<(), String> {
    let target = Path::new(&path);
    if target.exists() {
        return Err(format!("'{path}' already exists"));
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create parent directories for '{path}': {e}"))?;
    }
    std::fs::write(target, "").map_err(|e| format!("failed to create '{path}': {e}"))
}

/// Creates a new directory at `path`, including any missing parents.
/// Errors if `path` already exists — same reasoning as `create_file`.
#[tauri::command]
pub fn create_directory(path: String) -> Result<(), String> {
    let target = Path::new(&path);
    if target.exists() {
        return Err(format!("'{path}' already exists"));
    }
    std::fs::create_dir_all(target).map_err(|e| format!("failed to create directory '{path}': {e}"))
}

/// Deletes the file or directory at `path` (directories are removed
/// recursively). This is the one genuinely irreversible command in this
/// module — the frontend is expected to confirm with the user before ever
/// calling it; this function itself just does what it's told.
#[tauri::command]
pub fn delete_path(path: String) -> Result<(), String> {
    let target = Path::new(&path);
    if !target.exists() {
        return Err(format!("'{path}' does not exist"));
    }
    let result = if target.is_dir() {
        std::fs::remove_dir_all(target)
    } else {
        std::fs::remove_file(target)
    };
    result.map_err(|e| format!("failed to delete '{path}': {e}"))
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
        assert!(
            !names.contains(&".ibproject"),
            ".ibproject (where GDD docs live) must not be listed — it's hidden from the \
             Files browser by the same dotfile rule as .git, not a special case for it"
        );

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
        let path = format!(
            "{}/.ibproject/docs/gdd/sector-3-verticality.md",
            get_default_project_path()
        );
        let contents = read_file(path).expect("real GDD doc should be readable");

        assert!(contents.contains("status: in-progress"), "frontmatter should survive intact");
        assert!(contents.contains("[[scripts/ElevatorLogic.gd]]"), "wiki-link syntax should survive intact");
        assert!(contents.contains("# Sector 3 — Verticality"));
    }

    #[test]
    fn list_directory_hides_any_dotfile_not_just_git() {
        let dir = std::env::temp_dir().join(format!(
            "infinabox-fs-dotfile-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".ibproject/docs/gdd")).unwrap();
        std::fs::write(dir.join(".ibproject/docs/gdd/core-loop.md"), "# Core loop").unwrap();
        std::fs::write(dir.join(".DS_Store"), "").unwrap();
        std::fs::write(dir.join("visible.txt"), "hello").unwrap();

        let entries = list_directory(dir.to_string_lossy().into_owned())
            .expect("a folder with dotfiles should still list fine");
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();

        assert_eq!(
            names,
            vec!["visible.txt"],
            "only the non-dot entry should be listed, got {names:?}"
        );

        // The hidden folder's own contents must still be directly readable —
        // DesignSection targets `.ibproject/docs/gdd` itself, it never
        // discovers it by listing the project root.
        let gdd_path = dir.join(".ibproject/docs/gdd").to_string_lossy().into_owned();
        let gdd_entries = list_directory(gdd_path).expect("the hidden folder's own contents should list fine");
        let gdd_names: Vec<&str> = gdd_entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(gdd_names, vec!["core-loop.md"]);

        std::fs::remove_dir_all(&dir).ok();
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

    #[test]
    fn read_file_error_message_for_binary_content_contains_expected_substring() {
        let dir = std::env::temp_dir().join(format!(
            "infinabox-fs-binary-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("not_text.bin").to_string_lossy().into_owned();

        // Invalid UTF-8 byte sequence — 0xFF is never valid as a UTF-8
        // continuation or lead byte.
        std::fs::write(&path, [0xFFu8, 0xFE, 0xFD, 0x00]).unwrap();

        let err = read_file(path).expect_err("reading binary content as a string should fail");
        assert!(
            err.to_lowercase().contains("stream did not contain valid utf-8"),
            "expected the standard library's UTF-8 error text, got: {err}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn create_file_creates_missing_parents_and_errors_on_collision() {
        let dir = std::env::temp_dir().join(format!(
            "infinabox-fs-create-file-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir
            .join(".ibproject/docs/gdd/new-doc.md")
            .to_string_lossy()
            .into_owned();

        create_file(path.clone()).expect("creating a file with missing parents should succeed");
        assert_eq!(
            std::fs::read_to_string(&path).expect("file should exist"),
            "",
            "a freshly created file should be empty"
        );

        let err = create_file(path).expect_err("creating over an existing file should error");
        assert!(err.contains("already exists"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn create_directory_creates_missing_parents_and_errors_on_collision() {
        let dir = std::env::temp_dir().join(format!(
            "infinabox-fs-create-dir-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("a/b/c").to_string_lossy().into_owned();

        create_directory(path.clone()).expect("creating nested directories should succeed");
        assert!(Path::new(&path).is_dir());

        let err =
            create_directory(path).expect_err("creating over an existing directory should error");
        assert!(err.contains("already exists"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn delete_path_removes_a_file() {
        let dir = std::env::temp_dir().join(format!(
            "infinabox-fs-delete-file-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("scratch.md").to_string_lossy().into_owned();
        std::fs::write(&path, "content").unwrap();

        delete_path(path.clone()).expect("deleting an existing file should succeed");
        assert!(!Path::new(&path).exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn delete_path_removes_a_directory_recursively() {
        let dir = std::env::temp_dir().join(format!(
            "infinabox-fs-delete-dir-test-{}",
            std::process::id()
        ));
        let nested = dir.join("a/b");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("file.txt"), "content").unwrap();

        delete_path(dir.to_string_lossy().into_owned())
            .expect("deleting a directory should recursively remove its contents");
        assert!(!dir.exists());
    }

    #[test]
    fn delete_path_errors_for_a_nonexistent_path() {
        let path = std::env::temp_dir()
            .join(format!("infinabox-fs-does-not-exist-{}", std::process::id()))
            .to_string_lossy()
            .into_owned();

        let err = delete_path(path).expect_err("deleting a path that doesn't exist should error");
        assert!(err.contains("does not exist"));
    }
}
