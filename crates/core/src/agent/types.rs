//! Phase A contract types (frozen — see
//! `docs/superpowers/plans/2026-09-25-phase-a-foundations.md`, "Wave 0
//! contracts"). Mirrored for the frontend in `src/lib/studio-types.ts`;
//! change both together, and only through the plan's lead.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// One thing that happened during an agent turn, normalized from whatever
/// the underlying runtime (Phase A: Claude Code CLI stream-json) emits.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    SessionStarted {
        provider_session_id: String,
        model: Option<String>,
    },
    AssistantText {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        summary: String,
    },
    ToolResult {
        id: String,
        ok: bool,
        summary: String,
    },
    /// Paths (project-relative) the agent created or edited this turn,
    /// derived from real file-editing tool uses.
    FilesChanged {
        paths: Vec<String>,
    },
    TurnCompleted {
        is_error: bool,
        duration_ms: Option<u64>,
        usage: Option<Usage>,
    },
    Error {
        kind: AgentErrorKind,
        message: String,
    },
}

/// Only ever filled from figures the provider actually reported — never
/// estimated.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Usage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentErrorKind {
    NotInstalled,
    NotAuthenticated,
    RateLimited,
    ProcessFailed,
    Other,
}

/// The provider-independent interface from spec §8.3. Phase A has one
/// implementation (Claude Code CLI); Codex/API-key/local come later.
pub trait AgentRuntime: Send + Sync {
    fn detect(&self) -> RuntimeStatus;
    /// Runs one user turn. Blocks the calling thread until the turn ends;
    /// every event is delivered through `on_event` as it arrives.
    fn run_turn(
        &self,
        req: TurnRequest,
        on_event: &mut dyn FnMut(AgentEvent),
    ) -> anyhow::Result<()>;
    /// Stops the in-flight turn for `thread_id`, if any.
    fn cancel(&self, thread_id: &str);
}

#[derive(Clone, Debug)]
pub struct TurnRequest {
    pub thread_id: String,
    pub project_path: PathBuf,
    pub message: String,
    /// Present from the second turn on, so the runtime resumes the same
    /// provider-side conversation.
    pub resume_provider_session_id: Option<String>,
    pub mcp: McpLaunch,
}

/// How the runtime should launch the InfinaBox MCP server for the agent.
#[derive(Clone, Debug)]
pub struct McpLaunch {
    pub command: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct RuntimeStatus {
    pub name: String,
    pub installed: bool,
    pub version: Option<String>,
}
