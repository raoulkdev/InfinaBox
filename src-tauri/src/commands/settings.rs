//! App-wide settings commands (`infinabox_core::app_settings`), stored as
//! `settings.json` in the Tauri app data folder.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use infinabox_core::app_settings::{self, AppSettings};
use tauri::{AppHandle, Manager};

/// Serialises read-modify-write callers inside this app (a save is atomic
/// on disk either way).
static SAVE_LOCK: Mutex<()> = Mutex::new(());

pub fn settings_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|e| format!("Couldn't find the app data folder: {e}"))
}

/// The saved settings (defaults when none were saved yet).
pub fn load(app: &AppHandle) -> Result<AppSettings, String> {
    load_from(&settings_dir(app)?)
}

pub fn load_from(dir: &Path) -> Result<AppSettings, String> {
    app_settings::load(dir).map_err(|e| format!("Couldn't read InfinaBox's settings: {e:#}"))
}

/// Tidies what the person typed before it's saved: a Godot path with
/// surrounding spaces is trimmed, and an empty one means "no custom path".
fn normalize(mut settings: AppSettings) -> AppSettings {
    settings.godot_path = settings
        .godot_path
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty());
    settings
}

pub fn save_to(dir: &Path, settings: AppSettings) -> Result<AppSettings, String> {
    let settings = normalize(settings);
    let _guard = SAVE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    app_settings::save(dir, &settings)
        .map_err(|e| format!("Couldn't save InfinaBox's settings: {e:#}"))?;
    Ok(settings)
}

#[tauri::command(async)]
pub fn app_settings_get(app: AppHandle) -> Result<AppSettings, String> {
    load(&app)
}

/// Saves `settings` and returns what was saved.
#[tauri::command(async)]
pub fn app_settings_set(app: AppHandle, settings: AppSettings) -> Result<AppSettings, String> {
    save_to(&settings_dir(&app)?, settings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::godot::test_support::TempDir;
    use infinabox_core::connect::ProviderId;

    #[test]
    fn saves_tidies_and_loads_back() {
        let dir = TempDir::new("settings");
        assert_eq!(load_from(dir.path()).unwrap(), AppSettings::default());

        let saved = save_to(
            dir.path(),
            AppSettings {
                ai_provider: Some(ProviderId::Codex),
                godot_path: Some("  /opt/godot  ".into()),
                first_run_done: true,
            },
        )
        .unwrap();
        assert_eq!(saved.godot_path.as_deref(), Some("/opt/godot"));
        assert_eq!(load_from(dir.path()).unwrap(), saved);

        let saved = save_to(
            dir.path(),
            AppSettings {
                godot_path: Some("   ".into()),
                ..saved
            },
        )
        .unwrap();
        assert_eq!(saved.godot_path, None);
        assert_eq!(load_from(dir.path()).unwrap().godot_path, None);
    }

    #[test]
    fn a_damaged_file_is_an_error_not_the_defaults() {
        let dir = TempDir::new("settings-damaged");
        std::fs::write(dir.path().join("settings.json"), "{ nope").unwrap();
        let err = load_from(dir.path()).unwrap_err();
        assert!(err.contains("settings"), "{err}");
    }
}
