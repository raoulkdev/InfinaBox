//! App-wide settings commands (`infinabox_core::app_settings`).
//!
//! Wave 0 stub (Phase B plan, Task C2 fills it in).

use infinabox_core::app_settings::AppSettings;

const NOT_YET: &str = "Not implemented yet (Phase B, Task C2)";

#[tauri::command(async)]
pub fn app_settings_get() -> Result<AppSettings, String> {
    Err(NOT_YET.into())
}

#[tauri::command(async)]
pub fn app_settings_set(settings: AppSettings) -> Result<AppSettings, String> {
    let _ = settings;
    Err(NOT_YET.into())
}
