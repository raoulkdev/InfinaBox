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
pub fn app_settings_set(app: AppHandle, mut settings: AppSettings) -> Result<AppSettings, String> {
    let dir = settings_dir(&app)?;
    // Layouts have their own commands; a settings screen never overwrites them.
    settings.layouts = load_from(&dir)?.layouts;
    save_to(&dir, settings)
}

/// Everything the person has arranged (see `AppSettings::layouts`).
#[tauri::command(async)]
pub fn layouts_get(app: AppHandle) -> Result<std::collections::BTreeMap<String, serde_json::Value>, String> {
    Ok(load(&app)?.layouts)
}

/// Saves (or, with no value, forgets) one screen's layout.
#[tauri::command(async)]
pub fn layout_set(app: AppHandle, key: String, value: Option<serde_json::Value>) -> Result<(), String> {
    set_layout(&settings_dir(&app)?, &key, value)
}

pub fn set_layout(dir: &Path, key: &str, value: Option<serde_json::Value>) -> Result<(), String> {
    if key.is_empty() || key.len() > 100 {
        return Err("That layout name isn't valid.".into());
    }
    let _guard = SAVE_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let mut settings = load_from(dir)?;
    match value {
        Some(value) => settings.layouts.insert(key.to_string(), value),
        None => settings.layouts.remove(key),
    };
    app_settings::save(dir, &settings).map_err(|e| format!("Couldn't save InfinaBox's settings: {e:#}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::godot::test_support::TempDir;
    use infinabox_core::connect::ProviderId;

    #[test]
    fn layouts_are_saved_one_at_a_time_without_touching_other_settings() {
        let dir = TempDir::new("layouts");
        save_to(dir.path(), AppSettings { first_run_done: true, ..Default::default() }).unwrap();
        set_layout(dir.path(), "build", Some(serde_json::json!({"order": ["a", "b"]}))).unwrap();
        set_layout(dir.path(), "sidebar.collapsed", Some(serde_json::json!(true))).unwrap();
        set_layout(dir.path(), "build", Some(serde_json::json!({"order": ["b", "a"]}))).unwrap();
        let loaded = load_from(dir.path()).unwrap();
        assert!(loaded.first_run_done);
        assert_eq!(loaded.layouts["build"], serde_json::json!({"order": ["b", "a"]}));
        assert_eq!(loaded.layouts["sidebar.collapsed"], serde_json::json!(true));
        set_layout(dir.path(), "build", None).unwrap();
        assert!(!load_from(dir.path()).unwrap().layouts.contains_key("build"));
        assert!(set_layout(dir.path(), "", Some(serde_json::json!(1))).is_err());
    }

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
                ..Default::default()
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
