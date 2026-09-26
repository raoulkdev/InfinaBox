//! Studio chat: runs agent turns through `infinabox_core::agent` on a
//! background thread, streams each event to the frontend as `agent-event`,
//! persists everything to the project's chat store, and snapshots the
//! project after any turn that changed files.
//!
//! Split the same way `snapshot.rs` and `watcher.rs` are: the turn itself
//! (`start_turn` + `run_turn_to_end`) is plain code over a `TurnRunner` and
//! a `TurnSink`, with no `AppHandle`, so its ordering and failure handling
//! are unit-tested against real temp projects; the `#[tauri::command]`s only
//! wire the sink to `app.emit(...)` and the game restart.
//!
//! The one promise the chat UI depends on: every successful `agent_send`
//! ends with exactly one `agent-turn-finished`, whatever happens in between
//! (the CLI failing to start, a cancel, a chat-file or snapshot error), and
//! the turn's events on disk include a `turn_completed` — so a reload after
//! the turn never shows it running.
//!
//! Two limits on that promise:
//! - A panic in the runtime is caught (and the turn still ended) only when
//!   panics unwind: in debug and test builds. `src-tauri/Cargo.toml` asks
//!   for `panic = "abort"` in release; while this crate is a workspace
//!   member cargo ignores that profile (it warns "profiles for the non root
//!   package will be ignored"), so release builds unwind today too — but if
//!   the profile moves to the workspace root, a panic there takes the whole
//!   app down instead. After such a crash and a relaunch the UI isn't stuck
//!   anyway: ChatPanel's set of running threads starts empty, and the
//!   transcript just ends without a `turn_completed`.
//! - `turn_completed` isn't always the file's last event for the turn: a
//!   chat-store failure noticed during the turn, or a snapshot that failed
//!   after it, is reported as an `error` event after it (so both the live
//!   view and a reload show it).

use std::collections::{HashMap, HashSet};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

use infinabox_core::agent::claude::ClaudeCodeRuntime;
use infinabox_core::agent::claude_stream::STOPPED_MESSAGE;
use infinabox_core::agent::{
    AgentErrorKind, AgentEvent, AgentRuntime, McpLaunch, RuntimeStatus, TurnRequest,
};
use infinabox_core::chat_store::{self, ChatRecord, ThreadSummary};
use infinabox_core::snapshot::{self, Snapshot};
use infinabox_mcp_server::{ENV_BRIDGE_ADDR, ENV_BRIDGE_TOKEN, ENV_PROJECT, MCP_SERVER_FLAG};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::bridge::BridgeState;
use crate::commands::snapshot::EVENT_SNAPSHOTS_CHANGED;

/// Event names emitted by this module (frontend: `src/lib/studio-api.ts`).
pub const EVENT_AGENT: &str = "agent-event";
pub const EVENT_TURN_FINISHED: &str = "agent-turn-finished";

/// The provider name written into new thread headers.
const PROVIDER: &str = "claude-code";

/// Snapshot titles are the user's own words, cut to about this many
/// characters (History shows them on one line).
const TITLE_MAX_CHARS: usize = 60;

/// What the runtime says (in an `Error` event) when `--resume` names a
/// conversation it doesn't have — see the `e_bad_resume` fixture.
const BAD_RESUME_MARKER: &str = "No conversation found";

/// How often a stop the runtime may have missed is sent again (see
/// `resend_stop_until_started`).
const STOP_RESEND_INTERVAL: Duration = Duration::from_millis(250);

/// The InfinaBox MCP tool that (re)starts the game (`crates/mcp-server`),
/// as the CLI names it. The Director prompt tells the agent to call it
/// after editing, so a turn that did doesn't need a restart afterwards.
const RUN_GAME_TOOL: &str = "mcp__infinabox__run_game";

/// Tools that can't change the game's files. Every other tool (edits,
/// `Bash`, helpers, other MCP servers' tools) is assumed to possibly
/// change them. InfinaBox's own MCP tools (`mcp__infinabox__*`) are all
/// in this group too: the only one that writes, `write_context_card`,
/// writes design notes the running game doesn't load.
const READ_ONLY_TOOLS: &[&str] = &[
    "Read",
    "Glob",
    "Grep",
    "LS",
    "WebFetch",
    "WebSearch",
    "TodoWrite",
    "ToolSearch",
];

fn may_change_game_files(tool: &str) -> bool {
    !(tool.starts_with("mcp__infinabox__") || READ_ONLY_TOOLS.contains(&tool))
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AgentEventPayload {
    pub thread_id: String,
    pub event: AgentEvent,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TurnFinishedPayload {
    pub thread_id: String,
    pub snapshot: Option<Snapshot>,
}

#[derive(Serialize, Clone)]
pub struct LoadedThread {
    pub thread: ThreadSummary,
    pub records: Vec<ChatRecord>,
}

/// Threads with a turn in flight, each with its "stop requested" flag.
type ActiveTurns = Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>;

/// Tauri-managed state: the one agent runtime, plus which threads have a
/// turn in flight (a second concurrent turn on the same thread is refused).
#[derive(Default)]
pub struct AgentState {
    pub runtime: Arc<ClaudeCodeRuntime>,
    active_turns: ActiveTurns,
}

/// Formats a core error with its whole cause chain, like `snapshot.rs`.
fn user_error(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // A poisoned lock only means a turn thread panicked while holding it;
    // the map/unit inside is still usable.
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// The testable turn machinery
// ---------------------------------------------------------------------------

/// What a turn needs from a runtime. Every `AgentRuntime` is one (the
/// blanket impl below); it exists so this crate's tests can drive the turn
/// machinery with a scripted runtime without depending on `anyhow`.
pub(crate) trait TurnRunner: Send + Sync {
    fn run(&self, req: TurnRequest, on_event: &mut dyn FnMut(AgentEvent)) -> Result<(), String>;
    fn cancel(&self, thread_id: &str);
}

impl<R: AgentRuntime + ?Sized> TurnRunner for R {
    fn run(&self, req: TurnRequest, on_event: &mut dyn FnMut(AgentEvent)) -> Result<(), String> {
        self.run_turn(req, on_event).map_err(user_error)
    }

    fn cancel(&self, thread_id: &str) {
        AgentRuntime::cancel(self, thread_id)
    }
}

/// Where a turn's output goes (in the app: Tauri events).
pub(crate) trait TurnSink {
    /// One event, already appended to the chat store (or attempted).
    fn event(&self, thread_id: &str, event: &AgentEvent);
    /// The turn is over. Called exactly once per started turn, last.
    /// `restart_game` is whether a running game should be restarted onto
    /// the turn's changes (see `TurnLog::restart_game`).
    fn finished(&self, thread_id: &str, snapshot: Option<&Snapshot>, restart_game: bool);
}

/// A thread's claim on "the turn in progress". Dropping it (normally at the
/// end of `run_turn_to_end`, or on any unwind) frees the thread for its
/// next turn.
pub(crate) struct TurnSlot {
    active: ActiveTurns,
    thread_id: String,
    stop_requested: Arc<AtomicBool>,
}

impl Drop for TurnSlot {
    fn drop(&mut self) {
        let mut active = lock(&self.active);
        // Only remove our own entry (never a later turn's).
        if active
            .get(&self.thread_id)
            .is_some_and(|flag| Arc::ptr_eq(flag, &self.stop_requested))
        {
            active.remove(&self.thread_id);
        }
    }
}

/// The synchronous half of `agent_send`: checks the request, claims the
/// thread (refusing a second concurrent turn, without emitting anything),
/// and saves the user's message. On `Err` nothing was started.
pub(crate) fn start_turn(
    active: &ActiveTurns,
    project: &Path,
    thread_id: &str,
    message: &str,
) -> Result<TurnSlot, String> {
    if message.trim().is_empty() {
        return Err("There's no message to send.".into());
    }
    if !project.is_absolute() || !project.is_dir() {
        return Err(format!(
            "The project folder {} doesn't exist.",
            project.display()
        ));
    }
    let stop_requested = Arc::new(AtomicBool::new(false));
    {
        let mut active = lock(active);
        if active.contains_key(thread_id) {
            return Err("The AI is still working on the last message in this chat. \
                 Wait for it to finish, or stop it first."
                .into());
        }
        active.insert(thread_id.to_string(), stop_requested.clone());
    }
    let slot = TurnSlot {
        active: active.clone(),
        thread_id: thread_id.to_string(),
        stop_requested,
    };
    let record = ChatRecord::User {
        text: message.to_string(),
        at: now_unix(),
    };
    // On failure `slot` drops here and frees the thread again.
    chat_store::append(project, thread_id, &record)
        .map_err(|e| format!("Your message couldn't be saved to this chat: {e:#}"))?;
    Ok(slot)
}

/// Asks the thread's turn to stop. Also flags it, for a turn that hasn't
/// reached the runtime yet (or whose CLI hasn't registered as running —
/// the runtime may spend a few seconds finding `claude` first).
pub(crate) fn cancel_turn(runner: &dyn TurnRunner, active: &ActiveTurns, thread_id: &str) {
    if let Some(flag) = lock(active).get(thread_id) {
        flag.store(true, Ordering::SeqCst);
    }
    runner.cancel(thread_id);
}

/// Everything recorded during one turn, and what it implies afterwards.
struct TurnLog<'a> {
    project: &'a Path,
    thread_id: &'a str,
    sink: &'a dyn TurnSink,
    saw_session: bool,
    saw_error: bool,
    saw_completed: bool,
    saw_bad_resume: bool,
    files_changed: bool,
    /// This turn's number, once the thread has been read.
    turn: Option<u32>,
    /// The first chat-store failure, reported once at the end of the turn.
    store_error: Option<String>,
    /// How many events came before the one being recorded: orders tool
    /// uses and results within the turn.
    seq: usize,
    /// Tool uses that may change game files and haven't reported back yet.
    open_changes: HashSet<String>,
    /// When the last possibly-file-changing tool was used or finished.
    last_change: Option<usize>,
    /// `run_game` uses that haven't reported back yet, and when each began.
    open_runs: HashMap<String, usize>,
    /// When the latest `run_game` that succeeded began, if it began after
    /// every change before it had finished.
    verified_run: Option<usize>,
}

impl<'a> TurnLog<'a> {
    fn new(project: &'a Path, thread_id: &'a str, sink: &'a dyn TurnSink) -> Self {
        Self {
            project,
            thread_id,
            sink,
            saw_session: false,
            saw_error: false,
            saw_completed: false,
            saw_bad_resume: false,
            files_changed: false,
            turn: None,
            store_error: None,
            seq: 0,
            open_changes: HashSet::new(),
            last_change: None,
            open_runs: HashMap::new(),
            verified_run: None,
        }
    }

    /// Whether a running game should be restarted onto this turn's changes:
    /// yes when files changed, unless the agent already ran the game itself
    /// (a `run_game` that succeeded and began after the last tool that
    /// could have changed files had finished). Restarting then would only
    /// re-import and relaunch the same files, throwing away the game state
    /// the agent just checked.
    fn restart_game(&self) -> bool {
        let already_ran = self
            .verified_run
            .is_some_and(|run| self.last_change.is_none_or(|change| change < run));
        self.files_changed && !already_ran
    }

    /// Tracks tool uses/results for `restart_game`.
    fn note_tool(&mut self, event: &AgentEvent) {
        let at = self.seq;
        match event {
            AgentEvent::ToolUse { id, name, .. } if name == RUN_GAME_TOOL => {
                self.open_runs.insert(id.clone(), at);
            }
            AgentEvent::ToolUse { id, name, .. } if may_change_game_files(name) => {
                self.open_changes.insert(id.clone());
                self.last_change = Some(at);
            }
            AgentEvent::ToolResult { id, ok, .. } => {
                // A failed change may still have written something.
                if self.open_changes.remove(id) {
                    self.last_change = Some(at);
                }
                if let Some(began) = self.open_runs.remove(id) {
                    // Nothing that might change files was still running,
                    // and nothing had changed since it began.
                    let saw_latest = self.open_changes.is_empty()
                        && self.last_change.is_none_or(|change| change < began);
                    if *ok && saw_latest {
                        self.verified_run = Some(began);
                    }
                }
            }
            _ => {}
        }
    }

    /// Saves one event, then shows it. A save failure never stops the turn
    /// (the user still sees the event live); it's reported at the end.
    fn record(&mut self, event: AgentEvent) {
        match &event {
            AgentEvent::SessionStarted {
                provider_session_id,
                ..
            } => {
                self.saw_session = true;
                if let Err(e) = chat_store::set_provider_session(
                    self.project,
                    self.thread_id,
                    Some(provider_session_id),
                ) {
                    self.note_store_error(e);
                }
            }
            AgentEvent::FilesChanged { paths } if !paths.is_empty() => self.files_changed = true,
            AgentEvent::TurnCompleted { .. } => self.saw_completed = true,
            AgentEvent::Error { message, .. } => {
                self.saw_error = true;
                if message.contains(BAD_RESUME_MARKER) {
                    self.saw_bad_resume = true;
                }
            }
            AgentEvent::ToolUse { .. } | AgentEvent::ToolResult { .. } => self.note_tool(&event),
            _ => {}
        }
        self.seq += 1;
        let record = ChatRecord::Event {
            event,
            at: now_unix(),
        };
        if let Err(e) = chat_store::append(self.project, self.thread_id, &record) {
            self.note_store_error(e);
        }
        if let ChatRecord::Event { event, .. } = &record {
            self.sink.event(self.thread_id, event);
        }
    }

    fn note_store_error(&mut self, e: impl std::fmt::Display) {
        eprintln!("agent: chat store write failed: {e:#}");
        if self.store_error.is_none() {
            self.store_error = Some(user_error(e));
        }
    }

    fn error(&mut self, message: String) {
        self.record(AgentEvent::Error {
            kind: AgentErrorKind::Other,
            message,
        });
    }

    /// Closes the turn if the runtime didn't: an `Error` saying so (unless
    /// one was already reported) and a failed `TurnCompleted`.
    fn ensure_ended(&mut self) {
        if self.saw_completed {
            return;
        }
        if !self.saw_error {
            self.error("The AI stopped without finishing its reply.".into());
        }
        self.record(AgentEvent::TurnCompleted {
            is_error: true,
            duration_ms: None,
            usage: None,
        });
    }
}

/// Runs the turn through the runtime, recording every event (and, in
/// `log.turn`, this turn's number: the count of user messages so far).
fn drive(
    runner: &dyn TurnRunner,
    slot: &TurnSlot,
    project: &Path,
    message: &str,
    mcp: Result<McpLaunch, String>,
    log: &mut TurnLog,
) {
    let thread_id = slot.thread_id.as_str();
    let (thread, records) = match chat_store::load_thread(project, thread_id) {
        Ok(loaded) => loaded,
        Err(e) => {
            log.error(format!("This chat couldn't be read: {e:#}"));
            return;
        }
    };
    log.turn = Some(
        records
            .iter()
            .filter(|r| matches!(r, ChatRecord::User { .. }))
            .count() as u32,
    );
    let mcp = match mcp {
        Ok(mcp) => mcp,
        Err(e) => {
            log.error(e);
            return;
        }
    };
    if slot.stop_requested.load(Ordering::SeqCst) {
        log.error(STOPPED_MESSAGE.into());
        return;
    }

    let resume = thread.provider_session_id;
    let resumed = resume.is_some();
    let req = TurnRequest {
        thread_id: thread_id.to_string(),
        project_path: project.to_path_buf(),
        message: message.to_string(),
        resume_provider_session_id: resume,
        mcp,
    };

    let started = AtomicBool::new(false);
    let ended = RunEnded::default();
    let result = std::thread::scope(|scope| {
        scope.spawn(|| {
            resend_stop_until_started(runner, thread_id, &slot.stop_requested, &started, &ended)
        });
        // Wakes the watcher however `run` ends, a panic included (the
        // scope waits for it before unwinding further).
        let _ended = EndOnDrop(&ended);
        let mut stop_forwarded = false;
        runner.run(req, &mut |event| {
            // Once the runtime emits anything, the turn is registered with
            // it, so a stop it missed while starting up lands now.
            if !stop_forwarded && slot.stop_requested.load(Ordering::SeqCst) {
                stop_forwarded = true;
                runner.cancel(thread_id);
            }
            started.store(true, Ordering::SeqCst);
            log.record(event);
        })
    });
    if let Err(e) = result {
        log.error(e);
    }

    // The provider no longer has this conversation (a bad `--resume` gets
    // no `SessionStarted`): forget it so the next turn starts fresh instead
    // of failing the same way forever.
    if resumed && log.saw_bad_resume && !log.saw_session {
        if let Err(e) = chat_store::set_provider_session(project, thread_id, None) {
            log.note_store_error(e);
        }
    }
}

/// Set, and its waiter woken, once `runner.run` has returned or unwound.
#[derive(Default)]
struct RunEnded {
    done: Mutex<bool>,
    wake: Condvar,
}

struct EndOnDrop<'a>(&'a RunEnded);

impl Drop for EndOnDrop<'_> {
    fn drop(&mut self) {
        *lock(&self.0.done) = true;
        self.0.wake.notify_all();
    }
}

/// A stop pressed while the runtime is still starting up (finding `claude`
/// on the login shell's `PATH` can take seconds) can reach it before the
/// turn is registered there, and be lost. So while a stop is requested and
/// the runtime hasn't emitted anything yet, send it again every
/// `STOP_RESEND_INTERVAL`. From the first event on the runtime has the
/// turn, and `drive` forwards the stop once more itself, so this ends then
/// (or when the run does).
fn resend_stop_until_started(
    runner: &dyn TurnRunner,
    thread_id: &str,
    stop_requested: &AtomicBool,
    started: &AtomicBool,
    ended: &RunEnded,
) {
    let mut done = lock(&ended.done);
    while !*done && !started.load(Ordering::SeqCst) {
        if stop_requested.load(Ordering::SeqCst) {
            // Not under the lock: a cancel may take the runtime's locks.
            drop(done);
            runner.cancel(thread_id);
            done = lock(&ended.done);
            if *done {
                break;
            }
        }
        done = ended
            .wake
            .wait_timeout(done, STOP_RESEND_INTERVAL)
            .unwrap_or_else(|e| e.into_inner())
            .0;
    }
}

/// First non-empty line of the message, cut to about `TITLE_MAX_CHARS`.
pub(crate) fn snapshot_title(message: &str) -> String {
    let line = message
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    if line.is_empty() {
        return "AI change".into();
    }
    if line.chars().count() <= TITLE_MAX_CHARS {
        return line.to_string();
    }
    let cut: String = line.chars().take(TITLE_MAX_CHARS - 1).collect();
    // Prefer ending on a word boundary when there's one reasonably close.
    let cut = match cut.rfind(' ') {
        Some(i) if i >= TITLE_MAX_CHARS / 2 => &cut[..i],
        _ => cut.as_str(),
    };
    format!("{}…", cut.trim_end())
}

fn panic_text(panic: &(dyn std::any::Any + Send)) -> String {
    panic
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown error".into())
}

/// The background half of `agent_send`: runs the turn, ends it exactly
/// once, snapshots any file changes (chat included, since every record is
/// written first), frees the thread, then reports `finished`.
pub(crate) fn run_turn_to_end(
    runner: &dyn TurnRunner,
    slot: TurnSlot,
    project: &Path,
    message: &str,
    mcp: Result<McpLaunch, String>,
    sink: &dyn TurnSink,
) {
    let thread_id = slot.thread_id.clone();
    let mut log = TurnLog::new(project, &thread_id, sink);

    if let Err(panic) = catch_unwind(AssertUnwindSafe(|| {
        drive(runner, &slot, project, message, mcp, &mut log)
    })) {
        let text = panic_text(panic.as_ref());
        eprintln!("agent: turn panicked: {text}");
        let _ = catch_unwind(AssertUnwindSafe(|| {
            log.error(format!(
                "InfinaBox hit an internal error while the AI was working: {text}"
            ))
        }));
    }

    let snapshot = catch_unwind(AssertUnwindSafe(|| {
        log.ensure_ended();
        if let Some(e) = log.store_error.take() {
            log.error(format!("Part of this chat couldn't be saved: {e}"));
        }
        if !log.files_changed {
            return None;
        }
        let title = snapshot_title(message);
        let origin = log.turn.map(|n| (thread_id.as_str(), n));
        // `create_snapshot` serializes itself with every other snapshot,
        // restore and undo in the app.
        match snapshot::create_snapshot(project, &title, origin) {
            Ok(snapshot) => snapshot,
            Err(e) => {
                log.error(format!(
                    "This change couldn't be saved to your history: {e:#}"
                ));
                None
            }
        }
    }))
    .unwrap_or_else(|panic| {
        eprintln!(
            "agent: finishing the turn panicked: {}",
            panic_text(panic.as_ref())
        );
        None
    });

    let restart_game = log.restart_game();
    // Free the thread before announcing the end, so a message sent in
    // reaction to `finished` isn't refused as "still working".
    drop(slot);
    sink.finished(&thread_id, snapshot.as_ref(), restart_game);
}

// ---------------------------------------------------------------------------
// Tauri wiring
// ---------------------------------------------------------------------------

struct AppSink(AppHandle);

impl TurnSink for AppSink {
    fn event(&self, thread_id: &str, event: &AgentEvent) {
        let _ = self.0.emit(
            EVENT_AGENT,
            AgentEventPayload {
                thread_id: thread_id.to_string(),
                event: event.clone(),
            },
        );
    }

    fn finished(&self, thread_id: &str, snapshot: Option<&Snapshot>, restart_game: bool) {
        let _ = self.0.emit(
            EVENT_TURN_FINISHED,
            TurnFinishedPayload {
                thread_id: thread_id.to_string(),
                snapshot: snapshot.cloned(),
            },
        );
        if snapshot.is_some() {
            let _ = self.0.emit(EVENT_SNAPSHOTS_CHANGED, ());
        }
        if restart_game {
            // Blocks (it may re-import assets), so on its own thread, with
            // none of this module's locks held. A failed restart shows up
            // through the game's own `game-state`/`game-error` events.
            let app = self.0.clone();
            std::thread::spawn(move || {
                if let Err(e) = crate::commands::godot::restart_if_running(&app) {
                    eprintln!("agent: restarting the game after an AI change failed: {e}");
                }
            });
        }
    }
}

/// How the agent CLI launches the InfinaBox MCP server: this same
/// executable with `--mcp-server` (see `main.rs`), told the project and how
/// to reach this app's bridge.
fn mcp_launch(app: &AppHandle, project: &str) -> Result<McpLaunch, String> {
    let command = std::env::current_exe().map_err(|e| {
        format!("InfinaBox couldn't find its own program file to give the AI its tools: {e}")
    })?;
    let mut env = vec![(ENV_PROJECT.to_string(), project.to_string())];
    match lock(&app.state::<BridgeState>().0).as_ref() {
        Some(bridge) => {
            env.push((ENV_BRIDGE_ADDR.to_string(), bridge.addr.clone()));
            env.push((ENV_BRIDGE_TOKEN.to_string(), bridge.token.clone()));
        }
        // The context/history tools still work; the game tools will say
        // the app isn't reachable.
        None => eprintln!("agent: the bridge isn't running; the AI won't be able to run the game"),
    }
    Ok(McpLaunch {
        command,
        args: vec![MCP_SERVER_FLAG.to_string()],
        env,
    })
}

/// Checks for Claude Code. May take a few seconds the first time (it reads
/// the login shell's `PATH`), hence `async`.
#[tauri::command(async)]
pub fn agent_status(app: AppHandle) -> Result<RuntimeStatus, String> {
    Ok(app.state::<AgentState>().runtime.detect())
}

/// Saves the message and starts the turn on a background thread; returns
/// as soon as it has started. Everything after streams as `agent-event`s
/// and ends with one `agent-turn-finished`.
#[tauri::command(async)]
pub fn agent_send(
    app: AppHandle,
    project_path: String,
    thread_id: String,
    message: String,
) -> Result<(), String> {
    let state = app.state::<AgentState>();
    let project = PathBuf::from(&project_path);
    let slot = start_turn(&state.active_turns, &project, &thread_id, &message)?;
    let runtime = state.runtime.clone();
    let worker_app = app.clone();
    std::thread::Builder::new()
        .name("agent-turn".into())
        .spawn(move || {
            let mcp = mcp_launch(&worker_app, &project_path);
            let sink = AppSink(worker_app);
            run_turn_to_end(runtime.as_ref(), slot, &project, &message, mcp, &sink);
        })
        .map(|_| ())
        .map_err(|e| format!("The AI couldn't be started: {e}"))
}

/// Stops the thread's turn; it then ends normally (`agent-turn-finished`
/// still follows, after snapshotting anything already edited).
#[tauri::command(async)]
pub fn agent_cancel(app: AppHandle, thread_id: String) -> Result<(), String> {
    let state = app.state::<AgentState>();
    cancel_turn(state.runtime.as_ref(), &state.active_turns, &thread_id);
    Ok(())
}

#[tauri::command(async)]
pub fn chat_create_thread(project_path: String, title: String) -> Result<ThreadSummary, String> {
    chat_store::create_thread(Path::new(&project_path), &title, PROVIDER).map_err(user_error)
}

#[tauri::command(async)]
pub fn chat_list_threads(project_path: String) -> Result<Vec<ThreadSummary>, String> {
    chat_store::list_threads(Path::new(&project_path)).map_err(user_error)
}

#[tauri::command(async)]
pub fn chat_load_thread(project_path: String, thread_id: String) -> Result<LoadedThread, String> {
    let (thread, records) =
        chat_store::load_thread(Path::new(&project_path), &thread_id).map_err(user_error)?;
    Ok(LoadedThread { thread, records })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A fresh folder under the system temp dir (never inside this repo,
    /// which snapshots would refuse as "inside another git repository").
    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "infinabox-agent-cmd-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A project with one committed file (so a turn's edit is a real
    /// change) and one empty chat thread.
    fn project_with_thread(tag: &str) -> (PathBuf, String) {
        let dir = temp_dir(tag);
        fs::write(dir.join("player.gd"), "extends Node2D\n").unwrap();
        snapshot::create_snapshot(&dir, "Start", None)
            .unwrap()
            .unwrap();
        let thread = chat_store::create_thread(&dir, "Chat", PROVIDER).unwrap();
        (dir, thread.id)
    }

    type Script =
        dyn Fn(&TurnRequest, &mut dyn FnMut(AgentEvent)) -> Result<(), String> + Send + Sync;

    /// A runtime that plays a script, remembering what it was asked.
    struct Scripted {
        script: Box<Script>,
        requests: Mutex<Vec<TurnRequest>>,
        /// Shared, so a script can watch the cancels arrive mid-turn.
        cancels: Arc<Mutex<Vec<String>>>,
    }

    impl Scripted {
        fn new(
            script: impl Fn(&TurnRequest, &mut dyn FnMut(AgentEvent)) -> Result<(), String>
                + Send
                + Sync
                + 'static,
        ) -> Self {
            Self::with_cancels(Arc::default(), script)
        }

        fn with_cancels(
            cancels: Arc<Mutex<Vec<String>>>,
            script: impl Fn(&TurnRequest, &mut dyn FnMut(AgentEvent)) -> Result<(), String>
                + Send
                + Sync
                + 'static,
        ) -> Self {
            Self {
                script: Box::new(script),
                requests: Mutex::new(Vec::new()),
                cancels,
            }
        }
    }

    impl TurnRunner for Scripted {
        fn run(
            &self,
            req: TurnRequest,
            on_event: &mut dyn FnMut(AgentEvent),
        ) -> Result<(), String> {
            self.requests.lock().unwrap().push(req.clone());
            (self.script)(&req, on_event)
        }
        fn cancel(&self, thread_id: &str) {
            self.cancels.lock().unwrap().push(thread_id.to_string());
        }
    }

    #[derive(Debug, PartialEq)]
    enum Out {
        Event(AgentEvent),
        /// (snapshot title, thread trailer, turn trailer), restart the game.
        Finished(Option<(String, Option<String>, Option<u32>)>, bool),
    }

    #[derive(Default)]
    struct Recorder(Mutex<Vec<Out>>);

    impl TurnSink for Recorder {
        fn event(&self, _thread_id: &str, event: &AgentEvent) {
            self.0.lock().unwrap().push(Out::Event(event.clone()));
        }
        fn finished(&self, _thread_id: &str, snapshot: Option<&Snapshot>, restart_game: bool) {
            let snap = snapshot.map(|s| (s.title.clone(), s.thread_id.clone(), s.turn));
            self.0
                .lock()
                .unwrap()
                .push(Out::Finished(snap, restart_game));
        }
    }

    impl Recorder {
        fn take(&self) -> Vec<Out> {
            std::mem::take(&mut *self.0.lock().unwrap())
        }
    }

    fn mcp(project: &Path) -> Result<McpLaunch, String> {
        Ok(McpLaunch {
            command: PathBuf::from("/nonexistent/infinabox"),
            args: vec![MCP_SERVER_FLAG.into()],
            env: vec![(ENV_PROJECT.into(), project.display().to_string())],
        })
    }

    /// `agent_send` without Tauri: start, then run to the end (on this
    /// thread; the command makes the same call on a spawned one).
    fn send(
        runner: &dyn TurnRunner,
        active: &ActiveTurns,
        project: &Path,
        thread_id: &str,
        message: &str,
        sink: &Recorder,
    ) {
        let slot = start_turn(active, project, thread_id, message).unwrap();
        run_turn_to_end(runner, slot, project, message, mcp(project), sink);
    }

    fn completed(is_error: bool) -> AgentEvent {
        AgentEvent::TurnCompleted {
            is_error,
            duration_ms: None,
            usage: None,
        }
    }

    fn other_error(message: &str) -> AgentEvent {
        AgentEvent::Error {
            kind: AgentErrorKind::Other,
            message: message.into(),
        }
    }

    fn load(project: &Path, thread_id: &str) -> (ThreadSummary, Vec<ChatRecord>) {
        chat_store::load_thread(project, thread_id).unwrap()
    }

    /// The events the chat file holds, without timestamps.
    fn saved(project: &Path, thread_id: &str) -> Vec<Out> {
        load(project, thread_id)
            .1
            .into_iter()
            .filter_map(|r| match r {
                ChatRecord::Event { event, .. } => Some(Out::Event(event)),
                ChatRecord::User { .. } => None,
            })
            .collect()
    }

    fn user_texts(project: &Path, thread_id: &str) -> Vec<String> {
        load(project, thread_id)
            .1
            .into_iter()
            .filter_map(|r| match r {
                ChatRecord::User { text, .. } => Some(text),
                ChatRecord::Event { .. } => None,
            })
            .collect()
    }

    fn editing_turn() -> Vec<AgentEvent> {
        vec![
            AgentEvent::SessionStarted {
                provider_session_id: "sess-1".into(),
                model: Some("claude-test".into()),
            },
            AgentEvent::AssistantText {
                text: "Making the player faster.".into(),
            },
            AgentEvent::ToolUse {
                id: "t1".into(),
                name: "Edit".into(),
                summary: "Edit player.gd".into(),
            },
            AgentEvent::ToolResult {
                id: "t1".into(),
                ok: true,
                summary: "Edited".into(),
            },
            AgentEvent::FilesChanged {
                paths: vec!["player.gd".into()],
            },
            completed(false),
        ]
    }

    #[test]
    fn an_editing_turn_is_saved_streamed_and_snapshotted_with_its_chat() {
        let (dir, thread) = project_with_thread("edit");
        let events = editing_turn();
        let script_events = events.clone();
        let runner = Scripted::new(move |req, emit| {
            for event in &script_events {
                if matches!(event, AgentEvent::FilesChanged { .. }) {
                    fs::write(
                        req.project_path.join("player.gd"),
                        "extends Node2D\nvar speed = 2\n",
                    )
                    .unwrap();
                }
                emit(event.clone());
            }
            Ok(())
        });
        let active = ActiveTurns::default();
        let sink = Recorder::default();
        let message = "Make the player faster\nand jump higher too";
        send(&runner, &active, &dir, &thread, message, &sink);

        let mut expected: Vec<Out> = events.iter().cloned().map(Out::Event).collect();
        assert_eq!(saved(&dir, &thread), expected);
        expected.push(Out::Finished(
            Some((
                "Make the player faster".into(),
                Some(thread.clone()),
                Some(1),
            )),
            true,
        ));
        assert_eq!(sink.take(), expected);

        let (header, _) = load(&dir, &thread);
        assert_eq!(header.provider_session_id.as_deref(), Some("sess-1"));
        assert_eq!(user_texts(&dir, &thread), vec![message.to_string()]);
        let req = runner.requests.lock().unwrap()[0].clone();
        assert_eq!(req.resume_provider_session_id, None);
        assert_eq!(req.message, message);
        assert_eq!(req.mcp.args, vec![MCP_SERVER_FLAG.to_string()]);
        // The chat file went into that same snapshot: nothing is left over.
        assert!(snapshot::create_snapshot(&dir, "leftover", None)
            .unwrap()
            .is_none());
        assert!(active.lock().unwrap().is_empty());
        // No stop was asked for, so none was sent.
        assert!(runner.cancels.lock().unwrap().is_empty());

        // Second turn: resumes the saved session, and with no file changes
        // makes no snapshot.
        let runner2 = Scripted::new(|_, emit| {
            emit(AgentEvent::AssistantText {
                text: "It's faster now.".into(),
            });
            emit(completed(false));
            Ok(())
        });
        let before = snapshot::list_snapshots(&dir, 50).unwrap().len();
        send(&runner2, &active, &dir, &thread, "Is it faster?", &sink);
        assert_eq!(
            runner2.requests.lock().unwrap()[0]
                .resume_provider_session_id
                .as_deref(),
            Some("sess-1")
        );
        assert_eq!(
            sink.take(),
            vec![
                Out::Event(AgentEvent::AssistantText {
                    text: "It's faster now.".into()
                }),
                Out::Event(completed(false)),
                Out::Finished(None, false),
            ]
        );
        assert_eq!(snapshot::list_snapshots(&dir, 50).unwrap().len(), before);

        // A third, editing turn is numbered 3 (user messages so far).
        let runner3 = Scripted::new(|req, emit| {
            fs::write(req.project_path.join("enemy.gd"), "extends Node2D\n").unwrap();
            emit(AgentEvent::FilesChanged {
                paths: vec!["enemy.gd".into()],
            });
            emit(completed(false));
            Ok(())
        });
        send(&runner3, &active, &dir, &thread, "Add an enemy", &sink);
        let latest = &snapshot::list_snapshots(&dir, 1).unwrap()[0];
        assert_eq!(latest.title, "Add an enemy");
        assert_eq!(latest.thread_id.as_deref(), Some(thread.as_str()));
        assert_eq!(latest.turn, Some(3));

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_runtime_err_still_ends_the_turn_with_the_real_text() {
        let (dir, thread) = project_with_thread("err");
        let runner = Scripted::new(|_, _| Err("The project folder /x doesn't exist.".into()));
        let sink = Recorder::default();
        send(&runner, &ActiveTurns::default(), &dir, &thread, "hi", &sink);

        let turn = vec![
            Out::Event(other_error("The project folder /x doesn't exist.")),
            Out::Event(completed(true)),
        ];
        assert_eq!(saved(&dir, &thread), turn);
        let mut expected = turn;
        expected.push(Out::Finished(None, false));
        assert_eq!(sink.take(), expected);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_turn_without_turn_completed_gets_one_closing_event() {
        let (dir, thread) = project_with_thread("no-completed");
        let runner = Scripted::new(|_, emit| {
            emit(other_error("Something broke."));
            Ok(())
        });
        let sink = Recorder::default();
        send(&runner, &ActiveTurns::default(), &dir, &thread, "hi", &sink);
        assert_eq!(
            sink.take(),
            vec![
                Out::Event(other_error("Something broke.")),
                Out::Event(completed(true)),
                Out::Finished(None, false),
            ]
        );

        // A runtime that reports nothing at all still ends with a reason.
        let silent = Scripted::new(|_, _| Ok(()));
        send(
            &silent,
            &ActiveTurns::default(),
            &dir,
            &thread,
            "hi again",
            &sink,
        );
        assert_eq!(
            sink.take(),
            vec![
                Out::Event(other_error("The AI stopped without finishing its reply.")),
                Out::Event(completed(true)),
                Out::Finished(None, false),
            ]
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_panicking_runtime_still_ends_the_turn_frees_the_thread_and_saves_edits() {
        let (dir, thread) = project_with_thread("panic");
        let runner = Scripted::new(|req, emit| {
            fs::write(
                req.project_path.join("player.gd"),
                "extends Node2D # edited\n",
            )
            .unwrap();
            emit(AgentEvent::FilesChanged {
                paths: vec!["player.gd".into()],
            });
            panic!("runtime blew up");
        });
        let active = ActiveTurns::default();
        let sink = Recorder::default();
        send(&runner, &active, &dir, &thread, "Break things", &sink);

        assert_eq!(
            sink.take(),
            vec![
                Out::Event(AgentEvent::FilesChanged {
                    paths: vec!["player.gd".into()]
                }),
                Out::Event(other_error(
                    "InfinaBox hit an internal error while the AI was working: runtime blew up"
                )),
                Out::Event(completed(true)),
                Out::Finished(
                    Some(("Break things".into(), Some(thread.clone()), Some(1))),
                    true
                ),
            ]
        );
        assert!(active.lock().unwrap().is_empty());
        // The thread takes its next message.
        start_turn(&active, &dir, &thread, "again").unwrap();
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_second_concurrent_turn_on_a_thread_is_refused_without_saving_it() {
        let (dir, thread) = project_with_thread("concurrent");
        let active = ActiveTurns::default();
        let slot = start_turn(&active, &dir, &thread, "first").unwrap();
        let err = start_turn(&active, &dir, &thread, "second").err().unwrap();
        assert!(err.contains("still working"), "{err}");
        assert_eq!(user_texts(&dir, &thread), vec!["first".to_string()]);

        // Another thread in the same project isn't blocked.
        let other = chat_store::create_thread(&dir, "Other", PROVIDER).unwrap();
        drop(start_turn(&active, &dir, &other.id, "elsewhere").unwrap());

        drop(slot);
        start_turn(&active, &dir, &thread, "third").unwrap();
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn start_refuses_bad_requests_and_frees_the_thread() {
        let (dir, thread) = project_with_thread("bad-request");
        let active = ActiveTurns::default();
        assert!(start_turn(&active, &dir, &thread, "   ").is_err());
        assert!(start_turn(&active, &dir.join("missing"), &thread, "hi").is_err());
        assert!(start_turn(&active, Path::new("relative/project"), &thread, "hi").is_err());
        // A thread that doesn't exist: the save fails, nothing stays claimed.
        let err = start_turn(&active, &dir, "20260101T000000000-deadbeef", "hi")
            .err()
            .unwrap();
        assert!(err.contains("couldn't be saved"), "{err}");
        assert!(active.lock().unwrap().is_empty());
        assert!(user_texts(&dir, &thread).is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_stop_before_the_runtime_starts_ends_the_turn_without_running_it() {
        let (dir, thread) = project_with_thread("early-stop");
        let runner = Scripted::new(|_, _| panic!("must not run"));
        let active = ActiveTurns::default();
        let sink = Recorder::default();
        let slot = start_turn(&active, &dir, &thread, "hi").unwrap();
        cancel_turn(&runner, &active, &thread);
        run_turn_to_end(&runner, slot, &dir, "hi", mcp(&dir), &sink);
        assert!(runner.requests.lock().unwrap().is_empty());
        assert_eq!(
            sink.take(),
            vec![
                Out::Event(other_error(STOPPED_MESSAGE)),
                Out::Event(completed(true)),
                Out::Finished(None, false),
            ]
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_stop_the_runtime_missed_is_passed_on_at_the_first_event() {
        let (dir, thread) = project_with_thread("late-stop");
        let active = ActiveTurns::default();
        let flags = active.clone();
        let cancels = Arc::<Mutex<Vec<String>>>::default();
        let seen = cancels.clone();
        let runner = Scripted::with_cancels(cancels, move |req, emit| {
            // Stop pressed while the runtime was still starting up.
            lock(&flags)[&req.thread_id].store(true, Ordering::SeqCst);
            emit(AgentEvent::AssistantText {
                text: "Sure".into(),
            });
            // By the time the first event has been handled, it's been sent
            // (the re-send watcher may have sent it as well).
            assert!(!seen.lock().unwrap().is_empty());
            emit(other_error(STOPPED_MESSAGE));
            emit(completed(true));
            Ok(())
        });
        let sink = Recorder::default();
        send(&runner, &active, &dir, &thread, "hi", &sink);
        let cancels = runner.cancels.lock().unwrap().clone();
        assert!(!cancels.is_empty() && cancels.iter().all(|t| t == &thread));
        assert_eq!(sink.take().last(), Some(&Out::Finished(None, false)));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_stop_the_runtime_missed_is_resent_until_it_emits_something() {
        let (dir, thread) = project_with_thread("resend-stop");
        let active = ActiveTurns::default();
        let flags = active.clone();
        let cancels = Arc::<Mutex<Vec<String>>>::default();
        let seen = cancels.clone();
        let runner = Scripted::with_cancels(cancels, move |req, emit| {
            // Stop pressed while the runtime is still finding `claude`, and
            // it stays silent for a while: the stop keeps being re-sent.
            lock(&flags)[&req.thread_id].store(true, Ordering::SeqCst);
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            while seen.lock().unwrap().len() < 3 {
                assert!(
                    std::time::Instant::now() < deadline,
                    "the stop was sent only {} time(s)",
                    seen.lock().unwrap().len()
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            emit(other_error(STOPPED_MESSAGE));
            emit(completed(true));
            Ok(())
        });
        let sink = Recorder::default();
        send(&runner, &active, &dir, &thread, "hi", &sink);
        let cancels = runner.cancels.lock().unwrap().clone();
        assert!(cancels.len() >= 3, "{cancels:?}");
        assert!(cancels.iter().all(|t| t == &thread));
        assert_eq!(
            sink.take(),
            vec![
                Out::Event(other_error(STOPPED_MESSAGE)),
                Out::Event(completed(true)),
                Out::Finished(None, false),
            ]
        );
        // The watcher ended with the turn: nothing more is sent.
        std::thread::sleep(STOP_RESEND_INTERVAL * 2);
        assert_eq!(runner.cancels.lock().unwrap().len(), cancels.len());
        fs::remove_dir_all(&dir).unwrap();
    }

    /// A tool use and its successful result.
    fn tool(id: &str, name: &str) -> [AgentEvent; 2] {
        [
            AgentEvent::ToolUse {
                id: id.into(),
                name: name.into(),
                summary: name.into(),
            },
            AgentEvent::ToolResult {
                id: id.into(),
                ok: true,
                summary: "Done".into(),
            },
        ]
    }

    /// Runs one editing turn made of `tools`, then `FilesChanged` and
    /// `TurnCompleted`; returns whether the game would be restarted.
    fn restart_after(tools: Vec<AgentEvent>) -> bool {
        let (dir, thread) = project_with_thread("restart");
        let runner = Scripted::new(move |req, emit| {
            fs::write(req.project_path.join("player.gd"), "extends Node2D # v2\n").unwrap();
            for event in &tools {
                emit(event.clone());
            }
            emit(AgentEvent::FilesChanged {
                paths: vec!["player.gd".into()],
            });
            emit(completed(false));
            Ok(())
        });
        let sink = Recorder::default();
        send(&runner, &ActiveTurns::default(), &dir, &thread, "Edit", &sink);
        let out = sink.take();
        fs::remove_dir_all(&dir).unwrap();
        match out.last() {
            Some(Out::Finished(Some(_), restart)) => *restart,
            other => panic!("expected a snapshotted finish, got {other:?}"),
        }
    }

    #[test]
    fn the_game_is_not_restarted_again_when_the_agent_ran_it_after_its_last_edit() {
        // The Director flow: edit, run the game, check it, note it down.
        let flow = [
            tool("t1", "Edit"),
            tool("t2", RUN_GAME_TOOL),
            tool("t3", "mcp__infinabox__get_game_errors"),
            tool("t4", "Read"),
            tool("t5", "mcp__infinabox__write_context_card"),
        ]
        .concat();
        assert!(!restart_after(flow));
    }

    #[test]
    fn the_game_is_restarted_when_the_agent_did_not_run_it_after_its_last_edit() {
        // Never ran it.
        assert!(restart_after(tool("t1", "Edit").to_vec()));
        // Ran it, then changed something again (by editing, or any tool
        // that might write files, like a shell command).
        for later in ["Write", "Bash", "mcp__other__tool"] {
            let flow = [tool("t1", "Edit"), tool("t2", RUN_GAME_TOOL), tool("t3", later)].concat();
            assert!(restart_after(flow), "{later}");
        }
        // The run failed.
        let mut flow = [tool("t1", "Edit"), tool("t2", RUN_GAME_TOOL)].concat();
        if let AgentEvent::ToolResult { ok, .. } = &mut flow[3] {
            *ok = false;
        }
        assert!(restart_after(flow));
        // The run began while an edit was still going.
        let [edit_use, edit_result] = tool("t1", "Edit");
        let [run_use, run_result] = tool("t2", RUN_GAME_TOOL);
        assert!(restart_after(vec![edit_use, run_use, edit_result, run_result]));
    }

    #[test]
    fn a_failed_snapshot_is_reported_before_the_turn_finishes() {
        // A project inside another git repository: snapshots refuse it.
        let outer = temp_dir("snap-fail");
        fs::write(outer.join("readme.txt"), "outer\n").unwrap();
        snapshot::create_snapshot(&outer, "Outer", None).unwrap();
        let dir = outer.join("game");
        fs::create_dir_all(&dir).unwrap();
        let thread = chat_store::create_thread(&dir, "Chat", PROVIDER)
            .unwrap()
            .id;

        let runner = Scripted::new(|req, emit| {
            fs::write(req.project_path.join("player.gd"), "extends Node2D\n").unwrap();
            emit(AgentEvent::FilesChanged {
                paths: vec!["player.gd".into()],
            });
            emit(completed(false));
            Ok(())
        });
        let sink = Recorder::default();
        send(
            &runner,
            &ActiveTurns::default(),
            &dir,
            &thread,
            "Add a player",
            &sink,
        );

        let out = sink.take();
        assert_eq!(out.len(), 4, "{out:?}");
        let prefix = "This change couldn't be saved to your history: ";
        match &out[2] {
            Out::Event(AgentEvent::Error {
                kind: AgentErrorKind::Other,
                message,
            }) => assert!(
                message.starts_with(prefix) && message.len() > prefix.len(),
                "{message}"
            ),
            other => panic!("expected the snapshot error, got {other:?}"),
        }
        assert_eq!(out[3], Out::Finished(None, true));
        // The error is in the chat file too.
        assert_eq!(saved(&dir, &thread).len(), 3);
        fs::remove_dir_all(&outer).unwrap();
    }

    /// Runs the real `ClaudeCodeRuntime` against a stand-in `claude` that
    /// replays the recorded bad-`--resume` output (fixture `e_bad_resume`).
    #[cfg(unix)]
    #[test]
    fn a_dead_resume_id_is_cleared_so_the_next_turn_starts_fresh() {
        use std::os::unix::fs::PermissionsExt;

        let (dir, thread) = project_with_thread("bad-resume");
        chat_store::set_provider_session(
            &dir,
            &thread,
            Some("00000000-0000-0000-0000-000000000000"),
        )
        .unwrap();

        let fixtures =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../crates/core/tests/fixtures/claude");
        let bin = temp_dir("bad-resume-bin");
        let script = bin.join("claude");
        fs::write(
            &script,
            format!(
                "#!/bin/sh\ncat '{}'\ncat '{}' >&2\nexit \"$(cat '{}')\"\n",
                fixtures.join("e_bad_resume.jsonl").display(),
                fixtures.join("e_bad_resume.stderr.txt").display(),
                fixtures.join("e_bad_resume.exit.txt").display(),
            ),
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let runtime = ClaudeCodeRuntime::with_program(script.display().to_string());

        let sink = Recorder::default();
        let active = ActiveTurns::default();
        send(&runtime, &active, &dir, &thread, "hi", &sink);
        let out = sink.take();
        assert!(
            matches!(&out[0], Out::Event(AgentEvent::Error { message, .. })
                if message.starts_with(BAD_RESUME_MARKER)),
            "{out:?}"
        );
        assert!(
            matches!(
                out[1],
                Out::Event(AgentEvent::TurnCompleted { is_error: true, .. })
            ),
            "{out:?}"
        );
        assert_eq!(out[2..], [Out::Finished(None, false)]);
        assert_eq!(load(&dir, &thread).0.provider_session_id, None);

        // The next turn doesn't try to resume it.
        let runner = Scripted::new(|_, emit| {
            emit(completed(false));
            Ok(())
        });
        send(&runner, &active, &dir, &thread, "hi again", &sink);
        assert_eq!(
            runner.requests.lock().unwrap()[0].resume_provider_session_id,
            None
        );

        fs::remove_dir_all(&dir).unwrap();
        fs::remove_dir_all(&bin).unwrap();
    }

    /// Two real turns through the real Claude Code CLI: the first edits a
    /// file (and is snapshotted with its chat), the second resumes it.
    #[test]
    #[ignore = "needs a signed-in claude CLI; run with --ignored"]
    fn real_claude_turns_are_saved_snapshotted_and_resumed() {
        let (dir, thread) = project_with_thread("real");
        let runtime = ClaudeCodeRuntime::new();
        let active = ActiveTurns::default();
        let sink = Recorder::default();
        // No real MCP server in a test binary; the turn doesn't need one.
        let no_mcp = |project: &Path| McpLaunch {
            command: PathBuf::from("/bin/false"),
            args: vec![],
            env: vec![(ENV_PROJECT.into(), project.display().to_string())],
        };

        let message = "Create a file named hello.txt in the project folder containing exactly the word hi. Do nothing else.";
        let slot = start_turn(&active, &dir, &thread, message).unwrap();
        run_turn_to_end(&runtime, slot, &dir, message, Ok(no_mcp(&dir)), &sink);
        let out = sink.take();
        eprintln!("turn 1: {out:#?}");
        assert!(out.iter().any(|o| matches!(
            o,
            Out::Event(AgentEvent::TurnCompleted {
                is_error: false,
                ..
            })
        )));
        match out.last() {
            Some(Out::Finished(Some((title, Some(t), Some(1))), true)) => {
                assert_eq!(title, &snapshot_title(message));
                assert_eq!(t, &thread);
            }
            other => panic!("expected a snapshot, got {other:?}"),
        }
        assert_eq!(
            fs::read_to_string(dir.join("hello.txt")).unwrap().trim(),
            "hi"
        );
        let session = load(&dir, &thread)
            .0
            .provider_session_id
            .expect("session saved");
        assert!(snapshot::create_snapshot(&dir, "leftover", None)
            .unwrap()
            .is_none());
        eprintln!(
            "chat file:\n{}",
            fs::read_to_string(dir.join(".ibproject/chat").join(format!("{thread}.jsonl")))
                .unwrap()
        );

        let message = "What is the exact content of hello.txt? Reply with just the content.";
        let slot = start_turn(&active, &dir, &thread, message).unwrap();
        run_turn_to_end(&runtime, slot, &dir, message, Ok(no_mcp(&dir)), &sink);
        let out = sink.take();
        eprintln!("turn 2: {out:#?}");
        assert!(out.iter().any(|o| matches!(o,
            Out::Event(AgentEvent::SessionStarted { provider_session_id, .. }) if provider_session_id == &session)));
        assert_eq!(out.last(), Some(&Out::Finished(None, false)));

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn snapshot_titles_are_the_first_line_cut_to_about_sixty_chars() {
        assert_eq!(snapshot_title("Make it blue"), "Make it blue");
        assert_eq!(
            snapshot_title("\n  Add a boss  \nwith two phases"),
            "Add a boss"
        );
        assert_eq!(snapshot_title("   \n"), "AI change");
        let long =
            "Make the player jump higher and add a double jump with a little dust puff effect";
        let title = snapshot_title(long);
        assert!(title.chars().count() <= TITLE_MAX_CHARS, "{title}");
        assert_eq!(
            title,
            "Make the player jump higher and add a double jump with a…"
        );
        let unbroken = "é".repeat(100);
        assert_eq!(snapshot_title(&unbroken).chars().count(), TITLE_MAX_CHARS);
    }
}
