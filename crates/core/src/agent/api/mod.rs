//! Runtimes that call a model API directly and run InfinaBox's own tool loop
//! (spec §8.1): the Anthropic Messages API, OpenAI-compatible chat
//! completions (OpenAI, or a local model through Ollama / LM Studio).
//!
//! `ApiRuntime` is the `AgentRuntime`; a `ChatBackend` is the one-call-at-a-time
//! adapter for a provider's wire format. The loop (`loop_`), the sandboxed
//! file tools (`tools`) and the bridge to the InfinaBox MCP server (`mcp`) are
//! shared by every backend.
//!
//! Phase C contract (frozen). Wave 0 stub — tasks RL (`mod` body, `loop_`,
//! `tools`, `mcp`) and RB (`openai`, `anthropic`).

pub mod anthropic;
pub mod loop_;
pub mod mcp;
pub mod openai;
pub mod tools;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use super::types::{AgentErrorKind, AgentEvent, AgentRuntime, RuntimeStatus, TurnRequest, Usage};

/// A tool the model may call, as JSON Schema.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    /// JSON Schema object (`{"type":"object","properties":{...},"required":[...]}`).
    pub parameters: serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ToolCall {
    /// The provider's id for the call, echoed in the result.
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum Message {
    User(String),
    Assistant {
        text: Option<String>,
        tool_calls: Vec<ToolCall>,
    },
    ToolResult {
        call_id: String,
        content: String,
        is_error: bool,
    },
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    Other,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Completion {
    pub text: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub usage: Option<Usage>,
    pub stop: StopReason,
}

/// Why a model call failed, in terms the loop maps to `AgentErrorKind`.
#[derive(Debug, Clone, PartialEq)]
pub enum BackendError {
    NotAuthenticated(String),
    RateLimited(String),
    /// Couldn't reach the endpoint at all.
    Unreachable(String),
    Cancelled,
    Other(String),
}

impl BackendError {
    pub fn kind(&self) -> AgentErrorKind {
        match self {
            BackendError::NotAuthenticated(_) => AgentErrorKind::NotAuthenticated,
            BackendError::RateLimited(_) => AgentErrorKind::RateLimited,
            BackendError::Unreachable(_) => AgentErrorKind::ProcessFailed,
            BackendError::Cancelled => AgentErrorKind::Cancelled,
            BackendError::Other(_) => AgentErrorKind::Other,
        }
    }
    pub fn message(&self) -> String {
        match self {
            BackendError::NotAuthenticated(m)
            | BackendError::RateLimited(m)
            | BackendError::Unreachable(m)
            | BackendError::Other(m) => m.clone(),
            BackendError::Cancelled => "Stopped.".to_string(),
        }
    }
}

/// One provider's wire format: send the conversation, get one completion.
pub trait ChatBackend: Send + Sync {
    /// Provider label for the chat ("Anthropic API", "Local model").
    fn label(&self) -> String;
    fn model(&self) -> String;
    /// Blocks until the completion arrives; checks `cancel` while waiting
    /// and returns `BackendError::Cancelled` when it is set.
    fn complete(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[ToolSpec],
        cancel: &AtomicBool,
    ) -> std::result::Result<Completion, BackendError>;
    /// Cheap reachability/credential check for `detect`: `Some(true)` when
    /// the endpoint answers and the key works, `Some(false)` when the key is
    /// refused, `None` when unknown or unreachable.
    fn check(&self) -> Option<bool> {
        None
    }
}

/// Limits of one turn (spec decision table).
pub const MAX_MODEL_CALLS: usize = 40;
pub const MAX_TOOL_RESULT_BYTES: usize = 200 * 1024;

pub struct ApiRuntime {
    pub backend: Arc<dyn ChatBackend>,
}

impl ApiRuntime {
    pub fn new(backend: Arc<dyn ChatBackend>) -> Self {
        Self { backend }
    }
}

impl AgentRuntime for ApiRuntime {
    fn detect(&self) -> RuntimeStatus {
        RuntimeStatus {
            name: self.backend.label(),
            installed: true,
            version: Some(self.backend.model()),
            logged_in: self.backend.check(),
        }
    }

    fn run_turn(&self, req: TurnRequest, on_event: &mut dyn FnMut(AgentEvent)) -> Result<()> {
        let _ = (req, on_event);
        bail!("not implemented yet (Phase C, task RL)")
    }

    fn cancel(&self, thread_id: &str) {
        let _ = thread_id;
    }
}
