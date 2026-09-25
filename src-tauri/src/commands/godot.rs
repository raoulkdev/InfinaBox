//! Godot install/status and running the game from Studio's Play panel.
//! Owns the single running `GameProcess`, feeds its output through the
//! error parser, and emits `game-state` / `game-output` / `game-error`.
//! Phase A Task H fills this in.

use std::sync::Mutex;

use infinabox_core::godot::errors::RecentLog;
use infinabox_core::godot::run::GameProcess;
use infinabox_core::godot::{GameError, GameState, GodotStatus};
use serde::Serialize;
use tauri::AppHandle;

/// Event names emitted by this module (frontend: `src/lib/studio-api.ts`).
pub const EVENT_INSTALL_PROGRESS: &str = "godot-install-progress";
pub const EVENT_GAME_STATE: &str = "game-state";
pub const EVENT_GAME_OUTPUT: &str = "game-output";
pub const EVENT_GAME_ERROR: &str = "game-error";

#[derive(Serialize, Clone)]
pub struct GameStatePayload {
    pub state: GameState,
}

pub struct GodotInner {
    pub process: Option<GameProcess>,
    pub project_path: Option<String>,
    pub state: GameState,
    pub log: RecentLog,
}

/// Tauri-managed state: at most one running game at a time.
pub struct GodotState(pub Mutex<GodotInner>);

impl Default for GodotState {
    fn default() -> Self {
        GodotState(Mutex::new(GodotInner {
            process: None,
            project_path: None,
            state: GameState::Stopped,
            log: RecentLog::new(2000, 200),
        }))
    }
}

/// Restarts the game if it's currently running (used after an AI turn or a
/// snapshot restore changed the project). No-op otherwise.
pub fn restart_if_running(_app: &AppHandle) -> Result<(), String> {
    Ok(())
}

/// Starts (or restarts) the game for `project_path`. Shared by the
/// `game_run` command and the bridge's `RunGame`.
pub fn run_game(_app: &AppHandle, _project_path: &str) -> Result<(), String> {
    Err("not implemented yet: run_game".into())
}

pub fn stop_game(_app: &AppHandle) -> Result<(), String> {
    Err("not implemented yet: stop_game".into())
}

#[tauri::command]
pub fn godot_status(_app: AppHandle) -> Result<GodotStatus, String> {
    Err("not implemented yet: godot_status".into())
}

#[tauri::command]
pub async fn godot_install(_app: AppHandle) -> Result<GodotStatus, String> {
    Err("not implemented yet: godot_install".into())
}

#[tauri::command]
pub fn game_run(app: AppHandle, project_path: String) -> Result<(), String> {
    run_game(&app, &project_path)
}

#[tauri::command]
pub fn game_stop(app: AppHandle) -> Result<(), String> {
    stop_game(&app)
}

#[tauri::command]
pub fn game_recent_errors(_app: AppHandle, _limit: usize) -> Result<Vec<GameError>, String> {
    Err("not implemented yet: game_recent_errors".into())
}
