//! Asset commands (`infinabox_core::assets`).
//!

use std::path::Path;

use infinabox_core::assets::{self, AssetInfo, FilePayload, HealthReport, LicenseInfo};

fn err(e: anyhow::Error) -> String {
    format!("{e:#}")
}

#[tauri::command(async)]
pub fn assets_scan(project_path: String) -> Result<Vec<AssetInfo>, String> {
    assets::scan(Path::new(&project_path)).map_err(err)
}

#[tauri::command(async)]
pub fn assets_import(
    project_path: String,
    source: String,
    dest_subdir: Option<String>,
    title: String,
    license: LicenseInfo,
) -> Result<AssetInfo, String> {
    let source = Path::new(&source);
    assets::import_file(
        Path::new(&project_path),
        source,
        dest_subdir.as_deref(),
        &title,
        &license,
    )
    .map_err(err)
}

#[tauri::command(async)]
pub fn assets_health(project_path: String) -> Result<HealthReport, String> {
    assets::health(Path::new(&project_path)).map_err(err)
}

#[tauri::command(async)]
pub fn assets_credits(project_path: String) -> Result<String, String> {
    assets::credits(Path::new(&project_path)).map_err(err)
}

#[tauri::command(async)]
pub fn assets_read_base64(
    project_path: String,
    path: String,
    max_bytes: Option<u64>,
) -> Result<FilePayload, String> {
    assets::read_base64(Path::new(&project_path), &path, max_bytes).map_err(err)
}
