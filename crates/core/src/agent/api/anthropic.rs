//! The Anthropic Messages API (`POST {base}/v1/messages`). Wave 0 stub —
//! task RB.

use std::sync::atomic::AtomicBool;

use super::{BackendError, ChatBackend, Completion, Message, ToolSpec};

pub const DEFAULT_BASE: &str = "https://api.anthropic.com";
pub const BASE_ENV: &str = "INFINABOX_ANTHROPIC_BASE";

pub struct AnthropicMessages {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

impl ChatBackend for AnthropicMessages {
    fn label(&self) -> String {
        "Anthropic API".into()
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
