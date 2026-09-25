//! Chat threads persisted as `.ibproject/chat/<thread_id>.jsonl` inside the
//! project (committed with it, spec §9). The first line is the thread
//! header; every line after is a `ChatRecord`. Every string is passed
//! through `redact::redact` before it touches disk. Phase A Task D fills
//! this in.

use std::path::Path;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::agent::AgentEvent;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ThreadSummary {
    pub id: String,
    pub title: String,
    /// Unix seconds.
    pub created_at: i64,
    pub provider: String,
    pub provider_session_id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChatRecord {
    User { text: String, at: i64 },
    Event { event: AgentEvent, at: i64 },
}

pub fn create_thread(_project: &Path, _title: &str, _provider: &str) -> Result<ThreadSummary> {
    anyhow::bail!("not implemented yet: chat_store::create_thread")
}

pub fn append(_project: &Path, _thread_id: &str, _record: &ChatRecord) -> Result<()> {
    anyhow::bail!("not implemented yet: chat_store::append")
}

pub fn set_provider_session(_project: &Path, _thread_id: &str, _session_id: &str) -> Result<()> {
    anyhow::bail!("not implemented yet: chat_store::set_provider_session")
}

/// Newest first.
pub fn list_threads(_project: &Path) -> Result<Vec<ThreadSummary>> {
    anyhow::bail!("not implemented yet: chat_store::list_threads")
}

pub fn load_thread(_project: &Path, _thread_id: &str) -> Result<(ThreadSummary, Vec<ChatRecord>)> {
    anyhow::bail!("not implemented yet: chat_store::load_thread")
}
