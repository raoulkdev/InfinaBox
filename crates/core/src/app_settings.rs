//! App-wide settings (not per project): which AI the user connected, a
//! custom Godot path, and whether the first-run setup is done. Stored as
//! `settings.json` in the app data folder. Nothing secret is ever stored
//! here — the AI CLIs keep their own credentials.
//!
//! Saved as pretty JSON through a temp file and a rename, so a crash
//! mid-save leaves the old file, never half a new one. Fields this version
//! doesn't know (from a newer InfinaBox) are ignored on load.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::connect::ProviderId;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct AppSettings {
    pub ai_provider: Option<ProviderId>,
    /// A Godot binary the user chose in Advanced settings, used instead of
    /// the managed install.
    pub godot_path: Option<String>,
    pub first_run_done: bool,
    /// Per-provider model settings (Phase C), keyed by the provider's string
    /// id (`anthropic-api`, `openai-api`, `local-model`). Never holds a key.
    pub models: std::collections::BTreeMap<String, ModelConfig>,
}

/// Which model an API runtime uses, and where a local one lives.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct ModelConfig {
    /// Up to and including `/v1` for OpenAI-compatible servers
    /// (`http://localhost:11434/v1`); unused for the Anthropic API.
    pub base_url: Option<String>,
    pub model: Option<String>,
}

const FILE_NAME: &str = "settings.json";

fn settings_path(dir: &Path) -> PathBuf {
    dir.join(FILE_NAME)
}

/// Reads `<dir>/settings.json`; a missing file is the defaults. A file that
/// isn't valid settings JSON is an error naming the file, not silently
/// replaced by defaults (the next save would lose the person's choices).
pub fn load(dir: &Path) -> Result<AppSettings> {
    let path = settings_path(dir);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(AppSettings::default()),
        Err(e) => {
            return Err(e)
                .with_context(|| format!("couldn't read settings from {}", path.display()));
        }
    };
    serde_json::from_str(&text)
        .with_context(|| format!("the settings file {} is damaged", path.display()))
}

/// Writes `<dir>/settings.json` atomically, creating `dir` if needed.
pub fn save(dir: &Path, settings: &AppSettings) -> Result<()> {
    std::fs::create_dir_all(dir)
        .with_context(|| format!("couldn't create the settings folder {}", dir.display()))?;
    let path = settings_path(dir);
    let mut json = serde_json::to_string_pretty(settings)?;
    json.push('\n');
    // Same folder, so the rename can't cross filesystems; a unique name, so
    // two saves at once can't write into each other's temp file.
    let tmp = dir.join(format!(".{FILE_NAME}.{}.tmp", uuid::Uuid::new_v4()));
    let written = (|| -> std::io::Result<()> {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(json.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&tmp, &path)
    })();
    if let Err(e) = written {
        let _ = std::fs::remove_file(&tmp);
        return Err(e).with_context(|| format!("couldn't save settings to {}", path.display()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_is_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load(dir.path()).unwrap(), AppSettings::default());
        // Loading doesn't create anything.
        assert!(!settings_path(dir.path()).exists());
    }

    #[test]
    fn saves_and_loads_creating_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("app data").join("InfinaBox");
        let settings = AppSettings {
            ai_provider: Some(ProviderId::Codex),
            godot_path: Some("/opt/godot/godot".into()),
            first_run_done: true,
            ..Default::default()
        };
        save(&nested, &settings).unwrap();
        assert_eq!(load(&nested).unwrap(), settings);

        let text = std::fs::read_to_string(nested.join("settings.json")).unwrap();
        assert!(text.contains("\n  \"ai_provider\": \"codex\""), "{text}");

        // Overwrites, and leaves no temp files behind.
        let changed = AppSettings {
            ai_provider: Some(ProviderId::ClaudeCode),
            ..settings
        };
        save(&nested, &changed).unwrap();
        assert_eq!(load(&nested).unwrap(), changed);
        let names: Vec<_> = std::fs::read_dir(&nested)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["settings.json"]);
    }

    #[test]
    fn unknown_and_missing_fields_are_fine() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("settings.json"),
            r#"{"ai_provider": "claude-code", "from_a_newer_version": [1, 2]}"#,
        )
        .unwrap();
        assert_eq!(
            load(dir.path()).unwrap(),
            AppSettings {
                ai_provider: Some(ProviderId::ClaudeCode),
                ..AppSettings::default()
            }
        );
    }

    #[test]
    fn a_corrupt_file_is_a_clear_error() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("settings.json"), "{ not json").unwrap();
        let err = format!("{:#}", load(dir.path()).unwrap_err());
        assert!(
            err.contains("damaged") && err.contains("settings.json"),
            "{err}"
        );

        std::fs::write(
            dir.path().join("settings.json"),
            r#"{"ai_provider": "someone-else"}"#,
        )
        .unwrap();
        assert!(load(dir.path()).is_err());
    }
}
