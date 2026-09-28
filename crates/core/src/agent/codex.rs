//! `CodexRuntime`: drives the user's own `codex` CLI headless
//! (`codex exec --json`) — the second `AgentRuntime` (spec §8.1).
//!
//! Wave 0 stub (Phase B plan, Task CX fills it in).

use anyhow::bail;

use super::types::{AgentEvent, AgentRuntime, RuntimeStatus, TurnRequest};

/// The CLI's program name, looked up on the login-shell PATH.
pub const PROGRAM: &str = "codex";

pub struct CodexRuntime {
    program: String,
}

impl CodexRuntime {
    pub fn new() -> Self {
        Self::with_program(PROGRAM)
    }

    /// Uses `program` instead of `codex` (tests point this at a fake CLI).
    pub fn with_program(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
        }
    }
}

impl Default for CodexRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentRuntime for CodexRuntime {
    fn detect(&self) -> RuntimeStatus {
        RuntimeStatus {
            name: self.program.clone(),
            installed: false,
            version: None,
            logged_in: None,
        }
    }

    fn run_turn(
        &self,
        _req: TurnRequest,
        _on_event: &mut dyn FnMut(AgentEvent),
    ) -> anyhow::Result<()> {
        bail!("The Codex runtime isn't implemented yet")
    }

    fn cancel(&self, _thread_id: &str) {}
}
