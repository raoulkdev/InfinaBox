//! The automatic error-fix loop in the app: feeds game errors and turn
//! starts/ends into `infinabox_core::autofix::AutoFix`, polls it on a
//! timer, starts fix turns through the same path as `agent_send`, and
//! emits `autofix-state`.
//!
//! Wave 0 stub (Phase B plan, Task G2 fills it in). The hooks below are
//! already called from `godot.rs`; they do nothing yet.

use infinabox_core::godot::{GameError, GameState};
use serde::Serialize;
use tauri::AppHandle;

/// Event name (frontend: `src/lib/studio-api.ts`).
pub const EVENT_AUTOFIX_STATE: &str = "autofix-state";

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutoFixPhase {
    Fixing,
    GaveUp,
    Idle,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AutoFixStatePayload {
    pub project_path: String,
    pub thread_id: String,
    pub state: AutoFixPhase,
    pub attempt: u32,
    pub max_attempts: u32,
}

/// Called by `godot.rs` whenever the game's state changes.
pub fn note_game_state(app: &AppHandle, state: GameState) {
    let _ = (app, state);
}

/// Called by `godot.rs` for every error the running game reports.
pub fn note_game_error(app: &AppHandle, error: &GameError) {
    let _ = (app, error);
}
