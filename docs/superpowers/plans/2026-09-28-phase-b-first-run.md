# Phase B: The first-run experience — implementation plan

> **For agentic workers:** built the same way as Phase A — **subagents develop multiple tasks at the same time**, each in its own git worktree, against contracts the lead freezes first (Wave 0). The lead coordinates, reviews (with reviewer subagents), merges and verifies; it does not implement Wave 1 or Wave 2 tasks itself.

**Goal:** a person with no game-dev knowledge opens InfinaBox, connects the AI they already pay for, answers a short interview about their game, and plays a personalized prototype built from a polished template — with plans they approve, explanations they can learn from, and errors fixed automatically (spec §6.2, §14 Phase B).

**Spec:** `docs/superpowers/specs/2026-09-25-ai-game-studio-product-spec.md` (§6.1–6.2, §7.1, §7.4, §8, §11, §14 Phase B). **Builds on:** `docs/superpowers/plans/2026-09-25-phase-a-foundations.md` (its Global constraints and execution model apply unchanged, and are included in every subagent prompt).

## Status

Not started.

## Scope

**In Phase B** (spec §14):
1. **Connect your AI** (§8.2) for **Claude Code** and **Codex**: detect installed + signed in, recommend one, install it in a visible terminal, sign in from a button, "say hello" test, choose the active one. A second `AgentRuntime`: `CodexRuntime` (`codex exec --json`).
2. **Onboarding interview → starter Context**: seven questions, a starter Context (concept, style guide, first milestone, plus the template's own mechanic cards), a template choice with a plain reason, a first build turn, then Play.
3. **Three polished 2D templates**: `platformer-2d`, `topdown-2d` (adventure), `shooter-2d` (twin-stick arena). `blank-2d` stays as "Start from scratch".
4. **Plan/approve UI, explanations, auto error-fix loop**: the agent proposes plans through a `propose_plan` MCP tool rendered as an approvable card; a per-project "small changes without a plan" setting; a "Teach me" setting; errors in the running game are fed back to the agent automatically (at most 2 attempts in a row), shown as "Something broke — fixing it".
5. **The 6-entry navigation** (§11): Home, Studio, Context, Assets, Playtest & Launch, Advanced. The old discipline sections are retired.
6. **Phase A follow-ups**: Stop reads as a neutral note; no "No files changed" snapshots; `stage_everything` retries a file truncated mid-read.

**Not in Phase B:** 3D templates (the interview says plainly that games start in 2D for now), typed Context cards UI beyond a card browser/editor, assets, Producer journey, API-key/local runtimes, generation providers, accounts/Stripe (spec §15 is Phase D), embedding the game window, GitHub backup, removing the addon from settings.

## Decisions made for this phase (the user delegated these)

| Question | Decision | Why |
|---|---|---|
| Which 3 templates? | Platformer, Top-down adventure, Twin-stick arena shooter | All three put "something that moves and is yours" on screen in the first minute, and they cover the genres novices name most. Puzzle/visual novel need content before they're fun. |
| Who asks the interview questions? | A fixed, friendly question flow in the UI (chips + free text), not the AI | Fast, works before any AI turn succeeds, can't wander, and costs the user no AI usage. The AI does the creative part (the first build). |
| Who drafts the starter Context? | InfinaBox, deterministically, from the user's own words + the template's cards | Real and instant; nothing invented. The first build turn asks the AI to refine the cards as it builds. |
| Who picks the template? | The user's genre answer; for "Something else", keyword matching over their idea, with a plain reason ("Your idea mentions *jump* and *levels*, so we start from the Platformer") | Deterministic, explainable, testable. |
| Plan policy default | **Always plan first** (spec §5.4); opt-in "Small changes without a plan" per project | Matches "the user is the director"; auto-fix and approved-plan turns never re-plan. |
| How plans reach the UI | An MCP tool `propose_plan(title, steps)`; runtimes turn its tool call into a `plan_proposed` event | Structured and provider-independent (both CLIs call MCP tools), no fragile text parsing. |
| Auto-fix scope | Errors from a game run while no turn is running, on the project's most recent thread; max 2 fix turns in a row until the user sends a message; stops early if the same errors come back; on by default, per-project toggle | Fixes the common case without runaway loops or silent AI usage. |
| Codex sandboxing | `codex exec --json --sandbox workspace-write`, InfinaBox MCP server only, the user's own config and project `.codex/` not loaded where the CLI allows it | Mirrors the Claude restrictions from Phase A (`--setting-sources user`, `--strict-mcp-config`). Exact flags are verified against the real CLI in Task CX. |
| Install commands | Claude Code: the official native installer (`curl -fsSL https://claude.ai/install.sh \| bash`; Windows: `irm https://claude.ai/install.ps1 \| iex`). Codex: `npm install -g @openai/codex` (or `brew install --cask codex` on macOS when Homebrew exists). Sign in: `claude auth login`, `codex login` | The vendors' own documented installers, run visibly in a terminal the user watches. |
| Where settings live | App-wide (AI choice, custom Godot path, first run done): `settings.json` in the app data dir. Per project (plan policy, teach, auto-fix): `.ibproject/settings.json`, committed | Per-project behavior travels with the game; nothing secret is stored anywhere. |
| New navigation contents | Home (projects, AI + Godot status, "New game"), Studio, Context (card browser/editor over `.ibproject/context/` + Graphs tab), Assets and Playtest & Launch (honest "arrives in Phase C/D" placeholders), Advanced (terminal + code editor + git, settings, Open in Godot) | Spec §11. Placeholders follow the codebase's `NotBuiltYetSection` honesty rule. |

## Global constraints

Phase A's "Global constraints" section applies verbatim (real not fabricated, match the codebase, testable logic in `crates/core`, tests use real things, no frontend test runner, verify external tools, never destroy user work, async Tauri commands, stay in your lane, verification commands). Additions:

- **Disk is limited.** Rust tasks build with their own target dir and no debug info: `export CARGO_TARGET_DIR=/home/user/ib-targets/<task> CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`. Core-only tasks test with `cargo test -p infinabox-core --no-default-features` (no Tauri build). Delete your target dir when you finish.
- **Frontend worktrees** symlink the main checkout's `node_modules` (`ln -s /home/user/InfinaBox/node_modules node_modules`) instead of installing.
- **Real tools available:** Godot 4.7.2 at `/tmp/claude-0/godot/Godot_v4.7.2-stable_linux.x86_64` (use as `INFINABOX_GODOT`), a logged-in `claude` CLI, and `codex` 0.157.1 at `/tmp/claude-0/codex/node_modules/.bin/codex` (not logged in).

## Execution model

Same roles and process as Phase A: lead (Wave 0, merges, Wave 3), one implementer subagent per task in its own worktree (background), a reviewer subagent per task before merge, findings sent back to the same implementer.

```
Wave 0  (lead)            contracts, stubs, template skeletons, this plan
Wave 1  (12 in parallel)  CX Codex runtime · PL plans+prompts+project settings · CN connect detection+app settings
                          OB onboarding+scaffold · AF auto-fix policy+chat/snapshot follow-ups
                          T1 platformer · T2 top-down · T3 shooter
                          FN navigation · FH Home+connect UI · FO onboarding UI · FC Studio chat UI
Wave 2  (3 in parallel)   G2 agent+autofix commands · C2 connect+settings+Godot commands · O2 onboarding+project-settings commands
Wave 3  (lead + fix-ups)  integrate, E2E (first run end to end with real Godot + real Claude), docs
```

Merge order, Wave 1: AF, CN, PL, CX, T1, T2, T3, OB, FN, FH, FO, FC. Wave 2: C2, O2, G2.

### File ownership

| Task | Owns |
|---|---|
| Lead (W0) | every contract/stub file below; `Cargo.toml`s; `src-tauri/src/lib.rs`, `commands/mod.rs`, `crates/core/src/lib.rs`, `agent/mod.rs`, `agent/types.rs`; `src/lib/studio-types.ts`, `src/lib/studio-api.ts` |
| CX | `crates/core/src/agent/codex.rs`, `agent/codex_stream.rs`, `crates/core/tests/fixtures/codex/**`, `scripts/record-codex-fixtures.mjs`, `scripts/fixtures/mock-responses-server.mjs` |
| PL | `crates/core/src/agent/prompt.rs`, `agent/prompts/**`, `agent/claude.rs`, `agent/claude_stream.rs`, `crates/core/src/project_settings.rs`, `crates/mcp-server/src/server.rs` |
| CN | `crates/core/src/connect.rs`, `crates/core/src/app_settings.rs`, `crates/core/src/agent/path.rs` |
| OB | `crates/core/src/onboarding.rs`, `crates/core/src/scaffold.rs`, `templates/blank-2d/**` |
| AF | `crates/core/src/autofix.rs`, `crates/core/src/chat_store.rs`, `crates/core/src/snapshot.rs` |
| T1 / T2 / T3 | `templates/platformer-2d/**` / `templates/topdown-2d/**` / `templates/shooter-2d/**` |
| FN | `src/App.tsx`, `src/components/cockpit/Sidebar.tsx`, `src/components/context/**`, `src/components/advanced/**`, retiring `src/components/cockpit/{Design,Business,Marketing,Community,Release}Section.tsx` |
| FH | `src/components/cockpit/DashboardSection.tsx`, `src/components/connect/**` |
| FO | `src/components/onboarding/**` |
| FC | `src/components/studio/**` |
| G2 | `src-tauri/src/commands/agent.rs`, `src-tauri/src/commands/autofix.rs` |
| C2 | `src-tauri/src/commands/connect.rs`, `src-tauri/src/commands/settings.rs`, `src-tauri/src/commands/godot.rs` |
| O2 | `src-tauri/src/commands/onboarding.rs`, `src-tauri/src/commands/project_settings.rs`, `src-tauri/src/commands/scaffold.rs` |

## Wave 0 contracts

Frozen for Waves 1–2. Rust and TypeScript mirror each other (serde `snake_case`; TS fields are snake_case like Phase A's types; event payloads are camelCase like Phase A's payloads).

### Agent types (`crates/core/src/agent/types.rs`)

Additions to Phase A's contract:

```rust
pub enum AgentEvent {
    // ...Phase A variants unchanged...
    /// The agent proposed a plan with the `propose_plan` MCP tool and is
    /// waiting for the person to approve it. Emitted by each runtime's
    /// stream parser in place of that tool call's `ToolUse`.
    PlanProposed { title: String, steps: Vec<String> },
}

pub enum AgentErrorKind { NotInstalled, NotAuthenticated, RateLimited, ProcessFailed, Other,
    /// The person pressed Stop. Shown as a neutral note, not an error.
    Cancelled }

pub struct TurnRequest { /* Phase A fields */ pub options: TurnOptions }

#[derive(Clone, Debug, Default)]
pub struct TurnOptions { pub plan_policy: PlanPolicy, pub teach: bool, pub origin: MessageOrigin }

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanPolicy { #[default] AlwaysPlan, SmallChangesDirect }

/// Why a message was sent. Changes the instructions the agent gets (an
/// approved plan or an auto-fix is never re-planned) and how the chat shows it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageOrigin { #[default] User, PlanApproval, AutoFix, FirstBuild }

pub struct RuntimeStatus { pub name: String, pub installed: bool, pub version: Option<String>,
    /// From the CLI's own status command; None when it can't tell.
    pub logged_in: Option<bool> }
```

`agent/prompt.rs` (PL): `pub fn system_prompt(options: &TurnOptions, agents_md: Option<&str>) -> String` — the Director prompt plus policy/origin/teach sections plus the project's `AGENTS.md`; used by both runtimes. `pub const PROPOSE_PLAN_TOOL: &str = "propose_plan";`.

`agent/codex.rs` (CX): `pub struct CodexRuntime` with `new()`, `with_program(..)`, implementing `AgentRuntime`; `detect()` fills `logged_in` from `codex login status`. `ClaudeCodeRuntime::detect()` (PL) fills it from `claude auth status --json`.

### Chat store (`crates/core/src/chat_store.rs`)

```rust
pub enum ChatRecord {
    User { text: String, at: i64,
           #[serde(default, skip_serializing_if = "Option::is_none")] origin: Option<MessageOrigin> },
    Event { event: AgentEvent, at: i64 },
}
/// Switches a thread to another provider (the person changed their AI):
/// sets `provider` and clears `provider_session_id`.
pub fn set_provider(project: &Path, thread_id: &str, provider: &str) -> Result<()>;
```

### Connect + app settings (`crates/core/src/connect.rs`, `app_settings.rs`)

```rust
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProviderId { #[serde(rename = "claude-code")] ClaudeCode, #[serde(rename = "codex")] Codex }

#[derive(Serialize, Clone, Debug)]
pub struct ProviderInfo {
    pub id: ProviderId, pub name: String, pub installed: bool, pub version: Option<String>,
    pub logged_in: Option<bool>,
    /// Who it suits and what it costs, in plain words (static copy, no prices invented).
    pub blurb: String,
    /// The exact command the installer runs, shown to the user before it runs.
    pub install_command: Option<String>,
    /// Why it can't be installed from here (e.g. needs Node.js), if so.
    pub install_blocker: Option<String>,
    pub login_command: String,
    pub docs_url: String,
}
pub fn detect_all() -> Vec<ProviderInfo>;
pub fn detect(id: ProviderId) -> ProviderInfo;
/// Program + args for running the installer / sign-in inside a PTY.
pub fn install_invocation(id: ProviderId) -> Result<(String, Vec<String>)>;
pub fn login_invocation(id: ProviderId) -> Result<(String, Vec<String>)>;
/// The recommended provider: the first installed+signed-in, else installed, else Claude Code.
pub fn recommend(infos: &[ProviderInfo]) -> ProviderId;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct AppSettings { pub ai_provider: Option<ProviderId>, pub godot_path: Option<String>, pub first_run_done: bool }
pub fn load(dir: &Path) -> Result<AppSettings>;   // missing file → Default
pub fn save(dir: &Path, s: &AppSettings) -> Result<()>;  // atomic
```

### Project settings (`crates/core/src/project_settings.rs`)

```rust
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ProjectSettings { pub plan_policy: PlanPolicy, pub teach: bool, pub auto_fix: bool }
impl Default for ProjectSettings { /* AlwaysPlan, teach false, auto_fix true */ }
pub fn load(project: &Path) -> Result<ProjectSettings>; // `.ibproject/settings.json`; missing → default; unknown fields ignored
pub fn save(project: &Path, s: &ProjectSettings) -> Result<()>;
```

### Onboarding + templates (`crates/core/src/onboarding.rs`, `scaffold.rs`)

```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TemplateInfo { pub id: String, pub name: String, pub description: String, pub dimension: String,
    pub controls: String, pub features: Vec<String>, pub keywords: Vec<String> }
pub fn list_templates() -> Vec<TemplateInfo>;              // scaffold.rs, from each template's `template.json`
pub fn create_project_from_template(parent: &Path, name: &str, template_id: &str) -> Result<PathBuf>; // scaffold.rs
// `create_project(parent, name)` stays = blank-2d.

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InterviewAnswers { pub idea: String, pub genre: String /* template id or "other" */,
    pub genre_other: Option<String>, pub feel: Vec<String>, pub look: String,
    pub references: String, pub session_length: String, pub name: String }
#[derive(Serialize, Clone, Debug)]
pub struct TemplateChoice { pub template_id: String, pub reason: String }
#[derive(Serialize, Clone, Debug)]
pub struct OnboardingPreview { pub choice: TemplateChoice, pub project_name: String,
    pub cards: Vec<String> /* context-relative paths that will be written */, pub first_build_steps: Vec<String> }
#[derive(Serialize, Clone, Debug)]
pub struct CreatedProject { pub path: String, pub thread_id: String, pub first_build_message: String }

pub fn choose_template(a: &InterviewAnswers, templates: &[TemplateInfo]) -> TemplateChoice;
pub fn preview(a: &InterviewAnswers) -> Result<OnboardingPreview>;
/// Scaffolds from the chosen template, writes the starter Context over the
/// template's cards, creates the "First build" chat thread, snapshots
/// ("New game: <name>"), and returns the first build message.
pub fn create_from_interview(parent: &Path, a: &InterviewAnswers, template_id: &str, provider: &str) -> Result<CreatedProject>;
```

Each template directory has a `template.json` with the `TemplateInfo` fields.

### Auto-fix policy (`crates/core/src/autofix.rs`)

```rust
pub const MAX_ATTEMPTS: u32 = 2;
pub struct AutoFix { /* per-project state */ }
pub enum AutoFixDecision { Nothing, StartFix { thread_id: String, message: String, attempt: u32 }, GiveUp { thread_id: String } }
impl AutoFix {
    pub fn new(debounce: Duration) -> Self;
    pub fn on_turn_started(&mut self, thread_id: &str, origin: MessageOrigin); // User/PlanApproval/FirstBuild reset attempts
    pub fn on_turn_finished(&mut self, thread_id: &str);
    pub fn on_game_started(&mut self);        // clears buffered errors
    pub fn on_game_error(&mut self, err: &GameError, now: Instant);
    pub fn poll(&mut self, now: Instant, turn_running: bool, enabled: bool) -> AutoFixDecision;
}
```

### Tauri commands (stubs in Wave 0; bodies in Wave 2)

| Command | Args | Returns | Task |
|---|---|---|---|
| `agent_send` (changed) | `projectPath, threadId, message, origin?: MessageOrigin` | `()` | G2 |
| `agent_status` (changed meaning) | — | `RuntimeStatus` of the selected provider | G2 |
| `ai_providers` | — | `ProviderInfo[]` (plus `recommended` via `ai_recommended`) | C2 |
| `ai_recommended` | — | `ProviderId` | C2 |
| `ai_test_connection` | `provider` | `ConnectionTest` | C2 |
| `connect_run` | `provider, action: "install" \| "login", rows, cols` | `()` | C2 |
| `connect_write` | `data` | `()` | C2 |
| `connect_resize` | `rows, cols` | `()` | C2 |
| `connect_cancel` | — | `()` | C2 |
| `app_settings_get` | — | `AppSettings` | C2 |
| `app_settings_set` | `settings` | `AppSettings` | C2 |
| `godot_open_editor` | `projectPath` | `()` | C2 |
| `project_settings_get` | `projectPath` | `ProjectSettings` | O2 |
| `project_settings_set` | `projectPath, settings` | `ProjectSettings` | O2 |
| `onboarding_templates` | — | `TemplateInfo[]` | O2 |
| `onboarding_preview` | `answers` | `OnboardingPreview` | O2 |
| `onboarding_create` | `parentDir, answers, templateId` | `CreatedProject` | O2 |

`ConnectionTest { ok: bool, reply: Option<String>, error_kind: Option<AgentErrorKind>, message: Option<String>, duration_ms: u64 }` — a real one-line turn ("Reply with exactly: hello") in a temp folder.

### Tauri events (new)

| Event | Payload |
|---|---|
| `connect-output` | `{ data: string }` (raw PTY bytes, UTF-8 lossy) |
| `connect-exit` | `{ action: "install" \| "login", provider, success: boolean, code: number \| null }` |
| `autofix-state` | `{ projectPath, threadId, state: "fixing" \| "gave_up" \| "idle", attempt, maxAttempts }` |

### Frontend contract

- `src/lib/studio-types.ts` / `studio-api.ts` hold every type, command wrapper and event helper above.
- `PendingTurn { threadId: string; message: string; origin: MessageOrigin }` (studio-types): a turn the app should start as soon as Studio shows the project (the onboarding first build).
- `DashboardSection` props: `{ onOpenProject: (path: string, pendingTurn?: PendingTurn) => void }`.
- `OnboardingFlow` (`src/components/onboarding/OnboardingFlow.tsx`) props: `{ onCreated: (created: CreatedProject) => void; onCancel: () => void }`.
- `ConnectAiPanel` (`src/components/connect/ConnectAiPanel.tsx`) props: `{ onConnected?: (provider: ProviderId) => void }`.
- `StudioSection` props: `{ projectPath: string | null; pendingTurn: PendingTurn | null; onPendingTurnTaken: () => void }`.
- `ContextSection` / `AdvancedSection` props: `{ projectPath: string | null }`.

## Wave 1 tasks

### CX: Codex CLI runtime
Implement `CodexRuntime` driving `codex exec --json` (and `codex exec resume <id>` for later turns), mapped to `AgentEvent`s by `codex_stream.rs`, with the same guarantees as `ClaudeCodeRuntime` (process group kill on cancel, login-shell PATH via `agent/path.rs`, errors classified into `AgentErrorKind`, `FilesChanged` from real file-change items, `PlanProposed` from the `infinabox` `propose_plan` MCP call, `Cancelled` on stop).
- Verify every flag against `codex exec --help` and the CLI's real output. Restrictions: `--sandbox workspace-write`, `-C <project>`, the InfinaBox MCP server only (configured with `-c mcp_servers.infinabox...`; keep the bridge token out of the command line — pass it through the environment, e.g. the server config's env-forwarding option), the user's own config/MCP servers/hooks and the project's `.codex/` not loaded where the CLI allows (`--ignore-user-config` keeps auth), system prompt from `prompt::system_prompt` via the CLI's instructions override (verify which key works).
- **Fixtures:** record real `codex exec --json` output by pointing the real CLI at a local scripted OpenAI-compatible Responses endpoint (`scripts/fixtures/mock-responses-server.mjs`, configured as a custom `model_providers` entry) so the CLI's own event stream is real: a text-only turn, a file edit, an MCP tool call (to the echo MCP server from Phase A's fixtures), a `propose_plan` call, a resumed turn, a cancelled turn, plus not-logged-in and bad-resume errors against the real provider. README entry for each, like Phase A's.
- Tests: parser over every fixture; args builder; an `#[ignore]` test that runs the real CLI against the mock server.

### PL: Plans, prompts, project settings
- `prompt.rs`: `system_prompt()` composing `prompts/director.md` + a plan section per `PlanPolicy`/`MessageOrigin` + teach section + `AGENTS.md`. Plans: call `propose_plan` with a short title and 2–6 plain steps, then end the turn with one sentence and no changes; after `PlanApproval`, carry it out without re-planning; `AutoFix`/`FirstBuild` never plan. Explanations: end every change with one paragraph "what I did and why"; teach mode adds a short "How it works" lesson naming the real files/scenes.
- `claude.rs` uses `system_prompt`; `detect()` fills `logged_in` from `claude auth status --json`; stop yields `Error { kind: Cancelled, message: "Stopped." }`.
- `claude_stream.rs`: `mcp__infinabox__propose_plan` tool use → `PlanProposed` (validate input; malformed → normal `ToolUse`). Record a real fixture of a plan turn (extend `scripts/record-claude-fixtures.mjs` if needed).
- `crates/mcp-server/src/server.rs`: the `propose_plan` tool (title ≤ 80 chars, 1–8 non-empty steps, each ≤ 200 chars); its result tells the agent the plan is shown with Approve/Change buttons and to end the turn now.
- `project_settings.rs` with tests.

### CN: Connect detection and app settings
- `connect.rs`: detection via the login-shell PATH (`agent/path.rs`) — installed, `--version`, signed in (`claude auth status --json` → `loggedIn`; `codex login status` exit code), with timeouts; per-OS installer/sign-in invocations (see Decisions); `install_blocker` when a prerequisite (npm) is missing; `recommend()`. `path.rs` gains a way to refresh its cached PATH after an install.
- `app_settings.rs`: load/save (atomic, `0600` not needed — nothing secret).
- Tests with real binaries where present (`claude` here) and fake executables in temp dirs for the rest.

### OB: Onboarding and scaffold
- `scaffold.rs`: embed all four templates, `list_templates()` from `template.json`, `create_project_from_template()` (same safety as `create_project`, marker `dimension` from the template), `{{PROJECT_NAME}}` replacement in `.md`, `project.godot` `config/name` set to the project name.
- `onboarding.rs`: `choose_template`, `preview`, `create_from_interview`: writes `concept.md` (pitch = the idea, pillars from the feel, references, session length), `style-guide.md` (the look + feel), `tasks/first-playable.md` (first milestone checklist grounded in the template's features), keeps the template's mechanic cards; the first build message tells the agent to read those cards and customize the template toward the concept (player, core mechanic tuning, colors, in-game title) in one playable level, run it, fix errors, and update the cards.
- Tests over real temp dirs; an `#[ignore]` test per template that scaffolds it and boots it headless with real Godot (`godot::validate`) with zero errors.

### AF: Auto-fix policy and follow-ups
- `autofix.rs` per the contract: debounce (errors settle for ~1.5 s), dedupe, max 5 errors in the message (message + file:line), same error set as the last attempt → `GiveUp`, attempts reset by any non-auto-fix turn, nothing while a turn runs or when disabled or no thread is known. Tests.
- `chat_store.rs`: `origin` on user records; `set_provider`.
- `snapshot.rs`: `stage_everything` retries a file whose size changes while being read (bounded); `create_snapshot` returns `None` (no snapshot) when the tree equals HEAD after staging (fixes "No files changed" AI snapshots).

### T1 / T2 / T3: Templates
Each a clean Godot 4.7 project in `templates/<id>/` like `blank-2d` (same `.gitignore`, `project.godot` without the addon — scaffold adds it), with:
- `template.json` (`TemplateInfo` fields).
- Gameplay that works in the first minute, all placeholder art made from Godot primitives (`Polygon2D`, `ColorRect`, shapes) or tiny PNGs we generate — no third-party assets. Every script well commented in plain language; tuning values as `@export` vars at the top; input actions defined in `project.godot` (keyboard + gamepad).
  - **T1 platformer-2d:** run, variable-height jump with coyote time, a level with platforms, coins with a counter, a hazard that respawns the player at a checkpoint, a goal flag with a "Level complete" screen and restart.
  - **T2 topdown-2d:** 8-way movement, walls/rooms, a sign/NPC to talk to (simple dialogue box), a key that opens a door, one wandering enemy that knocks back, hearts HUD, win when reaching the exit.
  - **T3 shooter-2d:** twin-stick (WASD + mouse aim/click, or gamepad sticks), enemies that chase in waves, score, health, game over screen with restart.
- `AGENTS.md` + `CLAUDE.md` describing the layout, the scenes/scripts and where each tuning value lives (same style as blank-2d's).
- `.ibproject/context/` starter cards: one card per core mechanic (front-matter `type`, `title`, `status`, `implemented_in`), plus `concept.md` with `{{PROJECT_NAME}}` placeholders.
- Verified: boots headless with no errors/warnings in the output (`$GODOT --headless --path . --quit-after 300`), imports cleanly, and a short screenshot run shows the scene rendering (`--write-movie` or the addon's screenshot) — include the screenshot in the report.

### FN: Navigation, Context, Advanced
- Sidebar: Home, Studio, Context, Assets, Playtest & Launch, Advanced (lucide icons, same animations). `Section` type becomes `"home" | "studio" | "context" | "assets" | "launch" | "advanced"`.
- `App.tsx`: persistent sections Studio, Context, Advanced; Assets/Launch via `simpleSections` with honest `NotBuiltYetSection` copy ("Arrives in Phase C/D" in user words, no phase jargon); holds `pendingTurn` state from Dashboard → Studio; retire the old sections' files and their imports.
- `ContextSection`: `FileBrowser` over `.ibproject/context` (Markdown cards, `forceMdExtension`) with a Graphs tab over `.ibproject/graphs` (the existing `GraphsSection` behavior).
- `AdvancedSection`: tabs **Code & terminal** (the existing `BuildRow`), **Settings** (Godot: managed/custom path via `app_settings_*`, version from `godot_status`; "Open in Godot editor" via `godot_open_editor`; AI: current provider with a link back to Home's connect panel).

### FH: Home and connect UI
- `ConnectAiPanel` (`src/components/connect/`): cards for Claude Code and Codex from `ai_providers` (installed/version/signed-in, blurb, "Recommended" badge from `ai_recommended`), actions: Install (shows the exact command first, then runs it in an embedded xterm fed by `connect-output`/`connect_write`), Sign in (same terminal), Test ("say hello" via `ai_test_connection`, shows the real reply or the real error), Use this one (`app_settings_set`). Re-detects after each install/login exit.
- `DashboardSection`: first-run checklist when `first_run_done` is false (1 Connect your AI, 2 Set up Godot — `godot_install` with progress, 3 Make your first game → `OnboardingFlow`), otherwise projects + compact AI/Godot status + "New game" (→ `OnboardingFlow`) + "Open project". On `onCreated`, calls `onOpenProject(path, { threadId, message: first_build_message, origin: "first_build" })` and marks `first_run_done`.

### FO: Onboarding UI
`OnboardingFlow`: one question per screen, friendly copy, chips + free text, back/next, progress dots: idea, kind of game (templates from `onboarding_templates` + "Something else"), feel (multi), look, games it's like (optional), session length, name + folder (native folder picker as in Phase A's New Project). A note that games start in 2D for now. Then a review screen from `onboarding_preview`: the template and why, the cards that will be written, "What I'll build first" steps, and **Create my game** → `onboarding_create` → `onCreated`. Errors shown plainly with retry.

### FC: Studio chat UI
- `plan_proposed` → a plan card (title, numbered steps, **Approve** → `agentSend(..., "Approved — go ahead with the plan.", "plan_approval")`, **Change something** → focuses the composer with a hint). Only the latest unanswered plan is actionable.
- `cancelled` errors → neutral "Stopped" note; user records by origin: `plan_approval` → "Approved the plan" chip, `auto_fix` → "Something broke — fixing it" note with the errors expandable, `first_build` → "Building your game" note.
- `autofix-state` → banner in the chat ("Something broke — fixing it (attempt 1 of 2)", "I couldn't fix this automatically — ask me about it or undo the last change").
- Studio settings popover (chat header): plan policy, Teach me, Fix errors automatically (`project_settings_*`).
- `pendingTurn`: when Studio shows the matching project, select that thread, send it once, and call `onPendingTurnTaken`.
- The final assistant text of an editing turn renders as a "What changed" card.

## Wave 2 tasks

- **G2** (`agent.rs`, `autofix.rs`): runtime chosen from `AppSettings.ai_provider` (default Claude Code), threads created with that provider, a thread from another provider is switched with `chat_store::set_provider` (and a note event); `TurnOptions` from `project_settings::load` + `origin`; user records carry `origin`; stop → `Cancelled`; `autofix.rs` holds per-project `AutoFix` state, is fed by `agent.rs` turn start/finish and by `godot.rs` game start/errors (hooks pre-placed in Wave 0), polls on a timer thread, starts fix turns through the same path as `agent_send`, emits `autofix-state`.
- **C2** (`connect.rs`, `settings.rs`, `godot.rs`): the connect PTY (reusing `TerminalSession`), `connect-*` events, detection/test commands (test in a temp dir with the real runtime), app settings commands, Godot custom path (locate honors `AppSettings.godot_path` before `INFINABOX_GODOT`), `godot_open_editor` (`godot -e --path <project>`, detached).
- **O2** (`onboarding.rs`, `project_settings.rs`, `scaffold.rs` commands): thin async wrappers; `onboarding_create` uses the selected provider name for the thread.

## Wave 3: integration

- Merge, full verification after each merge.
- E2E (`e2e/`): new scenario **first-run**: fresh app data → Home checklist → (Godot install already cached) → onboarding interview → Platformer → Create → Studio runs the first build with the real AI (`--real-ai`) → game plays → a plan turn ("add a double jump") shows a plan card → Approve → built → an injected script error triggers auto-fix. Non-AI runs cover the interview, template scaffolds, navigation, settings.
- Docs: `CLAUDE.md`, this plan's status, spec cross-references.

**Exit (engineering):** the first-run scenario passes end to end with a real AI; the product exit (5 of 8 novice testers under 30 minutes) needs real people and is recorded as not yet run.
