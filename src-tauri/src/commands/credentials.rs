//! Provider credentials in the OS keychain (`infinabox_core::secrets`).
//! Values go in and never come back out: `credential_status` only says
//! whether a name is set.
//!
//! Wave 0 stubs — task X2 fills in the bodies.

use infinabox_core::secrets::SecretName;
use serde::Serialize;

const NOT_YET: &str = "Not implemented yet (Phase C, task X2)";

#[derive(Serialize, Clone, Debug)]
pub struct CredentialStatus {
    pub name: SecretName,
    pub connected: bool,
}

#[tauri::command(async)]
pub fn credential_status(names: Vec<SecretName>) -> Result<Vec<CredentialStatus>, String> {
    let _ = names;
    Err(NOT_YET.into())
}

#[tauri::command(async)]
pub fn credential_set(name: SecretName, value: String) -> Result<(), String> {
    let _ = (name, value);
    Err(NOT_YET.into())
}

#[tauri::command(async)]
pub fn credential_clear(name: SecretName) -> Result<(), String> {
    let _ = name;
    Err(NOT_YET.into())
}
