//! The instructions every runtime gives the agent: the Director prompt,
//! plus sections chosen by the turn's `TurnOptions` (plan policy, why the
//! message was sent, teach mode), plus the project's own `AGENTS.md`.
//! Shared so Claude Code and Codex get exactly the same instructions.
//!
//! Wave 0 stub (Phase B plan, Task PL fills it in).

use super::types::TurnOptions;

/// The Director system prompt.
pub const DIRECTOR_PROMPT: &str = include_str!("prompts/director.md");

/// The InfinaBox MCP tool the agent calls to show the person a plan
/// (`crates/mcp-server`). Runtimes turn a call to it into
/// `AgentEvent::PlanProposed`.
pub const PROPOSE_PLAN_TOOL: &str = "propose_plan";

/// The full instructions for one turn. `agents_md` is the project's
/// `AGENTS.md`, already read (and size-capped) by the caller.
pub fn system_prompt(options: &TurnOptions, agents_md: Option<&str>) -> String {
    let _ = options;
    match agents_md {
        Some(extra) => format!("{DIRECTOR_PROMPT}\n\n{extra}"),
        None => DIRECTOR_PROMPT.to_string(),
    }
}
