//! Asset commands (`infinabox_core::assets`).
//!
//! Wave 0 stubs — task X2 fills in the bodies.

use infinabox_core::assets::{AssetInfo, FilePayload, HealthReport, LicenseInfo};

const NOT_YET: &str = "Not implemented yet (Phase C, task X2)";

#[tauri::command(async)]
pub fn assets_scan(project_path: String) -> Result<Vec<AssetInfo>, String> {
    let _ = project_path;
    Err(NOT_YET.into())
}

#[tauri::command(async)]
pub fn assets_import(
    project_path: String,
    source: String,
    dest_subdir: Option<String>,
    title: String,
    license: LicenseInfo,
) -> Result<AssetInfo, String> {
    let _ = (project_path, source, dest_subdir, title, license);
    Err(NOT_YET.into())
}

#[tauri::command(async)]
pub fn assets_health(project_path: String) -> Result<HealthReport, String> {
    let _ = project_path;
    Err(NOT_YET.into())
}

#[tauri::command(async)]
pub fn assets_credits(project_path: String) -> Result<String, String> {
    let _ = project_path;
    Err(NOT_YET.into())
}

#[tauri::command(async)]
pub fn assets_read_base64(
    project_path: String,
    path: String,
    max_bytes: Option<u64>,
) -> Result<FilePayload, String> {
    let _ = (project_path, path, max_bytes);
    Err(NOT_YET.into())
}
