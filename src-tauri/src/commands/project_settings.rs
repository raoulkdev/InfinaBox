//! Per-project settings commands (`infinabox_core::project_settings`):
//! plan policy, "Teach me" and "Fix errors automatically", stored in the
//! project's `.ibproject/settings.json`.

use std::path::Path;

use infinabox_core::project_settings::{self, ProjectSettings};

/// Formats a core error with its whole cause chain, like `snapshot.rs`.
fn user_error(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

/// Settings belong to a project that exists: without this, saving for a
/// wrong path would create `.ibproject/` wherever it pointed.
fn existing_project(project_path: &str) -> Result<&Path, String> {
    let project = Path::new(project_path);
    if project.is_dir() {
        Ok(project)
    } else {
        Err(format!("The project folder {project_path} doesn't exist."))
    }
}

fn get(project_path: &str) -> Result<ProjectSettings, String> {
    project_settings::load(existing_project(project_path)?).map_err(user_error)
}

fn set(project_path: &str, settings: ProjectSettings) -> Result<ProjectSettings, String> {
    project_settings::save(existing_project(project_path)?, &settings).map_err(user_error)?;
    Ok(settings)
}

#[tauri::command(async)]
pub fn project_settings_get(project_path: String) -> Result<ProjectSettings, String> {
    get(&project_path)
}

/// Saves the settings and returns what was saved.
#[tauri::command(async)]
pub fn project_settings_set(
    project_path: String,
    settings: ProjectSettings,
) -> Result<ProjectSettings, String> {
    set(&project_path, settings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use infinabox_core::agent::PlanPolicy;
    use std::fs;
    use std::path::PathBuf;

    fn temp_project(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "infinabox-project-settings-cmd-{tag}-{}-{}",
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
    fn a_new_project_has_the_defaults() {
        let project = temp_project("defaults");
        let settings = get(&project.to_string_lossy()).unwrap();
        assert_eq!(settings, ProjectSettings::default());
        fs::remove_dir_all(&project).unwrap();
    }

    #[test]
    fn set_returns_what_was_saved_and_get_reads_it_back() {
        let project = temp_project("roundtrip");
        let path = project.to_string_lossy().into_owned();
        let wanted = ProjectSettings {
            plan_policy: PlanPolicy::SmallChangesDirect,
            teach: true,
            auto_fix: false,
        };
        assert_eq!(set(&path, wanted.clone()).unwrap(), wanted);
        assert!(project.join(".ibproject/settings.json").is_file());
        assert_eq!(get(&path).unwrap(), wanted);
        fs::remove_dir_all(&project).unwrap();
    }

    #[test]
    fn a_missing_project_is_refused_and_nothing_is_created() {
        let project = temp_project("missing");
        let gone = project.join("not-here");
        let path = gone.to_string_lossy().into_owned();
        assert!(get(&path).unwrap_err().contains("doesn't exist"));
        assert!(set(&path, ProjectSettings::default()).is_err());
        assert!(!gone.exists());
        fs::remove_dir_all(&project).unwrap();
    }

    #[test]
    fn a_damaged_file_is_reported_not_replaced() {
        let project = temp_project("damaged");
        fs::create_dir_all(project.join(".ibproject")).unwrap();
        fs::write(project.join(".ibproject/settings.json"), "[1, 2]").unwrap();
        let err = get(&project.to_string_lossy()).unwrap_err();
        assert!(err.contains("damaged"), "{err}");
        fs::remove_dir_all(&project).unwrap();
    }
}
