//! The loopback bridge the InfinaBox MCP server (running inside the user's
//! agent CLI) uses to reach this running app: run/stop the game, read its
//! errors and output. Protocol: `infinabox_mcp_server::bridge_protocol`.
//! Phase A Task H fills this in.

use std::sync::Mutex;

use tauri::AppHandle;

/// Where the bridge is listening and the per-launch token clients must
/// present. `None` until `start` has bound the listener.
#[derive(Clone, Debug)]
pub struct BridgeInfo {
    pub addr: String,
    pub token: String,
}

#[derive(Default)]
pub struct BridgeState(pub Mutex<Option<BridgeInfo>>);

/// Binds `127.0.0.1:0`, generates the token, stores both in `BridgeState`,
/// and serves connections on a background thread. Called once from the
/// app's `setup` hook.
pub fn start(_app: &AppHandle) -> Result<(), String> {
    Ok(())
}
