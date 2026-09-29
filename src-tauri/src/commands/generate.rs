//! Generation commands (`infinabox_core::generate`): run a request, then
//! accept (import with a "generated" license) or discard the preview.
//!
//! Wave 0 stubs — task X2 fills in the bodies.

use infinabox_core::assets::AssetInfo;
use infinabox_core::generate::{GenKind, GenRequest};
use infinabox_core::secrets::SecretName;
use serde::Serialize;

const NOT_YET: &str = "Not implemented yet (Phase C, task X2)";

/// A provider and whether it is connected (its secrets are all set).
#[derive(Serialize, Clone, Debug)]
pub struct GenProviderStatus {
    pub id: String,
    pub name: String,
    pub kinds: Vec<GenKind>,
    pub needs: Vec<SecretName>,
    pub blurb: String,
    pub signup_url: String,
    pub connected: bool,
}

/// A generated file waiting to be accepted or discarded.
#[derive(Serialize, Clone, Debug)]
pub struct GenPreview {
    pub temp_id: String,
    pub mime: String,
    pub base64: String,
    pub extension: String,
    pub provider: String,
    pub model: String,
    pub prompt_used: String,
    pub duration_ms: u64,
}

#[tauri::command(async)]
pub fn generate_providers() -> Result<Vec<GenProviderStatus>, String> {
    Err(NOT_YET.into())
}

#[tauri::command(async)]
pub fn generate_run(project_path: String, request: GenRequest) -> Result<GenPreview, String> {
    let _ = (project_path, request);
    Err(NOT_YET.into())
}

#[tauri::command(async)]
pub fn generate_accept(
    project_path: String,
    temp_id: String,
    title: String,
    dest_subdir: Option<String>,
) -> Result<AssetInfo, String> {
    let _ = (project_path, temp_id, title, dest_subdir);
    Err(NOT_YET.into())
}

#[tauri::command(async)]
pub fn generate_discard(temp_id: String) -> Result<(), String> {
    let _ = temp_id;
    Err(NOT_YET.into())
}
