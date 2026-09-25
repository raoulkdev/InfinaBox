//! Studio chat: runs agent turns through `infinabox_core::agent` on a
//! background thread, streams each event to the frontend as `agent-event`,
//! persists everything to the project's chat store, and snapshots the
//! project after any turn that changed files. Phase A Task G fills this in.

use std::sync::Mutex;
use std::collections::HashSet;

use infinabox_core::agent::claude::ClaudeCodeRuntime;
use infinabox_core::agent::{AgentEvent, RuntimeStatus};
use infinabox_core::chat_store::{ChatRecord, ThreadSummary};
use infinabox_core::snapshot::Snapshot;
use serde::Serialize;
use tauri::AppHandle;

/// Event names emitted by this module (frontend: `src/lib/studio-api.ts`).
pub const EVENT_AGENT: &str = "agent-event";
pub const EVENT_TURN_FINISHED: &str = "agent-turn-finished";

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AgentEventPayload {
    pub thread_id: String,
    pub event: AgentEvent,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TurnFinishedPayload {
    pub thread_id: String,
    pub snapshot: Option<Snapshot>,
}

#[derive(Serialize, Clone)]
pub struct LoadedThread {
    pub thread: ThreadSummary,
    pub records: Vec<ChatRecord>,
}

/// Tauri-managed state: the one agent runtime, plus which threads have a
/// turn in flight (a second concurrent turn on the same thread is refused).
#[derive(Default)]
pub struct AgentState {
    pub runtime: ClaudeCodeRuntime,
    pub active_threads: Mutex<HashSet<String>>,
}

#[tauri::command]
pub fn agent_status(_app: AppHandle) -> Result<RuntimeStatus, String> {
    Err("not implemented yet: agent_status".into())
}

#[tauri::command]
pub fn agent_send(
    _app: AppHandle,
    _project_path: String,
    _thread_id: String,
    _message: String,
) -> Result<(), String> {
    Err("not implemented yet: agent_send".into())
}

#[tauri::command]
pub fn agent_cancel(_app: AppHandle, _thread_id: String) -> Result<(), String> {
    Err("not implemented yet: agent_cancel".into())
}

#[tauri::command]
pub fn chat_create_thread(_project_path: String, _title: String) -> Result<ThreadSummary, String> {
    Err("not implemented yet: chat_create_thread".into())
}

#[tauri::command]
pub fn chat_list_threads(_project_path: String) -> Result<Vec<ThreadSummary>, String> {
    Err("not implemented yet: chat_list_threads".into())
}

#[tauri::command]
pub fn chat_load_thread(_project_path: String, _thread_id: String) -> Result<LoadedThread, String> {
    Err("not implemented yet: chat_load_thread".into())
}
