//! Generation commands (`infinabox_core::generate`): run a request, then
//! accept (import with a "generated" license) or discard the preview.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use base64::Engine;
use infinabox_core::assets::{self, AssetInfo, LicenseInfo};
use infinabox_core::context_cards;
use infinabox_core::generate::{self, GenKind, GenRequest, GenResult};
use infinabox_core::secrets::SecretName;
use serde::Serialize;
use tauri::State;

use super::credentials::SecretState;

/// Generated files waiting to be accepted or discarded, by temp id. Their
/// bytes live in a temp folder that is emptied on discard, accept and next
/// start.
#[derive(Default)]
pub struct GenState {
    pending: Mutex<HashMap<String, GenResult>>,
}

fn temp_root() -> PathBuf {
    std::env::temp_dir().join("infinabox-generated")
}

fn err(e: anyhow::Error) -> String {
    format!("{e:#}")
}

/// Deletes previews left behind by an earlier run.
pub fn clear_leftovers() {
    let _ = std::fs::remove_dir_all(temp_root());
}

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
pub fn generate_providers(secrets: State<SecretState>) -> Result<Vec<GenProviderStatus>, String> {
    generate::generators()
        .iter()
        .map(|g| {
            let info = g.info();
            let connected = info
                .needs
                .iter()
                .all(|n| secrets.0.is_set(*n).unwrap_or(false));
            Ok(GenProviderStatus {
                id: info.id,
                name: info.name,
                kinds: info.kinds,
                needs: info.needs,
                blurb: info.blurb,
                signup_url: info.signup_url,
                connected,
            })
        })
        .collect()
}

#[tauri::command(async)]
pub fn generate_run(
    secrets: State<SecretState>,
    gen: State<GenState>,
    project_path: String,
    mut request: GenRequest,
) -> Result<GenPreview, String> {
    if request.style_guide.is_none() {
        request.style_guide = context_cards::read_card(Path::new(&project_path), "style-guide.md")
            .ok()
            .map(|card| card.body)
            .filter(|body| !body.trim().is_empty());
    }
    let out_dir = temp_root();
    std::fs::create_dir_all(&out_dir)
        .map_err(|e| format!("Couldn't make a temporary folder: {e}"))?;
    let result = generate::run(&request, secrets.0.as_ref(), &out_dir).map_err(err)?;
    let bytes = std::fs::read(&result.file)
        .map_err(|e| format!("Couldn't read the generated file: {e}"))?;
    let temp_id = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    );
    let preview = GenPreview {
        temp_id: temp_id.clone(),
        mime: result.mime.clone(),
        base64: base64::engine::general_purpose::STANDARD.encode(&bytes),
        extension: result.extension.clone(),
        provider: result.provider.clone(),
        model: result.model.clone(),
        prompt_used: result.prompt_used.clone(),
        duration_ms: result.duration_ms,
    };
    gen.pending.lock().unwrap_or_else(|e| e.into_inner()).insert(temp_id, result);
    Ok(preview)
}

#[tauri::command(async)]
pub fn generate_accept(
    gen: State<GenState>,
    project_path: String,
    temp_id: String,
    title: String,
    dest_subdir: Option<String>,
) -> Result<AssetInfo, String> {
    let result = gen
        .pending
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&temp_id)
        .ok_or("That preview is gone. Generate it again.")?;
    // Named after the title so the imported file has a readable name.
    let named = result
        .file
        .with_file_name(format!("{}.{}", file_stem(&title), result.extension));
    let source = match std::fs::rename(&result.file, &named) {
        Ok(()) => named,
        Err(_) => result.file.clone(),
    };
    let license = LicenseInfo {
        name: Some("Generated".into()),
        source: Some("Generated".into()),
        author: None,
        url: None,
        generated_by: Some(format!("{} / {}", result.provider, result.model)),
    };
    let imported = assets::import_file(
        Path::new(&project_path),
        &source,
        dest_subdir.as_deref(),
        &title,
        &license,
    );
    let _ = std::fs::remove_file(&source);
    imported.map_err(err)
}

#[tauri::command(async)]
pub fn generate_discard(gen: State<GenState>, temp_id: String) -> Result<(), String> {
    if let Some(result) = gen.pending.lock().unwrap_or_else(|e| e.into_inner()).remove(&temp_id) {
        let _ = std::fs::remove_file(&result.file);
    }
    Ok(())
}

/// A file name from a title: lowercase letters, digits and dashes.
fn file_stem(title: &str) -> String {
    let stem: String = title
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let stem = stem.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-");
    if stem.is_empty() { "generated".into() } else { stem }
}
