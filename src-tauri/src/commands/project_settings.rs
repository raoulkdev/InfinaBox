//! Per-project settings commands (`infinabox_core::project_settings`).
//!
//! Wave 0 stub (Phase B plan, Task O2 fills it in).

use infinabox_core::project_settings::ProjectSettings;

const NOT_YET: &str = "Not implemented yet (Phase B, Task O2)";

#[tauri::command(async)]
pub fn project_settings_get(project_path: String) -> Result<ProjectSettings, String> {
    let _ = project_path;
    Err(NOT_YET.into())
}

#[tauri::command(async)]
pub fn project_settings_set(
    project_path: String,
    settings: ProjectSettings,
) -> Result<ProjectSettings, String> {
    let _ = (project_path, settings);
    Err(NOT_YET.into())
}
