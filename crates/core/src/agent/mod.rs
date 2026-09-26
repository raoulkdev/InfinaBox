//! The AI agent runtime layer (spec §8.3): one provider-independent
//! `AgentRuntime` interface, with the Claude Code CLI as the first (and in
//! Phase A, only) implementation. Everything above this module — the Tauri
//! commands and the Studio chat UI — only ever sees `AgentEvent`s.

pub mod claude;
pub mod claude_stream;
pub mod path;
pub mod types;

pub use types::*;
