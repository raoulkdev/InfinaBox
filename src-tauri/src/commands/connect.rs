//! "Connect your AI" commands (spec §8.2): detection, the recommended
//! provider, a real "say hello" test turn, and running a CLI's installer or
//! sign-in inside a visible terminal (streamed as `connect-output`, ended
//! with `connect-exit`).
//!
//! Wave 0 stub (Phase B plan, Task C2 fills it in).

use infinabox_core::agent::AgentErrorKind;
use infinabox_core::connect::{ProviderId, ProviderInfo};
use serde::{Deserialize, Serialize};

/// Event names (frontend: `src/lib/studio-api.ts`).
pub const EVENT_CONNECT_OUTPUT: &str = "connect-output";
pub const EVENT_CONNECT_EXIT: &str = "connect-exit";

const NOT_YET: &str = "Not implemented yet (Phase B, Task C2)";

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectAction {
    Install,
    Login,
}

#[derive(Serialize, Clone, Debug)]
pub struct ConnectOutputPayload {
    pub data: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct ConnectExitPayload {
    pub action: ConnectAction,
    pub provider: ProviderId,
    pub success: bool,
    pub code: Option<i32>,
}

/// The result of a real one-line test turn.
#[derive(Serialize, Clone, Debug)]
pub struct ConnectionTest {
    pub ok: bool,
    pub reply: Option<String>,
    pub error_kind: Option<AgentErrorKind>,
    pub message: Option<String>,
    pub duration_ms: u64,
}

#[tauri::command(async)]
pub fn ai_providers() -> Result<Vec<ProviderInfo>, String> {
    Err(NOT_YET.into())
}

#[tauri::command(async)]
pub fn ai_recommended() -> Result<ProviderId, String> {
    Err(NOT_YET.into())
}

#[tauri::command(async)]
pub fn ai_test_connection(provider: ProviderId) -> Result<ConnectionTest, String> {
    let _ = provider;
    Err(NOT_YET.into())
}

#[tauri::command(async)]
pub fn connect_run(
    provider: ProviderId,
    action: ConnectAction,
    rows: u16,
    cols: u16,
) -> Result<(), String> {
    let _ = (provider, action, rows, cols);
    Err(NOT_YET.into())
}

#[tauri::command(async)]
pub fn connect_write(data: String) -> Result<(), String> {
    let _ = data;
    Err(NOT_YET.into())
}

#[tauri::command(async)]
pub fn connect_resize(rows: u16, cols: u16) -> Result<(), String> {
    let _ = (rows, cols);
    Err(NOT_YET.into())
}

#[tauri::command(async)]
pub fn connect_cancel() -> Result<(), String> {
    Err(NOT_YET.into())
}
