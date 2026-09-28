//! Per-project settings, stored in `.ibproject/settings.json` and committed
//! with the game: how the agent plans, explains, and fixes errors. Nothing
//! secret lives here.
//!
//! The file is pretty-printed JSON. A missing file (or a missing field)
//! means the defaults; fields this version doesn't know are ignored, so a
//! project saved by a newer InfinaBox still opens. A file that isn't valid
//! settings JSON is an error, never silently the defaults — otherwise the
//! next save would throw away whatever the person had set. Saves are atomic
//! (a temp file in the same folder, then a rename), so a crash mid-save
//! never leaves half a file behind.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::agent::PlanPolicy;

/// Where the settings live, relative to the project folder.
pub const SETTINGS_FILE: &str = ".ibproject/settings.json";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct ProjectSettings {
    pub plan_policy: PlanPolicy,
    /// "Teach me": explanations grow into short lessons.
    pub teach: bool,
    /// Send the running game's errors back to the agent automatically.
    pub auto_fix: bool,
}

impl Default for ProjectSettings {
    fn default() -> Self {
        Self {
            plan_policy: PlanPolicy::AlwaysPlan,
            teach: false,
            auto_fix: true,
        }
    }
}

fn settings_path(project: &Path) -> PathBuf {
    project.join(SETTINGS_FILE)
}

/// Reads `.ibproject/settings.json`; a missing file is the defaults.
pub fn load(project: &Path) -> Result<ProjectSettings> {
    let path = settings_path(project);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ProjectSettings::default());
        }
        Err(e) => {
            return Err(e).with_context(|| format!("couldn't read {}", path.display()));
        }
    };
    // Only an object counts: serde would also read a struct from a list,
    // which here could only be a damaged file.
    serde_json::from_str::<serde_json::Value>(&text)
        .map_err(anyhow::Error::from)
        .and_then(|value| {
            if !value.is_object() {
                anyhow::bail!("expected a JSON object");
            }
            Ok(serde_json::from_value::<ProjectSettings>(value)?)
        })
        .with_context(|| {
            format!(
                "The project's settings file ({SETTINGS_FILE}) is damaged, so InfinaBox didn't \
use it. Fix or delete it to go back to the defaults."
            )
        })
}

/// Writes `.ibproject/settings.json` (creating `.ibproject/` if needed),
/// replacing the old file in one step.
pub fn save(project: &Path, settings: &ProjectSettings) -> Result<()> {
    if !project.is_dir() {
        anyhow::bail!("The project folder {} doesn't exist.", project.display());
    }
    let path = settings_path(project);
    let dir = path
        .parent()
        .expect("the settings file is inside .ibproject");
    std::fs::create_dir_all(dir).with_context(|| format!("couldn't create {}", dir.display()))?;
    let mut json = serde_json::to_string_pretty(settings)?;
    json.push('\n');

    let temp = dir.join(format!("settings.json.tmp-{}", uuid::Uuid::new_v4()));
    let write = || -> std::io::Result<()> {
        let mut file = std::fs::File::create_new(&temp)?;
        file.write_all(json.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&temp, &path)
    };
    write().map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        anyhow::Error::new(e).context(format!("couldn't save {}", path.display()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".ibproject")).unwrap();
        dir
    }

    fn write(project: &Path, text: &str) {
        std::fs::write(project.join(SETTINGS_FILE), text).unwrap();
    }

    #[test]
    fn defaults_always_plan_no_teach_auto_fix_on() {
        assert_eq!(
            ProjectSettings::default(),
            ProjectSettings {
                plan_policy: PlanPolicy::AlwaysPlan,
                teach: false,
                auto_fix: true,
            }
        );
    }

    #[test]
    fn a_missing_file_is_the_defaults() {
        let dir = project();
        assert_eq!(load(dir.path()).unwrap(), ProjectSettings::default());
        // Even without an .ibproject folder at all.
        let bare = tempfile::tempdir().unwrap();
        assert_eq!(load(bare.path()).unwrap(), ProjectSettings::default());
    }

    #[test]
    fn saves_pretty_json_and_loads_it_back() {
        let dir = project();
        let settings = ProjectSettings {
            plan_policy: PlanPolicy::SmallChangesDirect,
            teach: true,
            auto_fix: false,
        };
        save(dir.path(), &settings).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join(SETTINGS_FILE)).unwrap(),
            "{\n  \"plan_policy\": \"small_changes_direct\",\n  \"teach\": true,\n  \
\"auto_fix\": false\n}\n"
        );
        assert_eq!(load(dir.path()).unwrap(), settings);

        // Saving again replaces it, and leaves no temp files behind.
        save(dir.path(), &ProjectSettings::default()).unwrap();
        assert_eq!(load(dir.path()).unwrap(), ProjectSettings::default());
        let names: Vec<String> = std::fs::read_dir(dir.path().join(".ibproject"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["settings.json"]);
    }

    #[test]
    fn save_creates_the_ibproject_folder_but_not_the_project() {
        let bare = tempfile::tempdir().unwrap();
        save(bare.path(), &ProjectSettings::default()).unwrap();
        assert!(bare.path().join(SETTINGS_FILE).is_file());

        let missing = bare.path().join("no-such-project");
        assert!(save(&missing, &ProjectSettings::default()).is_err());
        assert!(!missing.exists());
    }

    #[test]
    fn unknown_fields_are_ignored_and_missing_ones_default() {
        let dir = project();
        write(
            dir.path(),
            r#"{"teach": true, "from_a_newer_version": {"x": 1}}"#,
        );
        assert_eq!(
            load(dir.path()).unwrap(),
            ProjectSettings {
                teach: true,
                ..ProjectSettings::default()
            }
        );
    }

    #[test]
    fn a_damaged_file_is_a_clear_error_not_the_defaults() {
        let dir = project();
        for text in [
            "{ not json",
            "",
            "[]",
            r#"{"plan_policy": "whenever"}"#,
            r#"{"teach": "yes"}"#,
        ] {
            write(dir.path(), text);
            let err = format!("{:#}", load(dir.path()).unwrap_err());
            assert!(err.contains("settings file"), "{text}: {err}");
            assert!(err.contains(".ibproject/settings.json"), "{text}: {err}");
        }
        // Nothing was overwritten by loading.
        assert_eq!(
            std::fs::read_to_string(dir.path().join(SETTINGS_FILE)).unwrap(),
            r#"{"teach": "yes"}"#
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_save_leaves_the_old_file_and_no_temp_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = project();
        save(dir.path(), &ProjectSettings::default()).unwrap();
        let ib = dir.path().join(".ibproject");
        std::fs::set_permissions(&ib, std::fs::Permissions::from_mode(0o555)).unwrap();
        // Root ignores directory permissions; only check when they apply.
        let blocked = std::fs::File::create(ib.join("probe")).is_err();
        let result = save(
            dir.path(),
            &ProjectSettings {
                teach: true,
                ..ProjectSettings::default()
            },
        );
        std::fs::set_permissions(&ib, std::fs::Permissions::from_mode(0o755)).unwrap();
        if blocked {
            assert!(result.is_err());
            assert_eq!(load(dir.path()).unwrap(), ProjectSettings::default());
            assert_eq!(std::fs::read_dir(&ib).unwrap().count(), 1);
        }
    }
}
