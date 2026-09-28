//! The AI agent runtime layer (spec §8.3): one provider-independent
//! `AgentRuntime` interface, implemented for the Claude Code CLI and the
//! Codex CLI, with the instructions both get built in `prompt`. Everything
//! above this module — the Tauri commands and the Studio chat UI — only
//! ever sees `AgentEvent`s.

pub mod claude;
pub mod claude_stream;
pub mod codex;
pub mod codex_stream;
pub mod path;
pub mod prompt;
pub mod types;

pub use types::*;
