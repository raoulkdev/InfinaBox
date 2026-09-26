# Phase A: Foundations Implementation Plan

> **For agentic workers:** this plan is built to be executed by **multiple subagents working in parallel**, coordinated by one lead agent. Read "Execution model: parallel subagents" before starting any task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** From a new Studio chat, a user can ask for a change to their Godot game, watch the AI build it, see the game relaunch with the change, and undo it.

**Spec:** `docs/superpowers/specs/2026-09-25-ai-game-studio-product-spec.md` (§6.1 core loop, §7.1 Studio, §7.5 History, §8.3–8.4 agent runtime and tools, §9 project format, §10 Godot integration, §14 Phase A).

**Exit criterion (from spec §14, demonstrated, not asserted):**
1. Create a new project from Home. It opens in Studio.
2. Install Godot from Studio with one click (skipped if already installed).
3. Press Play. The starter game runs in its own window.
4. Type "make the background dark blue and add a label that says Hello". The AI builds it, a snapshot is created, and the game relaunches with the change.
5. Press "Undo last change". The project returns to its previous state and the game relaunches without the change.
6. Introduce an error (ask the AI for something that breaks, or edit a script by hand). The error shows in the Play panel, and "Ask AI to fix" sends it to the chat.

---

## Scope

**In Phase A:**
- Claude Code CLI agent runtime (headless, streaming), behind the `AgentRuntime` interface.
- InfinaBox MCP server exposing Context, Godot, and History tools to the agent.
- A local "bridge" so the MCP server can ask the running app to run/stop the game and read errors.
- Managed Godot install (download, verify, locate), run/stop with output capture, error parsing.
- Snapshots over git: automatic after every AI turn that changed files, undo, go back to any snapshot.
- Chat threads persisted to `.ibproject/chat/*.jsonl`, committed, with secrets redacted before writing.
- A minimal blank 2D project scaffold (with the auto-installed InfinaBox Godot addon) so the loop has a real game to work on.
- A new **Studio** section (chat + Play panel + History panel) and making it the landing section for an open project.

**Not in Phase A** (later phases, per spec §14): Codex/API-key/local runtimes, the guided connect-your-AI flow, plan/approve UI, onboarding interview, genre templates, 3D, the 6-entry navigation overhaul, typed Context cards UI, assets, playtest, launch, accounts and Stripe, embedding the game window, the automatic error-fix loop (Phase A has the manual "Ask AI to fix" button only).

## Global constraints

Every subagent must follow these. Include this section verbatim in every subagent prompt.

- **Real, not fabricated.** No mocked data in any production code path. Status, versions, errors, and usage figures shown in the UI come from real processes and real files. If something isn't known (e.g. whether the user is logged in to Claude), say so rather than guessing.
- **Match the codebase.** Follow `CLAUDE.md` patterns: the three-step Tauri command registration, the `terminal.rs`/`watcher.rs` split (plain testable logic with callbacks, thin untested `#[tauri::command]` wrappers), `motion` conventions from `src/lib/motion.ts`, `ResizablePanelGroup` for splits, shadcn/ui components, comment density and style of the surrounding files.
- **Testable logic lives in `crates/core`** (or `crates/mcp-server`), not in `src-tauri`. Tauri commands stay thin.
- **Tests use real things.** Git tests use real temp repos via `git2`. Parser tests use output recorded from the real tools (Task 0.2 fixtures), not hand-written approximations. Tests that need a real Godot binary or a logged-in `claude` CLI are marked `#[ignore = "needs <tool>; run with --ignored"]` and must pass when run with `--ignored` on a machine that has the tool.
- **No frontend test runner exists.** Frontend tasks are verified with `npx tsc --noEmit` and `npm run build` (zero errors), plus checking every `invoke()`/`listen()` call against the Rust signatures in the Wave 0 contracts.
- **External tool details must be verified, not assumed.** Claude Code CLI flags and stream formats, Godot command-line flags and log formats, and the `rmcp` crate API all change between versions. Every task that depends on one starts by checking the real tool (`claude --help`, `godot --help`, the crate docs) and the Task 0.2 fixtures. Flags written in this plan are the expected ones, not a guarantee.
- **Never destroy user work.** No `git reset --hard`, no force checkout without first snapshotting uncommitted changes. Going back is always a new commit.
- **Tauri commands that do real work must not block the main thread.** In Tauri v2 a plain `#[tauri::command] pub fn` runs inline on the main thread and freezes the window. Use `#[tauri::command(async)]` (as in `commands/overview.rs`) or an `async fn` for anything touching git, the filesystem beyond a trivial read, or child processes.
- **Stay in your lane.** A subagent edits only the files its task owns (see the ownership table). If it needs a contract change, it stops and reports to the lead instead of editing shared files.
- Verification commands (from repo root): `cargo test --workspace --no-default-features`, `cd src-tauri && cargo check --no-default-features`, `npx tsc --noEmit`, `npm run build`. The known fixture failure `commands::fs::tests::reads_a_real_gdd_doc_from_the_fixture` is expected and not a regression.

---

## Execution model: parallel subagents

Phase A is built by **subagents developing multiple tasks at the same time**. One **lead agent** (the session executing this plan) coordinates; it does not implement Wave 1 or Wave 2 tasks itself.

### Roles
- **Lead agent**
  - Implements Wave 0 (contracts, stubs, fixtures) alone, because every other task depends on it.
  - Dispatches each Wave 1 and Wave 2 task to its own subagent, all tasks in a wave at once.
  - Reviews, merges, and verifies each subagent's work.
  - Runs Wave 3 (integration and end-to-end check).
- **Implementer subagents:** one per task. Each works in its **own git worktree** (Agent tool with `isolation: "worktree"`, run in the background) so parallel work never collides on disk.
- **Reviewer subagents:** after an implementer reports done, the lead dispatches a fresh reviewer subagent to check the diff against the task's acceptance criteria and the global constraints, before merging.

### How the lead dispatches a task
Each implementer subagent's prompt contains:
1. The full task section from this plan.
2. The "Global constraints" section.
3. The "Wave 0 contracts" section (the frozen interfaces).
4. Its file ownership list, and the instruction: "Edit only these files. If you need to change anything outside them, stop and report what and why."
5. The instruction to commit its work on its worktree branch with a clear message, run its task's verification commands, and report: files changed, test results (with output), anything it could not verify, and any assumptions about external tools that the fixtures didn't settle.

### Waves and dependencies

```
Wave 0  (lead, sequential)
  0.1 Contracts + stubs ──┐
  0.2 Real tool fixtures ─┤
                          ▼
Wave 1  (7 subagents in parallel, each in its own worktree)
  A  Agent runtime (Claude Code CLI)          crates/core/src/agent/
  B  MCP server                               crates/mcp-server/
  C  Godot manager                            crates/core/src/godot/
  D  Snapshots, chat store, redaction         crates/core/src/{snapshot,chat_store,redact}.rs
  E  Project scaffold + Godot addon           crates/core/src/scaffold.rs, templates/, godot-addon/
  F1 Studio chat UI                           src/components/studio/chat/
  F2 Studio Play + History UI                 src/components/studio/{play,history}/
                          ▼
Wave 2  (3 subagents in parallel, each in its own worktree)
  G  Agent + chat Tauri commands              src-tauri/src/commands/agent.rs
  H  Godot commands + bridge server           src-tauri/src/commands/{godot,bridge}.rs
  I  Snapshot/scaffold commands + app wiring  src-tauri/src/commands/{snapshot,scaffold}.rs, App.tsx, Sidebar.tsx, DashboardSection.tsx, StudioSection.tsx
                          ▼
Wave 3  (lead, plus fix-up subagents in parallel as needed)
  3.1 Integrate and verify everything
  3.2 End-to-end exit-criterion run
  3.3 Docs update
```

Wave 1 tasks depend only on Wave 0. Wave 2 tasks depend on the Wave 1 tasks they wrap (G on A, B, D; H on B, C; I on D, E, F1, F2), so Wave 2 starts once all of Wave 1 is merged.

### Why the tasks can run in parallel safely
- **Frozen contracts.** Wave 0 writes every shared type, event name, command signature, and bridge message before any subagent starts. Subagents implement against them; nobody edits them in Wave 1 or 2 without the lead.
- **Pre-registered stubs remove the shared hotspots.** The files every feature would otherwise touch (`src-tauri/src/lib.rs`, `commands/mod.rs`, `crates/core/src/lib.rs`, `Cargo.toml` members, `src-tauri/src/main.rs`) are fully edited in Wave 0: every new module exists, every new command is registered, and every command returns a clear "not implemented yet" error. Wave 1 and 2 subagents only fill in bodies in files they own.
- **Disjoint file ownership.** No two tasks in the same wave own the same file (see table).
- **Dependencies are added in Wave 0.** New crates (`rmcp`, `reqwest`, `zip`, `sha2`, `regex`, `tokio` for the MCP crate, etc.) go into the `Cargo.toml` files in Wave 0, so parallel subagents don't produce conflicting `Cargo.toml`/`Cargo.lock` edits. If a subagent genuinely needs another dependency, it reports to the lead, who adds it on the main branch and tells the other subagents to merge it.

### File ownership

| Task | Owns (may create/edit) |
|---|---|
| 0.1, 0.2 (lead) | All contract/stub files listed in Task 0.1; `crates/core/tests/fixtures/**` |
| A | `crates/core/src/agent/**` (except `agent/types.rs`, which is a contract) |
| B | `crates/mcp-server/src/**` (except `bridge_protocol.rs`, a contract) |
| C | `crates/core/src/godot/**` (except `godot/types.rs`) |
| D | `crates/core/src/snapshot.rs`, `crates/core/src/chat_store.rs`, `crates/core/src/redact.rs` |
| E | `crates/core/src/scaffold.rs`, `templates/blank-2d/**`, `godot-addon/infinabox/**` |
| F1 | `src/components/studio/chat/**` |
| F2 | `src/components/studio/play/**`, `src/components/studio/history/**` |
| G | `src-tauri/src/commands/agent.rs` |
| H | `src-tauri/src/commands/godot.rs`, `src-tauri/src/commands/bridge.rs` |
| I | `src-tauri/src/commands/snapshot.rs`, `src-tauri/src/commands/scaffold.rs`, `src/App.tsx`, `src/components/cockpit/Sidebar.tsx`, `src/components/cockpit/DashboardSection.tsx`, `src/components/studio/StudioSection.tsx` |

### Merging and review
- The lead merges Wave 1 branches in this order: D, C, E, A, B, F2, F1. Backend first, so frontend merges land on real signatures. After **each** merge it runs the full verification commands. A merge that breaks verification is reverted and sent back to its subagent.
- Every task gets a reviewer subagent before merge. The reviewer checks:
  - the acceptance criteria
  - the global constraints (especially no fabricated data and staying within file ownership)
  - test quality (real repos/fixtures, no tests weakened to pass)
- Findings go back to the **same** implementer subagent (continue it with SendMessage so it keeps its context). The lead never quietly rewrites a subagent's work.
- Wave 2 follows the same process, merge order H, G, I.
- Maximum concurrency: 7 subagents in Wave 1, 3 in Wave 2, plus reviewers.

---

## Wave 0 contracts

These are written by the lead in Task 0.1 and frozen for Waves 1–2. Names and shapes below are the contract; doc comments and exact module layout can follow the codebase's style.

### Agent events (`crates/core/src/agent/types.rs`, mirrored in `src/lib/studio-types.ts`)

```rust
/// One thing that happened during an agent turn, normalized from whatever
/// the underlying runtime (Phase A: Claude Code CLI stream-json) emits.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    SessionStarted { provider_session_id: String, model: Option<String> },
    AssistantText { text: String },
    ToolUse { id: String, name: String, summary: String },
    ToolResult { id: String, ok: bool, summary: String },
    /// Paths (project-relative) the agent created or edited this turn,
    /// derived from real file-editing tool uses.
    FilesChanged { paths: Vec<String> },
    TurnCompleted { is_error: bool, duration_ms: Option<u64>, usage: Option<Usage> },
    Error { kind: AgentErrorKind, message: String },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Usage { pub input_tokens: Option<u64>, pub output_tokens: Option<u64> }

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AgentErrorKind { NotInstalled, NotAuthenticated, RateLimited, ProcessFailed, Other }

/// The provider-independent interface from spec §8.3. Phase A has one
/// implementation (Claude Code CLI); Codex/API-key/local come later.
pub trait AgentRuntime: Send + Sync {
    fn detect(&self) -> RuntimeStatus;
    /// Runs one user turn. Blocks the calling thread until the turn ends;
    /// every event is delivered through `on_event` as it arrives.
    fn run_turn(&self, req: TurnRequest, on_event: &mut dyn FnMut(AgentEvent)) -> anyhow::Result<()>;
    /// Stops the in-flight turn for `thread_id`, if any.
    fn cancel(&self, thread_id: &str);
}

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
pub struct McpLaunch { pub command: PathBuf, pub args: Vec<String>, pub env: Vec<(String, String)> }

#[derive(Serialize, Clone, Debug)]
pub struct RuntimeStatus { pub name: String, pub installed: bool, pub version: Option<String> }
```

### Godot types (`crates/core/src/godot/types.rs`, mirrored in TS)

```rust
#[derive(Serialize, Clone, Debug)]
pub struct GodotStatus { pub installed: bool, pub version: Option<String>, pub path: Option<PathBuf>, pub managed: bool }

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum GameState { Stopped, Starting, Running, Crashed }

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GameOutputLine { pub stream: OutputStream, pub text: String }

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum OutputStream { Stdout, Stderr }

/// One error parsed from real Godot output (script error, parse error,
/// engine error), with its location when Godot printed one.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GameError { pub message: String, pub file: Option<String>, pub line: Option<u32>, pub raw: String }

#[derive(Serialize, Clone, Debug)]
pub struct InstallProgress { pub downloaded_bytes: u64, pub total_bytes: Option<u64>, pub phase: String }
```

### Snapshots and chat (`crates/core/src/snapshot.rs`, `chat_store.rs`: signatures as stubs)

```rust
#[derive(Serialize, Clone, Debug)]
pub struct Snapshot { pub id: String /* commit sha */, pub title: String, pub timestamp: i64, pub thread_id: Option<String>, pub turn: Option<u32>, pub files_changed: usize }

pub fn create_snapshot(project: &Path, title: &str, origin: Option<(&str, u32)>) -> Result<Option<Snapshot>>; // None = nothing changed
pub fn list_snapshots(project: &Path, limit: usize) -> Result<Vec<Snapshot>>;
pub fn restore_to(project: &Path, snapshot_id: &str) -> Result<Snapshot>;   // new commit; returns it
pub fn undo_last(project: &Path) -> Result<Option<Snapshot>>;              // restore_to(parent of latest snapshot)

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ThreadSummary { pub id: String, pub title: String, pub created_at: i64, pub provider: String, pub provider_session_id: Option<String> }

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChatRecord { User { text: String, at: i64 }, Event { event: AgentEvent, at: i64 } }

pub fn create_thread(project: &Path, title: &str, provider: &str) -> Result<ThreadSummary>;
pub fn append(project: &Path, thread_id: &str, record: &ChatRecord) -> Result<()>;   // redacts before writing
pub fn set_provider_session(project: &Path, thread_id: &str, session_id: &str) -> Result<()>;
pub fn list_threads(project: &Path) -> Result<Vec<ThreadSummary>>;
pub fn load_thread(project: &Path, thread_id: &str) -> Result<(ThreadSummary, Vec<ChatRecord>)>;

pub fn redact(text: &str) -> String; // in redact.rs
```

### Scaffold (`crates/core/src/scaffold.rs`)

```rust
pub fn create_project(parent_dir: &Path, name: &str) -> Result<PathBuf>; // Phase A: blank 2D only
pub fn ensure_addon(project: &Path) -> Result<bool>; // installs/updates addon; true if it changed anything
```

### Bridge protocol (`crates/mcp-server/src/bridge_protocol.rs`)
The MCP server process (launched by the agent CLI) talks to the running InfinaBox app over **loopback TCP**, newline-delimited JSON.
- The app listens on `127.0.0.1:<random port>` and generates a random 32-byte token per app launch.
- It passes both to the MCP server process via the env vars `INFINABOX_BRIDGE_ADDR`, `INFINABOX_BRIDGE_TOKEN`, and `INFINABOX_PROJECT`.
- The first message on every connection must be `{"hello": "<token>", "project": "<project path>"}` (the `Hello` struct). Otherwise the app closes the connection. The project path tells the app which game to run.

```rust
#[derive(Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum BridgeRequest { RunGame, StopGame, GameStatus, RecentErrors { limit: usize }, RecentOutput { lines: usize } }

#[derive(Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum BridgeResponse { Ok { data: serde_json::Value }, Error { message: String } }
```

### MCP server launch
No separate sidecar binary. The app executable itself runs the MCP server when started as `<app exe> --mcp-server`:
- `src-tauri/src/main.rs` checks for that argument before building Tauri and calls `infinabox_mcp_server::run_stdio()`.
- `McpLaunch.command` is `std::env::current_exe()`.
- This avoids packaging a second binary per platform. `crates/cli` gains a `mcp-server` subcommand calling the same function, for development and testing.

### Tauri commands (all registered as stubs in Wave 0)

| Command | Args | Returns | Owner |
|---|---|---|---|
| `agent_status` | | `RuntimeStatus` | G |
| `agent_send` | `projectPath, threadId, message` | `()` (events stream via `agent-event`) | G |
| `agent_cancel` | `threadId` | `()` | G |
| `chat_create_thread` | `projectPath, title` | `ThreadSummary` | G |
| `chat_list_threads` | `projectPath` | `ThreadSummary[]` | G |
| `chat_load_thread` | `projectPath, threadId` | `{ thread: ThreadSummary, records: ChatRecord[] }` | G |
| `godot_status` | | `GodotStatus` | H |
| `godot_install` | | `GodotStatus` (progress via `godot-install-progress`) | H |
| `game_run` | `projectPath` | `()` | H |
| `game_stop` | | `()` | H |
| `game_status` | | `GameState` (added after Wave 1: lets a panel start from the real state) | H |
| `game_recent_errors` | `limit` | `GameError[]` | H |
| `snapshot_list` | `projectPath, limit` | `Snapshot[]` | I |
| `snapshot_create` | `projectPath, title` | `Snapshot \| null` | I |
| `snapshot_restore` | `projectPath, snapshotId` | `Snapshot` | I |
| `snapshot_undo_last` | `projectPath` | `Snapshot \| null` | I |
| `project_create` | `parentDir, name` | `string` (project path) | I |

### Tauri events

| Event | Payload |
|---|---|
| `agent-event` | `{ threadId: string, event: AgentEvent }` |
| `agent-turn-finished` | `{ threadId: string, snapshot: Snapshot \| null }` (emitted after the post-turn snapshot) |
| `godot-install-progress` | `InstallProgress` |
| `game-state` | `{ state: GameState }` |
| `game-output` | `GameOutputLine` |
| `game-error` | `GameError` |
| `snapshots-changed` | none (payload-less, like `project-fs-changed`) |

### Frontend contract files
- `src/lib/studio-types.ts`: TS mirrors of every type above.
- `src/lib/studio-api.ts`: one typed wrapper per command (`agentSend(...)`, `gameRun(...)`, ...) and one typed `on…` subscription helper per event, following `src/lib/fs-watch.ts`'s style. Frontend subagents call only these wrappers, never raw `invoke`/`listen`.

---

## Wave 0 (lead only)

**Status (2026-09-25):** Task 0.1 done. Task 0.2: Godot fixtures recorded (4.7.2); Claude Code fixtures recorded 2026-09-26 (CLI 2.1.283) with a clean, default configuration in the cloud container; see `crates/core/tests/fixtures/README.md` for exactly how, and re-record on a local install to confirm.

### Task 0.1: Contracts, stubs, and dependencies

**Files:** create every contract file above. Create stub modules for every Wave 1/2 file in the ownership table: each stub function returns `anyhow::bail!("not implemented yet: <name>")`, and each stub component renders nothing. Edit `Cargo.toml` (add `crates/mcp-server` to members), `crates/core/src/lib.rs`, `crates/core/Cargo.toml`, `src-tauri/Cargo.toml`, `src-tauri/src/lib.rs`, `src-tauri/src/main.rs`, `src-tauri/src/commands/mod.rs`, `crates/cli/src/main.rs`.

- [ ] Create `crates/mcp-server` (`infinabox-mcp-server`, lib crate) with `run_stdio()` stub and `bridge_protocol.rs`.
- [ ] Add dependencies up front:
  - `crates/core`: `reqwest` (blocking, rustls), `zip`, `sha2`, `regex`, `serde_yaml` or equivalent for front-matter, `tempfile` (dev).
  - `crates/mcp-server`: `rmcp` (the official Rust MCP SDK, stdio server feature), `tokio`, `serde_json`.
  - `src-tauri`: `rand` for the bridge token, `infinabox-mcp-server`.
  - Pin to current releases and check each builds.
- [ ] Register every command from the contract table in `lib.rs`'s `generate_handler!` and `commands/mod.rs`. Add managed state types (`AgentState`, `GodotState`, `BridgeState`) and a `.setup()` hook that calls `bridge::start(app)` (stubbed).
- [ ] `main.rs`: dispatch `--mcp-server` to `infinabox_mcp_server::run_stdio()` before starting Tauri.
- [ ] Write `src/lib/studio-types.ts` and `src/lib/studio-api.ts`.
- [ ] Verify: all four verification commands pass (only the known fixture failure). Commit: "Phase A: contracts and stubs".

### Task 0.2: Record real tool output as fixtures

The runtime and Godot parsers must be built against what the real tools actually print. Recorded once here so every subagent shares the same ground truth.

- [ ] **Claude Code CLI:**
  - In a throwaway temp project, run the installed `claude` in headless streaming mode. Expected form: `claude -p "<prompt>" --output-format stream-json --verbose`; confirm the exact flags with `claude --help` first.
  - Record these scenarios: (a) a plain text answer, (b) a turn that edits a file, (c) a resumed second turn (`--resume <session id>`), (d) a turn that calls a tool from a test MCP server passed with `--mcp-config`, (e) an error case (e.g. an invalid `--resume` id).
  - Save the raw lines to `crates/core/tests/fixtures/claude/<scenario>.jsonl`.
  - Also record `claude --version` output and, for (d), the exact tool-name format the CLI uses for MCP tools.
- [ ] **Godot:** download the pinned Godot version by hand. Record stdout/stderr of:
  - (a) a clean project boot
  - (b) a GDScript parse error
  - (c) a runtime script error (null access)
  - (d) a missing `res://` resource
  - (e) `--version` output

  Save to `crates/core/tests/fixtures/godot/<scenario>.txt`. Record the command-line flags actually used.
- [ ] **Scrub** every fixture: replace the home directory path and username with placeholders. There must be no tokens or secrets (check with `redact`-style patterns by eye).
- [ ] Write `crates/core/tests/fixtures/README.md`: tool versions, exact commands, date recorded. Commit: "Phase A: real tool output fixtures".

Pin the Godot version in a constant during this task: the latest stable 4.x release at the time, with its official SHA-512 checksums for the macOS universal, Windows x86_64 and Linux x86_64 downloads.

---

## Wave 1 (7 subagents in parallel)

**Status (2026-09-25):** B, C, D, E, F1, F2 built in parallel, reviewed (D had two review rounds for data safety), and merged. Task A waits for the Claude Code fixtures (see Wave 0 status). Decisions made during review:
- Undo and "go back" never rewind the chat (`.ibproject/chat/`) or remove `.ibproject/.ibx`; everything else, including `.ibproject/context/`, is restored. `files_changed` excludes chat files, and `undo_last` skips chat-only snapshots.
- A project inside another git repository is refused with a plain message rather than getting a nested repository.
- The MCP server end-to-end test lives in `crates/cli/tests/mcp_server_e2e.rs`.

### Task A: Agent runtime for Claude Code CLI

**Owns:** `crates/core/src/agent/**` except `types.rs`.
**Uses:** fixtures in `crates/core/tests/fixtures/claude/`.

- [ ] **Stream parser** (`agent/claude_stream.rs`): a pure function turning one stream-json line into zero or more `AgentEvent`s.
  - The init/system message becomes `SessionStarted`.
  - Assistant text blocks become `AssistantText`.
  - `tool_use` blocks become `ToolUse`, with a short human summary such as "Editing scripts/player.gd". Never dump the raw input.
  - Tool results become `ToolResult`.
  - The final result message becomes `TurnCompleted`, using only usage figures the stream actually reports.
  - Unknown message types are ignored, not errors.
- [ ] **Files changed:** collect paths from file-writing tool uses (the edit/write tools as named in the fixtures). Make them project-relative. Emit one `FilesChanged` before `TurnCompleted`.
- [ ] **Error classification:**
  - Missing binary gives `NotInstalled`.
  - Auth failures become `NotAuthenticated` and rate-limit messages become `RateLimited`, matched from the real messages in the fixtures. Where a fixture for one of these isn't available, match conservatively and fall back to `Other` with the real message text.
  - A non-zero exit with no result gives `ProcessFailed`.
- [ ] **`ClaudeCodeRuntime`** implementing `AgentRuntime`:
  - `detect()` does a PATH lookup (reuse the approach in `src-tauri/src/commands/environment.rs`: move `is_on_path` into core as `agent::path::is_on_path` and keep the src-tauri one untouched) and runs `claude --version`.
  - `run_turn()` spawns the CLI with `std::process::Command` (not a PTY):
    - cwd = the project
    - headless streaming flags; `--resume` when `resume_provider_session_id` is set
    - `--mcp-config` pointing to a temp JSON file built from `McpLaunch`, with the server named `infinabox`
    - the Director system prompt appended (see below)
    - limit the built-in tools with `--tools` (read/edit/write/search only; `--allowedTools` alone does **not** remove other tools, see `crates/core/tests/fixtures/README.md`), allow `mcp__infinabox__*`, and pass `--strict-mcp-config`
    - no shell tool in Phase A
  - Stdout is read line by line on the calling thread; stderr is captured for error classification.
  - Use a login-shell-equivalent PATH, the same concern `terminal.rs` solves with `-l`: resolve the user's login-shell PATH once (run `$SHELL -l -c 'echo $PATH'`) and cache it. GUI-launched apps on macOS don't inherit the shell PATH.
- [ ] **`cancel()`:** kills the child process for that thread (keep a `thread_id -> Child` map behind a mutex).
- [ ] **Director prompt** (`agent/prompts/director.md`, included with `include_str!`). Phase A version: speak plainly to a non-programmer; after changing the game, call `infinabox` `run_game` and check `get_game_errors`; keep Context cards in `.ibproject/context/` up to date when adding or changing mechanics; explain what you did and why in 2–4 sentences at the end.
- [ ] **Tests:**
  - Parser tests over every fixture file, asserting the exact event sequences.
  - A files-changed test.
  - An error-classification test per fixture error.
  - One `#[ignore]` test that runs a real turn in a temp dir and asserts `SessionStarted` … `TurnCompleted`.

**Acceptance:** `cargo test -p infinabox-core agent` passes; the ignored test passes on a machine with a logged-in `claude`.

### Task B: InfinaBox MCP server

**Owns:** `crates/mcp-server/src/**` except `bridge_protocol.rs`.

- [ ] Implement `run_stdio()` with `rmcp` as a stdio MCP server named `infinabox`. Check the crate's current server/tool macros and follow its docs.
- [ ] **Context tools** (direct filesystem, rooted at `INFINABOX_PROJECT/.ibproject/context/`):
  - `list_context_cards`, `read_context_card(path)`, `search_context(query)` (plain case-insensitive text search)
  - `write_context_card(path, markdown)`
  - Refuse any path that resolves outside the context folder (canonicalize and check the prefix). Test this.
- [ ] **Game tools** via the bridge: `run_game`, `stop_game`, `get_game_status`, `get_game_errors(limit)`, `get_game_output(lines)`.
  - Bridge client: connect, send `hello`, send one request, read one response.
  - If the bridge env vars are missing or the connection fails, the tool returns a clear error ("InfinaBox app is not running"). Never pretend it succeeded.
- [ ] **History tool:** `list_snapshots(limit)`, calling `infinabox_core::snapshot::list_snapshots` directly. Read-only: the app creates snapshots, the agent doesn't.
- [ ] Tool descriptions written for the model: short, plain, say when to use each tool.
- [ ] Wire `crates/cli`'s `mcp-server` subcommand.
- [ ] **Tests:**
  - Path-escape refusal.
  - Context tools against a temp project.
  - Bridge client against an in-test TCP listener that implements the protocol, including a bad-token case.
  - An end-to-end test that spawns the server binary (via the cli crate), performs the MCP `initialize` handshake and `tools/list`, and checks the tool names.

**Acceptance:** `cargo test -p infinabox-mcp-server` passes; `claude` can list the tools when given an `--mcp-config` pointing at `infinabox-cli mcp-server` (verify by hand, report the output).

### Task C: Godot manager

**Owns:** `crates/core/src/godot/**` except `types.rs`.
**Uses:** fixtures in `crates/core/tests/fixtures/godot/`.

- [ ] **`install.rs`:**
  - Resolve the platform download URL for the pinned version (official GitHub release assets).
  - Download with progress callbacks (`InstallProgress`) into `<app data>/godot/<version>/`. The app data dir is passed in; core never calls Tauri.
  - Verify SHA-512 against the pinned checksum, unzip, and set the executable bit on Unix. On macOS the binary is `Godot.app/Contents/MacOS/Godot`.
  - Atomic: download to a temp name and rename only after verification.
  - A checksum mismatch deletes the download and errors clearly.
- [ ] **`locate.rs`:** `status(app_data)` returns a `GodotStatus`. It prefers the managed install; otherwise it checks a user-configured path (Phase A: an `INFINABOX_GODOT` env var only, settings UI later); otherwise `installed: false`. The version is read from the real `--version` output.
- [ ] **`run.rs`:** `GameProcess::start(godot, project, window_hint, on_line)`.
  - Spawns `godot --path <project>` plus position/size flags when a hint is given (confirm the flag names against the fixtures README / `--help`).
  - Streams stdout and stderr lines through the callback from reader threads.
  - Exposes `stop()` (kill + wait) and `is_running()`.
  - On a non-zero exit it reports `Crashed`, otherwise `Stopped`.
- [ ] **`errors.rs`:**
  - An incremental parser fed line by line that emits `GameError`s, handling Godot's multi-line error blocks (message line followed by an "at:" location line), matched against the real fixtures.
  - Extracts `res://` file and line when present.
  - Keeps a bounded ring buffer of recent errors and output lines for the bridge's `RecentErrors`/`RecentOutput`.
- [ ] **`validate.rs`:** a headless boot check that runs the project headless for a few frames and quits (flags confirmed from `--help`), returning the parsed errors. The agent-facing validate tool can come later; Phase A uses this for the scaffold test in Task E and in Wave 3.
- [ ] **Tests:**
  - Error parser against every fixture: exact errors, files and lines.
  - Ring buffer bounds.
  - Checksum-mismatch handling, using a local file served from an in-test HTTP listener or a `file`-based download seam.
  - `#[ignore]` tests: real download and verify of the pinned version; run and stop a real project.

**Acceptance:** `cargo test -p infinabox-core godot` passes; the ignored tests pass with network access and a real Godot.

### Task D: Snapshots, chat store, redaction

**Owns:** `crates/core/src/snapshot.rs`, `chat_store.rs`, `redact.rs`.

- [ ] **`snapshot.rs`** using `git2`, following the style in `git_indexer.rs`:
  - `create_snapshot`:
    - Stage everything, respecting `.gitignore`.
    - If the tree equals HEAD's tree, return `None`.
    - Otherwise commit with the given title and trailers `InfinaBox-Snapshot: 1`, plus `InfinaBox-Thread: <id>` and `InfinaBox-Turn: <n>` when an origin is given.
    - The signature comes from the repo/user git config if present, else `InfinaBox <infinabox@localhost>`. Never write git config.
    - Initialize the repo if missing.
  - `list_snapshots`: walk from HEAD, keep commits carrying the trailer, and parse title, trailers, and files-changed count (diff against parent).
  - `restore_to`:
    - First snapshot any uncommitted changes as "Auto-save before going back".
    - Then make a new commit whose tree is the target's tree, titled `Went back to: <target title>`.
    - Then update the working directory to match (checkout of that tree with force *after* the auto-save, so nothing is lost).
    - History is never rewritten.
  - `undo_last`: find the latest snapshot and `restore_to` its parent snapshot. If the latest snapshot is itself a restore, undo that, so pressing Undo twice redoes.
- [ ] **`redact.rs`:** replace known credential formats with `[redacted]`:
  - Anthropic, OpenAI, GitHub, AWS access key, Stripe, and Supabase service-role/JWT-looking tokens
  - Generic `api_key=`/`token:`/`Authorization: Bearer` values
  - PEM private key blocks

  Err on the side of redacting. Table-driven tests, including false-positive checks on normal game text and GDScript.
- [ ] **`chat_store.rs`:**
  - Files are `.ibproject/chat/<thread_id>.jsonl`. The first line is the thread header (`ThreadSummary` plus `"kind":"thread"`); following lines are `ChatRecord`s.
  - Every string passes through `redact` before writing.
  - `set_provider_session` rewrites the header line atomically (write temp, rename).
  - Thread ids are sortable: timestamp plus random suffix.
  - `list_threads` sorts newest first.
- [ ] **Tests:** all against real temp git repos and dirs:
  - create/list/no-op snapshot
  - restore keeps later history and restores content exactly
  - restore auto-saves uncommitted work
  - undo twice
  - trailer parsing
  - chat round-trip
  - redaction applied on write
  - header rewrite

**Acceptance:** `cargo test -p infinabox-core snapshot chat_store redact` passes.

### Task E: Project scaffold and Godot addon

**Owns:** `crates/core/src/scaffold.rs`, `templates/blank-2d/**`, `godot-addon/infinabox/**`.

- [ ] **`templates/blank-2d/`** — a minimal Godot 4 project:
  - `project.godot` naming the main scene and a 1280×720 window
  - `main.tscn` with a `Node2D`, a background `ColorRect`, and a simple controllable `CharacterBody2D` player using a built-in shape, so the AI has something real to change
  - `player.gd`
  - `.gitignore` ignoring `.godot/`
  - Keep it small and well commented.
- [ ] **`godot-addon/infinabox/`:**
  - `plugin.cfg` and a runtime autoload script `infinabox_runtime.gd`.
  - In `_ready`, only when `OS.is_debug_build()`, it prints `[infinabox] ready <addon version>` to stdout. It does nothing in release exports.
  - It is registered as an autoload in `project.godot`. It must be a runtime autoload, not only an editor plugin: editor plugins don't run in the game.
  - Keep it minimal in Phase A; screenshots and live tuning come in Phase D.
- [ ] **`scaffold.rs`:**
  - Embed both directories with `include_dir` or `include_str!`.
  - `create_project` copies the template and installs the addon.
  - It writes `.ibproject/.ibx` as version 2 (`name`, `createdAt`, `formatVersion: 2`, `dimension: "2d"`). Keep the JSON shape compatible with `src/lib/project-picker.ts`'s reader: it only checks the file exists.
  - It creates `.ibproject/context/concept.md` (front-matter `type: concept`, a short placeholder the user and AI will fill in, clearly labelled as a starting point) and `.ibproject/chat/`.
  - It writes `CLAUDE.md` and `AGENTS.md` explaining the project layout, the Context folder, and the `infinabox` MCP tools.
  - It runs `git init` and makes the first snapshot "New project".
  - It refuses an existing non-empty target directory.
  - `ensure_addon` installs or updates the addon files and the autoload entry idempotently.
- [ ] **Tests:**
  - Scaffold into a temp dir: expected files exist, `.ibx` parses, exactly one snapshot exists.
  - `ensure_addon` is idempotent.
  - Refuses a non-empty directory.
  - `#[ignore]`: boot the scaffolded project headless with a real Godot via Task C's validate (or a direct command if C isn't merged yet) and assert zero errors and the `[infinabox] ready` line.

**Acceptance:** `cargo test -p infinabox-core scaffold` passes; the ignored boot test passes with a real Godot.

### Task F1: Studio chat UI

**Owns:** `src/components/studio/chat/**`.
**Uses only:** `src/lib/studio-api.ts`, `src/lib/studio-types.ts`, existing `src/components/ui/*`, `src/lib/motion.ts`.

- [ ] **`chat-reducer.ts`:** a pure function folding `ChatRecord`s and live `AgentEvent`s into a view model:
  - user bubbles and assistant text bubbles
  - collapsed "working" rows grouping tool uses with their results, e.g. "Edited 2 files, ran the game"
  - an error card
  - a turn-in-progress flag

  Written so a test runner could test it later: no React inside.
- [ ] **`ChatPanel.tsx`:** loads the latest thread (or creates "Main" if none), renders history, subscribes to `agent-event` for the active thread, and shows "Stop" while a turn runs (`agentCancel`).
  - The header has a thread picker (list and new thread).
  - Exposes an imperative `sendMessage(text)` through a prop callback registration, so the Play panel's "Ask AI to fix" can inject a message.
- [ ] **`ChatComposer.tsx`:** multiline input, Enter to send, Shift+Enter for a newline, disabled while a turn runs.
- [ ] **`AgentStatusBanner.tsx`:** uses `agentStatus()`.
  - If `claude` isn't installed: an honest message with the official install command and docs link, plus a "Check again" button.
  - If a turn returns `NotAuthenticated`: tell the user to run `claude` once in Advanced → terminal to sign in. The full guided flow is Phase B.
  - `RateLimited` shows the provider's real message.
- [ ] **Explanations:** the assistant's final text renders as markdown (reuse an existing markdown renderer if one is in the bundle; otherwise plain text with paragraphs).
- [ ] **Verify:** `npx tsc --noEmit`, `npm run build`. Every call goes through `studio-api.ts`.

### Task F2: Studio Play and History UI

**Owns:** `src/components/studio/play/**`, `src/components/studio/history/**`.

- [ ] **`PlayPanel.tsx`:**
  - Shows real `godot_status`. Not installed shows an "Install Godot" button with a progress bar from `godot-install-progress` (real bytes; indeterminate when the total is unknown).
  - Installed shows Play/Stop/Restart driven by `game-state`.
  - An output log (virtualized or capped at the last N lines) with error lines highlighted.
  - An errors list from `game-error`; each error has "Ask AI to fix", which calls the injected `sendMessage` with the error message, file, and line.
  - Shows the Godot version in small text.
- [ ] **`HistoryPanel.tsx`:**
  - The snapshot list (title, relative time, files changed), refreshed on `snapshots-changed` and `agent-turn-finished`.
  - An "Undo last change" button.
  - A per-row "Go back to this point" action behind an `AlertDialog` confirmation that says plainly that nothing is lost and it can be undone.
  - A small honest note: "History is saved on this computer only."
- [ ] **Verify:** `npx tsc --noEmit`, `npm run build`.

---

## Wave 2 (3 subagents in parallel)

**Status (2026-09-25):** H and I built in parallel, reviewed and merged. G (agent and chat commands) waits for Task A. Notes from Wave 2:
- Added a `game_status` command to the contract so the Play panel starts from the real state.
- The bridge enforces a 3s total hello deadline and keeps separate connection budgets for unauthenticated and authenticated clients.
- `godot::restart_if_running` blocks (it may re-import assets); callers run it on a background thread, as the snapshot commands do. Task G must do the same.
- The MCP client's bridge response timeout is 330s, just above core's 300s import limit.

### Task G: Agent and chat commands

**Owns:** `src-tauri/src/commands/agent.rs`.

- [ ] `AgentState` holds one `ClaudeCodeRuntime` and a per-thread "turn in progress" set, rejecting a second concurrent turn on the same thread.
- [ ] `agent_send`:
  - Append the user record to the chat store.
  - Spawn a background thread that builds a `TurnRequest`:
    - resume id from the thread header
    - `McpLaunch` = current exe + `--mcp-server` + bridge env from `BridgeState` + `INFINABOX_PROJECT`
  - Run the turn, and for each event: append it to the chat store, emit `agent-event`, and on `SessionStarted` save the provider session id.
  - After `TurnCompleted`:
    - If any `FilesChanged`, call `snapshot::create_snapshot` with a title derived from the user's message (first line, trimmed to ~60 characters) and origin (thread, turn number).
    - Commit the chat file in the same snapshot: write the chat records *before* snapshotting.
    - Emit `agent-turn-finished` and `snapshots-changed`.
    - If the game is running, ask `GodotState` to restart it. This is the Phase A "watch the game relaunch" behavior.
- [ ] **Always end the turn:** emit `agent-turn-finished` exactly once per `agent_send`, on every path: normal completion, `run_turn` returning `Err` (e.g. the CLI fails to start), cancel, or an `Error` event with no `TurnCompleted`. When the turn fails without the runtime reporting why, append and emit an `AgentEvent::Error` with the real error text first. The chat UI relies on this to leave its "working" state. Also save the user message and every event to the chat store, because the UI reloads the thread from disk after each turn.
- [ ] `agent_cancel`, `agent_status`, and the `chat_*` commands are thin wrappers.
- [ ] **Verify:** `cargo check`, `cargo test --workspace --no-default-features`. Report a manual run: one real turn from the UI, with the JSONL file contents and the created snapshot.

### Task H: Godot commands and bridge server

**Owns:** `src-tauri/src/commands/godot.rs`, `src-tauri/src/commands/bridge.rs`.

- [ ] **`GodotState`:**
  - Holds the current `GameProcess`, the error parser/ring buffers, and the current project path.
  - `game_run` stops any running game first, calls `scaffold::ensure_addon`, then starts the game.
  - The window hint puts the game window beside the InfinaBox window, computed from the main window's position and size, falling back to no hint.
  - Emits `game-state`, `game-output`, and `game-error`.
  - Exposes `restart_if_running()` for Task G.
- [ ] **Findings from Task C to handle here:**
  - `godot --path` does not import newly added assets (a new `icon.png` fails with `No loader found for resource`). Call `godot::validate::import_assets` before `GameProcess::start` when project files changed since the last import (at least after every AI turn that changed files); it takes a few seconds, so report it as the `starting` state.
  - An error block is only known to be complete when the next stderr line arrives. Call the parser's `finish()` when stderr has been quiet briefly (check `has_pending()`) and always from `on_exit`, so the last error of a running game isn't held back.
  - `--import` output contains ANSI colour codes even when piped; strip them before storing output lines.
- [ ] **`godot_install`:** runs `godot::install` on a background thread with the app data dir, emits progress, and returns the final status. `godot_status` is a thin wrapper.
- [ ] **`bridge.rs`:**
  - `start(app)` binds `127.0.0.1:0`, generates a token (`rand`), and stores the address and token in `BridgeState`.
  - Accepts connections on a background thread. It checks the `hello` token (constant-time compare), then serves `BridgeRequest`s by calling into `GodotState`. `RunGame` runs the project named in the connection's `Hello.project`.
  - Every connection handles one request at a time. Malformed input closes the connection.
- [ ] **Verify:** `cargo check`, `cargo test --workspace --no-default-features`. Report a manual run: install Godot, run the scaffolded project, and see a deliberate error arrive as a `game-error` event. Then use the bridge from `infinabox-cli mcp-server` to run the game.

### Task I: Snapshot and scaffold commands, app wiring

**Owns:** `src-tauri/src/commands/snapshot.rs`, `src-tauri/src/commands/scaffold.rs`, `src/App.tsx`, `src/components/cockpit/Sidebar.tsx`, `src/components/cockpit/DashboardSection.tsx`, `src/components/studio/StudioSection.tsx`.

- [ ] Snapshot commands and `project_create` are thin wrappers. Restore and undo emit `snapshots-changed`, and restart the game if it's running.
- [ ] **`StudioSection.tsx`:** `ResizablePanelGroup` with `storageKey="studio"`, the chat on the left and a right column with PlayPanel above HistoryPanel. It connects ChatPanel's `sendMessage` to PlayPanel's "Ask AI to fix".
- [ ] **`App.tsx`:**
  - Add `"studio"` to `Section` and to `PERSISTENT_SECTIONS`, following the existing stacked/inert crossfade mechanism exactly. Studio holds a live turn stream and must never unmount.
  - Opening a project from the dashboard lands on `"studio"` instead of `"build"`.
  - Update the file's comments where they list the persistent sections.
- [ ] **`Sidebar.tsx`:** add a pinned "Studio" entry above Build (icon: `Sparkles` or similar from lucide). The full 6-entry restructure is Phase B; don't remove anything else now.
- [ ] **`DashboardSection.tsx`:** the "New Project" flow calls `project_create` instead of creating `.ibproject/docs`/`graphs` and writing `.ibx` from the frontend. Keep the existing dialog and location picker.
  - Old-format projects still open fine, since `isInfinaBoxProject` only checks `.ibx`.
  - Opening an old-format project in Studio runs `ensure_addon` on first Play, so it still works as long as it's a Godot project. If there's no `project.godot`, Play shows an honest "This project isn't a Godot game yet" message.
- [ ] **Verify:** all four verification commands. Report a manual run of creating a project and seeing it open in Studio.

---

## Wave 3: integration and exit criterion (lead, with fix-up subagents)

### Task 3.1: Integrate
- [ ] All Wave 1 and 2 branches merged in the stated order. Full verification passes after each merge.
- [ ] Run every `#[ignore]` test on a machine with a real Godot and a logged-in `claude`: `cargo test --workspace --no-default-features -- --ignored`. Record the results.
- [ ] Grep for leftover `not implemented yet` stubs: there must be none.

### Task 3.2: End-to-end exit criterion
- [ ] Run `npm run tauri dev` and walk through the six exit-criterion steps at the top of this plan exactly. Record what happened at each step, with screenshots.
- [ ] Each failure becomes a small, self-contained fix task. The lead dispatches **independent fixes to subagents in parallel** (worktree isolation, same review-then-merge process). Fixes that touch the same files are done in sequence.
- [ ] Repeat until all six steps pass in one uninterrupted run.

### Task 3.3: Documentation
- [ ] Update `CLAUDE.md`:
  - the new crates and modules
  - the new commands and events
  - Studio as a persistent section
  - the bridge and `--mcp-server` launch
  - the fixture folders and `--ignored` tests
  - that a chat panel now exists by design, per the spec
- [ ] Add a short "Phase A complete" status note to `docs/superpowers/plans/2026-09-22-project-status-and-roadmap.md`, or supersede it with a new status doc.
- [ ] Write down anything learned about the Claude Code CLI or Godot that differed from this plan, for Phase B.

---

## Risks to watch during execution

| Risk | Where | Response |
|---|---|---|
| Claude Code CLI flags or stream format differ from this plan | Task 0.2, A | Fixtures are the source of truth; adjust the parser, not the fixtures |
| GUI-launched app can't find `claude` or `godot` on PATH (macOS) | A, C | Login-shell PATH resolution in Task A; the managed Godot install avoids PATH for Godot entirely |
| MCP tool naming or allowlist syntax in the CLI differs | A, B | Confirmed in fixture scenario (d) before Task A's allowlist is written |
| The agent edits files while the game is running | G, H | The game restarts after the turn, never mid-turn |
| Snapshots include huge or generated files | D, E | Template `.gitignore` covers `.godot/`; snapshot respects `.gitignore` |
| Parallel subagents drift from contracts | all | Contracts frozen in Wave 0; reviewers check contract use; contract changes go through the lead only |
| Merge conflicts between parallel branches | Waves 1–2 | Disjoint ownership plus pre-registered stubs; lead merges in a fixed order and re-verifies after each |
