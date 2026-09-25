//! `AgentRuntime` backed by the user's own installed Claude Code CLI, driven
//! headless with streaming JSON output. Phase A Task A fills this in.

use super::types::{AgentEvent, AgentRuntime, RuntimeStatus, TurnRequest};

#[derive(Default)]
pub struct ClaudeCodeRuntime {}

impl ClaudeCodeRuntime {
    pub fn new() -> Self {
        Self::default()
    }
}

impl AgentRuntime for ClaudeCodeRuntime {
    fn detect(&self) -> RuntimeStatus {
        RuntimeStatus {
            name: "claude".to_string(),
            installed: false,
            version: None,
        }
    }

    fn run_turn(
        &self,
        _req: TurnRequest,
        _on_event: &mut dyn FnMut(AgentEvent),
    ) -> anyhow::Result<()> {
        anyhow::bail!("not implemented yet: ClaudeCodeRuntime::run_turn")
    }

    fn cancel(&self, _thread_id: &str) {}
}
