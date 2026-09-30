//! Agent contract types (frozen — see the "Wave 0 contracts" sections of
//! `docs/superpowers/plans/2026-09-25-phase-a-foundations.md` and
//! `docs/superpowers/plans/2026-09-28-phase-b-first-run.md`). Mirrored for
//! the frontend in `src/lib/studio-types.ts`; change both together, and only
//! through the plan's lead.

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
    /// The agent proposed a plan with the `propose_plan` MCP tool and is
    /// waiting for the person to approve it. Emitted by each runtime's
    /// stream parser in place of that tool call's `ToolUse`.
    PlanProposed {
        title: String,
        steps: Vec<String>,
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
    /// The person pressed Stop. Shown as a neutral note, not an error.
    Cancelled,
}

/// The provider-independent interface from spec §8.3. Implemented by the
/// Claude Code CLI (Phase A) and the Codex CLI (Phase B); API-key and local
/// runtimes come later.
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
    pub options: TurnOptions,
}

/// How the agent should behave this turn: the project's settings plus why
/// the message was sent. Runtimes turn it into instructions with
/// `agent::prompt::system_prompt`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TurnOptions {
    /// Which specialist the message is for (Phase C). `Director` adds nothing.
    pub role: Role,
    pub plan_policy: PlanPolicy,
    /// "Teach me" mode: explanations grow into short lessons.
    pub teach: bool,
    pub origin: MessageOrigin,
    /// A model to use for this message instead of the AI's default (a
    /// CLI alias like `sonnet`, or a full model name). Already checked by
    /// `clean_model`.
    pub model: Option<String>,
    /// How hard the AI should think. Ignored by runtimes that can't set it.
    pub effort: Option<Effort>,
}

/// How much thinking effort a turn gets (what the AI's CLI calls effort or
/// reasoning effort). Not every AI offers every level.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Effort {
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

impl Effort {
    pub fn as_str(self) -> &'static str {
        match self {
            Effort::Low => "low",
            Effort::Medium => "medium",
            Effort::High => "high",
            Effort::Xhigh => "xhigh",
            Effort::Max => "max",
        }
    }
}

/// A model name safe to pass on a command line: letters, digits and
/// `. _ - : /` only, at most 100 characters. Blank means "the default".
pub fn clean_model(model: &str) -> Option<String> {
    let model = model.trim();
    let ok = !model.is_empty()
        && model.len() <= 100
        && model
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ':' | '/'))
        && !model.starts_with('-');
    ok.then(|| model.to_string())
}

/// A specialist the Director hands work to (spec §7.1). A role changes the
/// instructions the agent gets, not its tools.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    #[default]
    Director,
    Designer,
    Programmer,
    Artist,
    Sound,
    Qa,
    Producer,
    Marketer,
}

impl Role {
    pub const ALL: [Role; 8] = [
        Role::Director,
        Role::Designer,
        Role::Programmer,
        Role::Artist,
        Role::Sound,
        Role::Qa,
        Role::Producer,
        Role::Marketer,
    ];

    /// The file name (without extension) of the role's prompt section.
    pub fn slug(self) -> &'static str {
        match self {
            Role::Director => "director",
            Role::Designer => "designer",
            Role::Programmer => "programmer",
            Role::Artist => "artist",
            Role::Sound => "sound",
            Role::Qa => "qa",
            Role::Producer => "producer",
            Role::Marketer => "marketer",
        }
    }
}

/// When the agent must propose a plan before changing the game (spec §5.4).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanPolicy {
    /// Every change to the game gets a plan the person approves first.
    #[default]
    AlwaysPlan,
    /// Small, single-step changes are made directly; bigger ones get a plan.
    SmallChangesDirect,
}

/// Why a message was sent. Changes the instructions the agent gets (an
/// approved plan or an auto-fix is never re-planned) and how the chat
/// shows it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageOrigin {
    /// Typed by the person.
    #[default]
    User,
    /// The person pressed Approve on a plan.
    PlanApproval,
    /// InfinaBox sent the game's errors back automatically.
    AutoFix,
    /// The first build after the onboarding interview.
    FirstBuild,
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
    /// From the CLI's own sign-in status command; None when it can't tell.
    pub logged_in: Option<bool>,
}
