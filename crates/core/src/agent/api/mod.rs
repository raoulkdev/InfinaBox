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

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

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

/// Cancel flags of the turns in flight, by thread id.
type RunningMap = Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>;

pub struct ApiRuntime {
    pub backend: Arc<dyn ChatBackend>,
    running: RunningMap,
}

impl ApiRuntime {
    pub fn new(backend: Arc<dyn ChatBackend>) -> Self {
        Self {
            backend,
            running: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

/// Holds a thread's slot in the running map for as long as its turn runs.
struct RunningGuard {
    running: RunningMap,
    thread_id: String,
    flag: Arc<AtomicBool>,
}

impl RunningGuard {
    /// Reserves `thread_id`, or `None` if a turn already holds it. Check and
    /// insert happen under one lock.
    fn reserve(running: &RunningMap, thread_id: &str) -> Option<Self> {
        let mut map = running.lock().unwrap_or_else(|e| e.into_inner());
        if map.contains_key(thread_id) {
            return None;
        }
        let flag = Arc::new(AtomicBool::new(false));
        map.insert(thread_id.to_string(), flag.clone());
        Some(Self {
            running: running.clone(),
            thread_id: thread_id.to_string(),
            flag,
        })
    }
}

impl Drop for RunningGuard {
    fn drop(&mut self) {
        let mut map = self.running.lock().unwrap_or_else(|e| e.into_inner());
        if map
            .get(&self.thread_id)
            .is_some_and(|f| Arc::ptr_eq(f, &self.flag))
        {
            map.remove(&self.thread_id);
        }
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

    /// Runs the tool loop (`loop_::run_turn`). Returns `Err` without
    /// emitting anything only when a turn is already running for the thread;
    /// otherwise every outcome — including a failure or a stop — is reported
    /// through the events, which end with `TurnCompleted`.
    fn run_turn(&self, req: TurnRequest, on_event: &mut dyn FnMut(AgentEvent)) -> Result<()> {
        let Some(turn) = RunningGuard::reserve(&self.running, &req.thread_id) else {
            bail!("A turn is already running in this chat.");
        };
        loop_::run_turn(&*self.backend, &req, &turn.flag, on_event)
    }

    /// Asks the in-flight turn for `thread_id` to stop: the backend call and
    /// any tool wait check the flag.
    fn cancel(&self, thread_id: &str) {
        let flag = self
            .running
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(thread_id)
            .cloned();
        if let Some(flag) = flag {
            flag.store(true, Ordering::SeqCst);
        }
    }
}

/// Where a local model is looked for when no address is set (Ollama).
pub const LOCAL_DEFAULT_BASE: &str = "http://localhost:11434/v1";
const OPENAI_BASE: &str = "https://api.openai.com/v1";

/// The backend for an API provider from its saved settings and the key in
/// the secret store. The error is a plain sentence for the person: what to
/// finish setting up. A CLI provider is not an API provider.
pub fn backend_for(
    provider: crate::connect::ProviderId,
    config: &crate::app_settings::ModelConfig,
    secrets: &dyn crate::secrets::SecretStore,
    model_override: Option<&str>,
) -> std::result::Result<Arc<dyn ChatBackend>, String> {
    use crate::connect::ProviderId;
    use crate::secrets::SecretName;

    let model = model_override
        .or(config.model.as_deref())
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .ok_or("Choose which model to use first (Advanced → Connect your AI).")?
        .to_string();
    let base = config.base_url.as_deref().map(str::trim).filter(|b| !b.is_empty());
    let key = |name: SecretName, what: &str| -> std::result::Result<String, String> {
        secrets
            .get(name)
            .map_err(|e| format!("{e:#}"))?
            .filter(|k| !k.trim().is_empty())
            .ok_or_else(|| format!("Paste your {what} first (Advanced → Connect your AI)."))
    };
    match provider {
        ProviderId::AnthropicApi => Ok(Arc::new(anthropic::AnthropicMessages {
            base_url: anthropic::DEFAULT_BASE.into(),
            api_key: key(SecretName::AnthropicApiKey, "Anthropic API key")?,
            model,
        })),
        ProviderId::OpenAiApi => Ok(Arc::new(openai::OpenAiCompatible {
            base_url: base.unwrap_or(OPENAI_BASE).trim_end_matches('/').into(),
            api_key: Some(key(SecretName::OpenAiApiKey, "OpenAI API key")?),
            model,
            label: "OpenAI API".into(),
        })),
        ProviderId::LocalModel => Ok(Arc::new(openai::OpenAiCompatible {
            base_url: base.unwrap_or(LOCAL_DEFAULT_BASE).trim_end_matches('/').into(),
            api_key: None,
            model,
            label: "Local model".into(),
        })),
        ProviderId::ClaudeCode | ProviderId::Codex => {
            Err("That AI runs from its own program, not an API key.".into())
        }
    }
}
