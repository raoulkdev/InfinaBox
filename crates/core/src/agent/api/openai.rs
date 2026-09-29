//! OpenAI-compatible chat completions (`POST {base}/chat/completions`): the
//! OpenAI API itself, or a local model behind Ollama
//! (`http://localhost:11434/v1`) or LM Studio. Wave 0 stub — task RB.

use std::sync::atomic::AtomicBool;

use super::{BackendError, ChatBackend, Completion, Message, ToolSpec};

pub struct OpenAiCompatible {
    /// Up to and including `/v1` (no trailing slash).
    pub base_url: String,
    /// `None` for local servers that need no key.
    pub api_key: Option<String>,
    pub model: String,
    /// "OpenAI API" or "Local model".
    pub label: String,
}

impl ChatBackend for OpenAiCompatible {
    fn label(&self) -> String {
        self.label.clone()
    }
    fn model(&self) -> String {
        self.model.clone()
    }
    fn complete(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[ToolSpec],
        cancel: &AtomicBool,
    ) -> Result<Completion, BackendError> {
        let _ = (system, messages, tools, cancel);
        Err(BackendError::Other("not implemented yet (Phase C, task RB)".into()))
    }
}
