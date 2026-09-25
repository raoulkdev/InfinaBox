//! Thin Tauri wrappers over `infinabox_core::snapshot` for Studio's History
//! panel. Restore/undo emit `snapshots-changed` and restart a running game.
//! Phase A Task I fills this in.

use infinabox_core::snapshot::Snapshot;
use tauri::AppHandle;

/// Payload-less event, like `project-fs-changed`.
pub const EVENT_SNAPSHOTS_CHANGED: &str = "snapshots-changed";

#[tauri::command]
pub fn snapshot_list(_project_path: String, _limit: usize) -> Result<Vec<Snapshot>, String> {
    Err("not implemented yet: snapshot_list".into())
}

#[tauri::command]
pub fn snapshot_create(_app: AppHandle, _project_path: String, _title: String) -> Result<Option<Snapshot>, String> {
    Err("not implemented yet: snapshot_create".into())
}

#[tauri::command]
pub fn snapshot_restore(_app: AppHandle, _project_path: String, _snapshot_id: String) -> Result<Snapshot, String> {
    Err("not implemented yet: snapshot_restore".into())
}

#[tauri::command]
pub fn snapshot_undo_last(_app: AppHandle, _project_path: String) -> Result<Option<Snapshot>, String> {
    Err("not implemented yet: snapshot_undo_last".into())
}
