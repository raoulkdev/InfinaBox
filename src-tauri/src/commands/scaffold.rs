//! Thin Tauri wrapper over `infinabox_core::scaffold` for Home's "New
//! Project" flow. All the real work (name validation, template copy, addon
//! install, `.ibproject/` marker, `git init`, first snapshot, cleanup on
//! failure) lives in core; core's errors are plain sentences written for
//! users ("the project name can't start with a dot"), so they're passed
//! through whole — cause chain included via `{:#}` — for the dialog to show.

use std::path::Path;

#[tauri::command(async)]
pub fn project_create(parent_dir: String, name: String) -> Result<String, String> {
    infinabox_core::scaffold::create_project(Path::new(&parent_dir), &name)
        .map(|project| project.to_string_lossy().into_owned())
        .map_err(|e| format!("{e:#}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_parent(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "infinabox-scaffold-cmd-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn creates_a_project_and_returns_its_path() {
        let parent = temp_parent("ok");
        let path = project_create(parent.to_string_lossy().into_owned(), "My Game".into()).unwrap();
        assert_eq!(Path::new(&path), parent.join("My Game"));
        assert!(Path::new(&path).join(".ibproject/.ibx").is_file());
        assert!(Path::new(&path).join("project.godot").is_file());
        fs::remove_dir_all(&parent).unwrap();
    }

    #[test]
    fn a_bad_name_comes_back_as_cores_plain_message() {
        let parent = temp_parent("bad");
        let err = project_create(parent.to_string_lossy().into_owned(), ".hidden".into()).unwrap_err();
        assert_eq!(err, "the project name can't start with a dot");
        fs::remove_dir_all(&parent).unwrap();
    }
}
