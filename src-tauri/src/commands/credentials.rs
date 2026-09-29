//! Provider credentials in the OS keychain (`infinabox_core::secrets`).
//! Values go in and never come back out: `credential_status` only says
//! whether a name is set.

use std::sync::Arc;

use infinabox_core::secrets::{KeychainStore, MemoryStore, SecretName, SecretStore};
use serde::Serialize;
use tauri::State;

/// The app's secret store: the OS keychain. `INFINABOX_SECRETS=memory`
/// keeps secrets in memory only, for tests on machines with no keychain.
pub struct SecretState(pub Arc<dyn SecretStore>);

impl Default for SecretState {
    fn default() -> Self {
        if std::env::var("INFINABOX_SECRETS").as_deref() == Ok("memory") {
            SecretState(Arc::new(MemoryStore::default()))
        } else {
            SecretState(Arc::new(KeychainStore))
        }
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct CredentialStatus {
    pub name: SecretName,
    pub connected: bool,
}

#[tauri::command(async)]
pub fn credential_status(
    secrets: State<SecretState>,
    names: Vec<SecretName>,
) -> Result<Vec<CredentialStatus>, String> {
    names
        .into_iter()
        .map(|name| {
            let connected = secrets.0.is_set(name).map_err(|e| format!("{e:#}"))?;
            Ok(CredentialStatus { name, connected })
        })
        .collect()
}

#[tauri::command(async)]
pub fn credential_set(
    secrets: State<SecretState>,
    name: SecretName,
    value: String,
) -> Result<(), String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("Paste the key first.".into());
    }
    secrets.0.set(name, value).map_err(|e| format!("{e:#}"))
}

#[tauri::command(async)]
pub fn credential_clear(secrets: State<SecretState>, name: SecretName) -> Result<(), String> {
    secrets.0.delete(name).map_err(|e| format!("{e:#}"))
}
