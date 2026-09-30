//! "Connect your AI" commands (spec §8.2): detection, the recommended
//! provider, a real "say hello" test turn, and running a CLI's installer or
//! sign-in inside a visible terminal (streamed as `connect-output`, ended
//! with `connect-exit`).
//!
//! Split like `terminal.rs`: `ConnectSession` (the PTY process, its output
//! and its exit) and `test_connection` (the test turn) take no `AppHandle`,
//! so they're unit-tested against real processes; the `#[tauri::command]`s
//! at the bottom are thin wrappers that plug in Tauri events and state.
//!
//! Output that arrives before the frontend is listening would be lost (its
//! `listen` resolves a moment after it calls `connect_run`), so a session's
//! output is held until the frontend's first `connect_resize` — which
//! `ConnectTerminal` sends as soon as its terminal is laid out, after
//! subscribing — or `HOLD_OUTPUT_FOR` after the start, whichever comes
//! first. Held output is then emitted once, in order, before anything
//! newer: nothing is lost and nothing is shown twice.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use infinabox_core::agent::claude::ClaudeCodeRuntime;
use infinabox_core::agent::codex::CodexRuntime;
use infinabox_core::agent::path::refresh_login_shell_path;
use infinabox_core::agent::{
    AgentErrorKind, AgentEvent, AgentRuntime, McpLaunch, TurnOptions, TurnRequest,
};
use infinabox_core::connect::{self, ProviderId, ProviderInfo};
use infinabox_mcp_server::{ENV_PROJECT, MCP_SERVER_FLAG};
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

/// Event names (frontend: `src/lib/studio-api.ts`).
pub const EVENT_CONNECT_OUTPUT: &str = "connect-output";
pub const EVENT_CONNECT_EXIT: &str = "connect-exit";

pub const ALREADY_RUNNING: &str =
    "Another install or sign-in is still running. Wait for it to finish, or cancel it first.";
pub const NOT_RUNNING: &str = "No install or sign-in is running.";

/// The test turn's message, and how long it gets before it's stopped.
pub const TEST_MESSAGE: &str = "Reply with exactly: hello";
pub const TEST_TIMEOUT: Duration = Duration::from_secs(90);
const TEST_THREAD_ID: &str = "connect-test";

/// Longest a session's output is held waiting for the frontend (see the
/// module docs).
const HOLD_OUTPUT_FOR: Duration = Duration::from_secs(1);
/// Held output past this is trimmed from the front (an installer's
/// progress bar can print a lot before anyone listens).
const MAX_HELD_BYTES: usize = 256 * 1024;
/// After the process exits, how long its PTY output gets to drain. A
/// background process it left behind can keep the PTY open indefinitely.
const DRAIN_GRACE: Duration = Duration::from_millis(500);

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectAction {
    Install,
    Login,
}

#[derive(Serialize, Clone, Debug)]
pub struct ConnectOutputPayload {
    pub data: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct ConnectExitPayload {
    pub action: ConnectAction,
    pub provider: ProviderId,
    pub success: bool,
    pub code: Option<i32>,
}

/// The result of a real one-line test turn.
#[derive(Serialize, Clone, Debug)]
pub struct ConnectionTest {
    pub ok: bool,
    pub reply: Option<String>,
    pub error_kind: Option<AgentErrorKind>,
    pub message: Option<String>,
    pub duration_ms: u64,
}

// --- The test turn ---

/// What `test_connection` needs from a runtime: run one turn, and stop it.
/// Separate from `AgentRuntime` so tests can script a turn without naming
/// `anyhow` (not a dependency of this crate); `RuntimeTurn` adapts a real
/// runtime.
pub trait TestTurn: Sync {
    fn run(&self, req: TurnRequest, on_event: &mut dyn FnMut(AgentEvent)) -> Result<(), String>;
    fn cancel(&self, thread_id: &str);
}

pub struct RuntimeTurn<'a>(pub &'a dyn AgentRuntime);

impl TestTurn for RuntimeTurn<'_> {
    fn run(&self, req: TurnRequest, on_event: &mut dyn FnMut(AgentEvent)) -> Result<(), String> {
        self.0.run_turn(req, on_event).map_err(|e| format!("{e:#}"))
    }

    fn cancel(&self, thread_id: &str) {
        self.0.cancel(thread_id);
    }
}

/// Runs one real turn (`TEST_MESSAGE`) through `runtime` in a fresh, empty
/// temp folder, and reports what really came back. The turn is cancelled
/// if it hasn't ended after `timeout`. The folder is removed afterwards.
///
/// Codex doesn't need the folder to be a git repo (`CodexRuntime` passes
/// `--skip-git-repo-check`), so it's left a plain folder.
pub fn test_connection(
    runtime: &dyn TestTurn,
    mcp: impl FnOnce(&Path) -> McpLaunch,
    timeout: Duration,
) -> ConnectionTest {
    let started = Instant::now();
    let folder = match TempFolder::new("connect-test") {
        Ok(folder) => folder,
        Err(e) => {
            return ConnectionTest {
                ok: false,
                reply: None,
                error_kind: Some(AgentErrorKind::Other),
                message: Some(format!(
                    "Couldn't make a temporary folder for the test: {e}"
                )),
                duration_ms: 0,
            };
        }
    };
    let req = TurnRequest {
        thread_id: TEST_THREAD_ID.into(),
        project_path: folder.path().to_path_buf(),
        message: TEST_MESSAGE.into(),
        resume_provider_session_id: None,
        mcp: mcp(folder.path()),
        options: TurnOptions::default(),
    };

    let mut reply = String::new();
    let mut error: Option<(AgentErrorKind, String)> = None;
    let mut completed = false;
    let timed_out = AtomicBool::new(false);
    let (done_tx, done_rx) = mpsc::channel::<()>();
    let result = thread::scope(|s| {
        // Watchdog: stops the turn if it runs past `timeout`.
        let timed_out = &timed_out;
        s.spawn(move || {
            if let Err(mpsc::RecvTimeoutError::Timeout) = done_rx.recv_timeout(timeout) {
                timed_out.store(true, Ordering::SeqCst);
                runtime.cancel(TEST_THREAD_ID);
            }
        });
        let result = runtime.run(req, &mut |event| match event {
            AgentEvent::AssistantText { text } => {
                if !reply.is_empty() {
                    reply.push('\n');
                }
                reply.push_str(&text);
            }
            AgentEvent::Error { kind, message } => {
                error.get_or_insert((kind, message));
            }
            AgentEvent::TurnCompleted { is_error, .. } => completed = !is_error,
            _ => {}
        });
        let _ = done_tx.send(());
        result
    });
    let duration_ms = started.elapsed().as_millis() as u64;
    drop(folder);

    if let Err(e) = result {
        error.get_or_insert((AgentErrorKind::Other, e));
    }
    if timed_out.load(Ordering::SeqCst) {
        // The runtime reports this as the person pressing Stop; say what
        // really happened instead.
        error = Some((
            AgentErrorKind::Other,
            format!(
                "The AI didn't answer within {} seconds, so the test was stopped.",
                timeout.as_secs()
            ),
        ));
    }
    let reply = reply.trim().to_string();
    let ok = completed && error.is_none() && !reply.is_empty();
    let (error_kind, message) = match error {
        Some((kind, message)) => (Some(kind), Some(message)),
        None if !ok => (
            Some(AgentErrorKind::Other),
            Some("The AI finished without replying.".to_string()),
        ),
        None => (None, None),
    };
    ConnectionTest {
        ok,
        reply: (!reply.is_empty()).then_some(reply),
        error_kind,
        message,
        duration_ms,
    }
}

/// A uniquely named temp folder, removed on drop.
struct TempFolder(PathBuf);

impl TempFolder {
    fn new(tag: &str) -> std::io::Result<Self> {
        static N: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let path = std::env::temp_dir().join(format!(
            "infinabox-{tag}-{}-{nanos}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        // `create_dir`, not `create_dir_all`: fails rather than reusing
        // something that's already there.
        std::fs::create_dir(&path)?;
        Ok(TempFolder(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempFolder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// --- The connect terminal ---

/// How a connect process ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConnectExit {
    pub success: bool,
    /// The exit code; `None` when it was cancelled or killed by a signal.
    pub code: Option<i32>,
}

/// Decodes PTY bytes as UTF-8, carrying a character split across two reads
/// over to the next one (`tail`) instead of turning it into `�`. Truly
/// invalid bytes become `�`.
fn decode_utf8(tail: &mut Vec<u8>, chunk: &[u8]) -> String {
    let mut bytes = std::mem::take(tail);
    bytes.extend_from_slice(chunk);
    let mut out = String::with_capacity(bytes.len());
    let mut rest = &bytes[..];
    loop {
        match std::str::from_utf8(rest) {
            Ok(s) => {
                out.push_str(s);
                break;
            }
            Err(e) => {
                let valid = e.valid_up_to();
                // Checked by `from_utf8` just now.
                out.push_str(std::str::from_utf8(&rest[..valid]).unwrap_or_default());
                match e.error_len() {
                    Some(n) => {
                        out.push('\u{FFFD}');
                        rest = &rest[valid + n..];
                    }
                    None => {
                        *tail = rest[valid..].to_vec();
                        break;
                    }
                }
            }
        }
    }
    out
}

type OutputFn = Box<dyn Fn(String) + Send + Sync>;

/// Orders and (until opened) holds a session's output; see the module docs.
struct OutputGate {
    state: Mutex<GateState>,
    emit: OutputFn,
}

struct GateState {
    open: bool,
    held: String,
    /// The start of a UTF-8 character still waiting for its other bytes.
    tail: Vec<u8>,
}

impl OutputGate {
    fn new(emit: OutputFn) -> Self {
        OutputGate {
            state: Mutex::new(GateState {
                open: false,
                held: String::new(),
                tail: Vec::new(),
            }),
            emit,
        }
    }

    fn state(&self) -> MutexGuard<'_, GateState> {
        lock(&self.state)
    }

    /// Emitting under the lock keeps chunks in order across the reader
    /// thread and whoever opens the gate.
    fn push(&self, chunk: &[u8]) {
        let mut state = self.state();
        let text = decode_utf8(&mut state.tail, chunk);
        if text.is_empty() {
            return;
        }
        if state.open {
            (self.emit)(text);
            return;
        }
        state.held.push_str(&text);
        if state.held.len() > MAX_HELD_BYTES {
            let mut cut = state.held.len() - MAX_HELD_BYTES;
            while !state.held.is_char_boundary(cut) {
                cut += 1;
            }
            state.held.drain(..cut);
        }
    }

    /// Emits everything held, then lets output through as it arrives.
    fn open(&self) {
        let mut state = self.state();
        if state.open {
            return;
        }
        state.open = true;
        let held = std::mem::take(&mut state.held);
        if !held.is_empty() {
            (self.emit)(held);
        }
    }

    /// The process is done: flush a dangling partial character too.
    fn finish(&self) {
        self.open();
        let mut state = self.state();
        if !state.tail.is_empty() {
            let tail = std::mem::take(&mut state.tail);
            (self.emit)(String::from_utf8_lossy(&tail).into_owned());
        }
    }
}

/// One installer or sign-in running in its own PTY. The process is started
/// directly (not through a shell); it's the leader of its own session (and
/// so its own process group), which `cancel` kills as a whole.
pub struct ConnectSession {
    writer: Mutex<Box<dyn Write + Send>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    pid: Option<u32>,
    gate: Arc<OutputGate>,
    cancelled: Arc<AtomicBool>,
}

impl ConnectSession {
    /// Starts `program args` in a `cols`x`rows` PTY, in the home folder.
    /// `on_output` gets its output as text, in order (held until
    /// `output_ready` or `HOLD_OUTPUT_FOR`); `on_exit` is called once, on a
    /// background thread, after the process has exited and its output has
    /// been delivered.
    pub fn spawn(
        program: &str,
        args: &[String],
        rows: u16,
        cols: u16,
        on_output: impl Fn(String) + Send + Sync + 'static,
        on_exit: impl FnOnce(ConnectExit) + Send + 'static,
    ) -> std::io::Result<Self> {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(to_io_error)?;
        let mut cmd = CommandBuilder::new(program);
        cmd.args(args);
        // Installers print progress bars and colours only for a terminal
        // that says what it is; the frontend's xterm is xterm-compatible.
        cmd.env("TERM", "xterm-256color");
        if let Some(home) = home_dir().filter(|h| h.is_dir()) {
            cmd.cwd(home);
        }
        let mut child = pair.slave.spawn_command(cmd).map_err(to_io_error)?;
        drop(pair.slave);

        let pid = child.process_id();
        let killer = child.clone_killer();
        let writer = pair.master.take_writer().map_err(to_io_error)?;
        let mut reader = pair.master.try_clone_reader().map_err(to_io_error)?;
        let gate = Arc::new(OutputGate::new(Box::new(on_output)));
        let cancelled = Arc::new(AtomicBool::new(false));

        let (drained_tx, drained_rx) = mpsc::channel::<()>();
        let reader_gate = gate.clone();
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    // EOF, or EIO once the process side of the PTY closed.
                    Ok(0) | Err(_) => break,
                    Ok(n) => reader_gate.push(&buf[..n]),
                }
            }
            let _ = drained_tx.send(());
        });

        let hold_gate = Arc::downgrade(&gate);
        thread::spawn(move || {
            thread::sleep(HOLD_OUTPUT_FOR);
            if let Some(gate) = hold_gate.upgrade() {
                gate.open();
            }
        });

        let exit_gate = gate.clone();
        let exit_cancelled = cancelled.clone();
        thread::spawn(move || {
            let status = child.wait();
            let _ = drained_rx.recv_timeout(DRAIN_GRACE);
            exit_gate.finish();
            let cancelled = exit_cancelled.load(Ordering::SeqCst);
            let exit = match status {
                Ok(status) => ConnectExit {
                    success: status.success() && !cancelled,
                    // portable-pty reports a signal death as code 1; only
                    // its `Display` tells the two apart.
                    code: (!cancelled && !status.to_string().starts_with("Terminated by"))
                        .then(|| status.exit_code() as i32),
                },
                Err(_) => ConnectExit {
                    success: false,
                    code: None,
                },
            };
            on_exit(exit);
        });

        Ok(ConnectSession {
            writer: Mutex::new(writer),
            master: Mutex::new(pair.master),
            killer: Mutex::new(killer),
            pid,
            gate,
            cancelled,
        })
    }

    pub fn write(&self, data: &[u8]) -> std::io::Result<()> {
        let mut writer = lock(&self.writer);
        writer.write_all(data)?;
        writer.flush()
    }

    pub fn resize(&self, rows: u16, cols: u16) -> std::io::Result<()> {
        lock(&self.master)
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(to_io_error)
    }

    /// The frontend is listening: send the held output and stream the rest.
    pub fn output_ready(&self) {
        self.gate.open();
    }

    /// Kills the process and everything it started (its whole process
    /// group). Its exit is still reported, as unsuccessful.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        #[cfg(unix)]
        if let Some(pid) = self.pid {
            // The child is its session's leader (portable-pty calls
            // `setsid`), so its group id is its pid. src-tauri has no libc
            // binding; `kill` is POSIX and on every Mac and Linux.
            let _ = std::process::Command::new("kill")
                .args(["-KILL", "--", &format!("-{pid}")])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
        // SIGHUP on Unix (in case the group kill failed); TerminateProcess
        // on Windows.
        let _ = lock(&self.killer).kill();
    }
}

fn to_io_error<E: std::fmt::Display>(e: E) -> std::io::Error {
    std::io::Error::other(e.to_string())
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
}

fn lock<T: ?Sized>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// Tauri-managed state: at most one connect process at a time.
#[derive(Default)]
pub struct ConnectState(Mutex<Option<RunningConnect>>);

struct RunningConnect {
    id: u64,
    session: Arc<ConnectSession>,
}

impl ConnectState {
    fn current(&self) -> Result<Arc<ConnectSession>, String> {
        lock(&self.0)
            .as_ref()
            .map(|r| r.session.clone())
            .ok_or_else(|| NOT_RUNNING.to_string())
    }
}

// --- Commands ---

#[tauri::command(async)]
pub fn ai_providers() -> Result<Vec<ProviderInfo>, String> {
    Ok(connect::detect_all())
}

#[tauri::command(async)]
pub fn ai_recommended() -> Result<ProviderId, String> {
    Ok(connect::recommend(&connect::detect_all()))
}

/// When the CLI's own status command already says it's signed out, the
/// test turn isn't run: that's the answer, and a signed-out CLI can spend
/// the whole `TEST_TIMEOUT` retrying (seen with Codex 0.157.1).
pub fn signed_out_result(info: &ProviderInfo, duration_ms: u64) -> Option<ConnectionTest> {
    (info.installed && info.logged_in == Some(false)).then(|| ConnectionTest {
        ok: false,
        reply: None,
        error_kind: Some(AgentErrorKind::NotAuthenticated),
        message: Some(format!(
            "{} says it isn't signed in yet. Sign in first, then test again.",
            info.name
        )),
        duration_ms,
    })
}

/// A real one-line turn with the provider's own CLI; takes as long as the
/// AI does (up to `TEST_TIMEOUT`).
#[tauri::command(async)]
pub fn ai_test_connection(app: AppHandle, provider: ProviderId) -> Result<ConnectionTest, String> {
    let started = Instant::now();
    // The API providers have no CLI to be signed in to; a missing key or
    // model shows up as the turn's own plain error below.
    if !provider.is_api() {
        if let Some(result) = signed_out_result(
            &connect::detect(provider),
            started.elapsed().as_millis() as u64,
        ) {
            return Ok(result);
        }
    }
    let command = std::env::current_exe().map_err(|e| {
        format!("InfinaBox couldn't find its own program file to give the AI its tools: {e}")
    })?;
    // The test folder isn't a project and no game runs, so the server gets
    // no bridge: its tools just have nothing to act on.
    let mcp = |folder: &Path| McpLaunch {
        command,
        args: vec![MCP_SERVER_FLAG.to_string()],
        env: vec![(ENV_PROJECT.to_string(), folder.display().to_string())],
    };
    let runtime: Box<dyn AgentRuntime> = match provider {
        ProviderId::ClaudeCode => Box::new(ClaudeCodeRuntime::new()),
        ProviderId::Codex => Box::new(CodexRuntime::new()),
        api => {
            let dir = super::settings::settings_dir(&app)?;
            let settings = super::settings::load_from(&dir)?;
            let config = settings.models.get(api.as_str()).cloned().unwrap_or_default();
            let secrets = app.state::<super::credentials::SecretState>();
            let backend =
                match infinabox_core::agent::api::backend_for(api, &config, secrets.0.as_ref(), None) {
                    Ok(backend) => backend,
                    // Not set up yet: reported as the test's result, not a crash.
                    Err(message) => {
                        return Ok(ConnectionTest {
                            ok: false,
                            reply: None,
                            error_kind: Some(AgentErrorKind::NotAuthenticated),
                            message: Some(message),
                            duration_ms: started.elapsed().as_millis() as u64,
                        });
                    }
                };
            Box::new(infinabox_core::agent::api::ApiRuntime::new(backend))
        }
    };
    Ok(test_connection(
        &RuntimeTurn(runtime.as_ref()),
        mcp,
        TEST_TIMEOUT,
    ))
}

/// Starts the provider's installer or sign-in in a PTY. Output streams as
/// `connect-output`; the end is one `connect-exit` (after the login-shell
/// `PATH` is re-read, so detecting again finds a fresh install).
#[tauri::command(async)]
pub fn connect_run(
    app: AppHandle,
    provider: ProviderId,
    action: ConnectAction,
    rows: u16,
    cols: u16,
) -> Result<(), String> {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    let state = app.state::<ConnectState>();
    let mut running = lock(&state.0);
    if running.is_some() {
        return Err(ALREADY_RUNNING.into());
    }
    let (program, args) = match action {
        ConnectAction::Install => connect::install_invocation(provider),
        ConnectAction::Login => connect::login_invocation(provider),
    }
    .map_err(|e| format!("{e:#}"))?;

    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let output_app = app.clone();
    let exit_app = app.clone();
    let session = ConnectSession::spawn(
        &program,
        &args,
        rows.max(1),
        cols.max(1),
        move |data| {
            let _ = output_app.emit(EVENT_CONNECT_OUTPUT, ConnectOutputPayload { data });
        },
        move |exit| {
            // A new install may have added a folder to the shell profile.
            refresh_login_shell_path();
            let success = run_succeeded(action, exit.success, || connect::detect(provider).installed);
            {
                let state = exit_app.state::<ConnectState>();
                let mut running = lock(&state.0);
                if running.as_ref().is_some_and(|r| r.id == id) {
                    *running = None;
                }
            }
            let _ = exit_app.emit(
                EVENT_CONNECT_EXIT,
                ConnectExitPayload {
                    action,
                    provider,
                    success,
                    code: exit.code,
                },
            );
        },
    )
    .map_err(|e| format!("Couldn't start {program}: {e}"))?;
    *running = Some(RunningConnect {
        id,
        session: Arc::new(session),
    });
    Ok(())
}

/// Whether an install or sign-in worked. `curl … | sh` exits 0 even when
/// the download fails (a pipe's status is its last command's), so an
/// install only counts once the CLI is really there (`installed_now`, only
/// asked after a clean exit). The command itself runs exactly as it was
/// shown to the person.
fn run_succeeded(action: ConnectAction, exited_ok: bool, installed_now: impl FnOnce() -> bool) -> bool {
    exited_ok && (action != ConnectAction::Install || installed_now())
}

#[tauri::command(async)]
pub fn connect_write(app: AppHandle, data: String) -> Result<(), String> {
    let session = app.state::<ConnectState>().current()?;
    session
        .write(data.as_bytes())
        .map_err(|e| format!("Couldn't send that to the terminal: {e}"))
}

/// Resizes the PTY. The first call also releases the output held since the
/// start (see the module docs).
#[tauri::command(async)]
pub fn connect_resize(app: AppHandle, rows: u16, cols: u16) -> Result<(), String> {
    let session = app.state::<ConnectState>().current()?;
    let resized = session
        .resize(rows.max(1), cols.max(1))
        .map_err(|e| format!("Couldn't resize the terminal: {e}"));
    session.output_ready();
    resized
}

/// Stops the running install or sign-in; `connect-exit` still follows, with
/// `success: false`. A no-op when nothing runs.
#[tauri::command(async)]
pub fn connect_cancel(app: AppHandle) -> Result<(), String> {
    if let Ok(session) = app.state::<ConnectState>().current() {
        session.cancel();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Collects a session's output and its exit.
    #[derive(Default)]
    struct Sink {
        output: Mutex<Vec<String>>,
        exit: Mutex<Option<ConnectExit>>,
    }

    impl Sink {
        fn text(&self) -> String {
            self.output.lock().unwrap().concat()
        }

        fn wait_exit(&self, timeout: Duration) -> ConnectExit {
            let deadline = Instant::now() + timeout;
            loop {
                if let Some(exit) = *self.exit.lock().unwrap() {
                    return exit;
                }
                assert!(
                    Instant::now() < deadline,
                    "no exit; output: {:?}",
                    self.text()
                );
                thread::sleep(Duration::from_millis(20));
            }
        }
    }

    #[test]
    fn an_install_only_succeeds_when_the_cli_is_there_afterwards() {
        // Seen for real: `curl -fsSL … | sh` with the download refused
        // (curl: (22) … 403) exits 0 without installing anything.
        assert!(!run_succeeded(ConnectAction::Install, true, || false));
        assert!(run_succeeded(ConnectAction::Install, true, || true));
        assert!(!run_succeeded(ConnectAction::Install, false, || true));
        // A sign-in is judged by its own exit; nothing is re-detected.
        assert!(run_succeeded(ConnectAction::Login, true, || panic!("not asked")));
        assert!(!run_succeeded(ConnectAction::Login, false, || panic!("not asked")));
        assert!(!run_succeeded(ConnectAction::Install, false, || panic!("not asked")));
    }

    fn spawn(script: &str, sink: &Arc<Sink>) -> ConnectSession {
        let (out, exit) = (sink.clone(), sink.clone());
        ConnectSession::spawn(
            "/bin/sh",
            &["-c".to_string(), script.to_string()],
            24,
            80,
            move |text| out.output.lock().unwrap().push(text),
            move |e| *exit.exit.lock().unwrap() = Some(e),
        )
        .unwrap()
    }

    #[cfg(unix)]
    #[test]
    fn a_real_command_streams_its_output_then_reports_its_exit_code() {
        let sink = Arc::new(Sink::default());
        let _session = spawn("echo hi; exit 3", &sink);
        let exit = sink.wait_exit(Duration::from_secs(10));
        assert_eq!(
            exit,
            ConnectExit {
                success: false,
                code: Some(3)
            }
        );
        // All output was delivered before the exit, even though nobody
        // called `output_ready` (the process ended first).
        assert!(sink.text().contains("hi"), "{:?}", sink.text());

        let sink = Arc::new(Sink::default());
        let _session = spawn("echo done", &sink);
        assert_eq!(
            sink.wait_exit(Duration::from_secs(10)),
            ConnectExit {
                success: true,
                code: Some(0)
            }
        );
    }

    #[cfg(unix)]
    #[test]
    fn output_is_held_until_ready_then_delivered_once_in_order() {
        let sink = Arc::new(Sink::default());
        let session = spawn("echo first; read line; echo \"got $line\"", &sink);
        thread::sleep(Duration::from_millis(300));
        assert_eq!(sink.text(), "", "nothing before the frontend is ready");

        session.output_ready();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !sink.text().contains("first") {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(20));
        }
        session.write(b"typed\n").unwrap();
        let exit = sink.wait_exit(Duration::from_secs(10));
        assert!(exit.success);
        let text = sink.text();
        assert_eq!(text.matches("first").count(), 1, "{text:?}");
        let first = text.find("first").unwrap();
        let got = text.find("got typed").unwrap_or_else(|| panic!("{text:?}"));
        assert!(first < got);
    }

    #[cfg(unix)]
    #[test]
    fn held_output_is_released_by_itself_after_a_moment() {
        let sink = Arc::new(Sink::default());
        let session = spawn("echo early; sleep 30", &sink);
        let deadline = Instant::now() + HOLD_OUTPUT_FOR + Duration::from_secs(5);
        while !sink.text().contains("early") {
            assert!(Instant::now() < deadline, "held output never released");
            thread::sleep(Duration::from_millis(20));
        }
        session.cancel();
        sink.wait_exit(Duration::from_secs(10));
    }

    #[cfg(unix)]
    #[test]
    fn cancel_kills_the_process_and_what_it_started_and_still_reports_the_exit() {
        let dir =
            std::env::temp_dir().join(format!("infinabox-connect-cancel-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let pidfile = dir.join("grandchild");
        let sink = Arc::new(Sink::default());
        let session = spawn(
            &format!("sleep 60 & echo $! > '{}'; wait", pidfile.display()),
            &sink,
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        let grandchild = loop {
            if let Ok(pid) = std::fs::read_to_string(&pidfile) {
                if !pid.trim().is_empty() {
                    break pid.trim().to_string();
                }
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(20));
        };
        session.cancel();
        assert_eq!(
            sink.wait_exit(Duration::from_secs(10)),
            ConnectExit {
                success: false,
                code: None
            }
        );
        thread::sleep(Duration::from_millis(200));
        // Not `kill -0`: once its parent is gone, a killed process can sit
        // as a zombie until init reaps it (never, in some containers).
        let stat = std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", &grandchild])
            .output()
            .unwrap();
        let stat = String::from_utf8_lossy(&stat.stdout).trim().to_string();
        let alive = !stat.is_empty() && !stat.starts_with('Z');
        assert!(!alive, "the process group should have been killed");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_character_split_across_reads_is_kept_whole() {
        let bytes = "ümlaut ✓".as_bytes();
        let mut tail = Vec::new();
        let mut out = String::new();
        for byte in bytes {
            out.push_str(&decode_utf8(&mut tail, std::slice::from_ref(byte)));
        }
        assert_eq!(out, "ümlaut ✓");
        assert!(tail.is_empty());
        // Really invalid bytes are replaced, not carried forever.
        assert_eq!(decode_utf8(&mut tail, b"a\xffb"), "a\u{FFFD}b");
        assert!(tail.is_empty());
    }

    #[test]
    fn held_output_is_capped_from_the_front() {
        let out = Arc::new(Mutex::new(String::new()));
        let sink = out.clone();
        let gate = OutputGate::new(Box::new(move |t| sink.lock().unwrap().push_str(&t)));
        gate.push(&vec![b'a'; MAX_HELD_BYTES]);
        gate.push(b"end");
        gate.open();
        let text = out.lock().unwrap().clone();
        assert_eq!(text.len(), MAX_HELD_BYTES);
        assert!(text.ends_with("end"));
    }

    /// A runtime that plays back a scripted turn, for `test_connection`.
    struct Scripted {
        events: Vec<AgentEvent>,
        /// Block until cancelled instead of finishing.
        hang: bool,
        cancelled: AtomicBool,
        saw_folder: Mutex<Option<PathBuf>>,
    }

    impl Scripted {
        fn new(events: Vec<AgentEvent>, hang: bool) -> Self {
            Scripted {
                events,
                hang,
                cancelled: AtomicBool::new(false),
                saw_folder: Mutex::new(None),
            }
        }
    }

    impl TestTurn for Scripted {
        fn run(
            &self,
            req: TurnRequest,
            on_event: &mut dyn FnMut(AgentEvent),
        ) -> Result<(), String> {
            assert!(req.project_path.is_dir());
            assert_eq!(req.message, TEST_MESSAGE);
            assert_eq!(req.options, TurnOptions::default());
            *self.saw_folder.lock().unwrap() = Some(req.project_path.clone());
            while self.hang && !self.cancelled.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_millis(10));
            }
            if self.hang {
                on_event(AgentEvent::Error {
                    kind: AgentErrorKind::Cancelled,
                    message: "Stopped.".into(),
                });
                on_event(completed(true));
                return Ok(());
            }
            for event in &self.events {
                on_event(event.clone());
            }
            Ok(())
        }

        fn cancel(&self, thread_id: &str) {
            assert_eq!(thread_id, TEST_THREAD_ID);
            self.cancelled.store(true, Ordering::SeqCst);
        }
    }

    fn completed(is_error: bool) -> AgentEvent {
        AgentEvent::TurnCompleted {
            is_error,
            duration_ms: None,
            usage: None,
        }
    }

    fn no_mcp(folder: &Path) -> McpLaunch {
        McpLaunch {
            command: PathBuf::from("/nonexistent"),
            args: vec![],
            env: vec![(ENV_PROJECT.into(), folder.display().to_string())],
        }
    }

    #[test]
    fn a_reply_is_ok_and_the_temp_folder_is_removed() {
        let runtime = Scripted::new(
            vec![
                AgentEvent::AssistantText {
                    text: "hello\n".into(),
                },
                completed(false),
            ],
            false,
        );
        let result = test_connection(&runtime, no_mcp, TEST_TIMEOUT);
        assert!(result.ok, "{result:?}");
        assert_eq!(result.reply.as_deref(), Some("hello"));
        assert_eq!((result.error_kind, result.message), (None, None));
        let folder = runtime.saw_folder.lock().unwrap().clone().unwrap();
        assert!(!folder.exists(), "the test folder should be cleaned up");
    }

    #[test]
    fn the_first_error_is_reported() {
        let runtime = Scripted::new(
            vec![
                AgentEvent::Error {
                    kind: AgentErrorKind::NotAuthenticated,
                    message: "Please sign in.".into(),
                },
                AgentEvent::Error {
                    kind: AgentErrorKind::Other,
                    message: "later".into(),
                },
                completed(true),
            ],
            false,
        );
        let result = test_connection(&runtime, no_mcp, TEST_TIMEOUT);
        assert!(!result.ok);
        assert_eq!(result.error_kind, Some(AgentErrorKind::NotAuthenticated));
        assert_eq!(result.message.as_deref(), Some("Please sign in."));
        assert_eq!(result.reply, None);
    }

    #[test]
    fn a_turn_past_the_timeout_is_stopped_and_says_so() {
        let runtime = Scripted::new(vec![], true);
        let result = test_connection(&runtime, no_mcp, Duration::from_millis(200));
        assert!(runtime.cancelled.load(Ordering::SeqCst));
        assert!(!result.ok);
        assert_eq!(result.error_kind, Some(AgentErrorKind::Other));
        assert!(
            result.message.as_deref().unwrap().contains("didn't answer"),
            "{result:?}"
        );
        assert!(result.duration_ms >= 200);
    }

    #[test]
    fn a_cli_that_says_it_is_signed_out_is_not_run() {
        let mut info = ProviderInfo {
            id: ProviderId::Codex,
            name: "Codex".into(),
            installed: true,
            version: Some("0.157.1".into()),
            logged_in: Some(false),
            blurb: String::new(),
            install_command: None,
            install_blocker: None,
            login_command: "codex login".into(),
            docs_url: String::new(),
        };
        let result = signed_out_result(&info, 12).unwrap();
        assert!(!result.ok);
        assert_eq!(result.error_kind, Some(AgentErrorKind::NotAuthenticated));
        assert!(result.message.unwrap().starts_with("Codex says"));
        assert_eq!(result.duration_ms, 12);
        // Signed in, or unknown: the real turn decides.
        for logged_in in [Some(true), None] {
            info.logged_in = logged_in;
            assert!(signed_out_result(&info, 0).is_none());
        }
    }

    #[test]
    fn an_empty_reply_is_not_ok() {
        let runtime = Scripted::new(vec![completed(false)], false);
        let result = test_connection(&runtime, no_mcp, TEST_TIMEOUT);
        assert!(!result.ok);
        assert_eq!(result.error_kind, Some(AgentErrorKind::Other));
    }

    /// The real signed-in Claude Code, through the same code path as the
    /// command (the MCP server launch points nowhere: a test binary can't
    /// serve it, and "hello" needs no tools).
    #[test]
    #[ignore = "needs a real, signed-in `claude`; run with --ignored"]
    fn says_hello_with_the_real_claude_code() {
        let result = test_connection(
            &RuntimeTurn(&ClaudeCodeRuntime::new()),
            no_mcp,
            TEST_TIMEOUT,
        );
        eprintln!("{result:?}");
        assert!(result.ok, "{result:?}");
        assert!(
            result
                .reply
                .as_deref()
                .unwrap()
                .to_lowercase()
                .contains("hello"),
            "{result:?}"
        );
        assert!(result.duration_ms > 0);
    }

    /// The real Codex CLI through the turn itself (bypassing
    /// `signed_out_result`), not signed in: whatever really happens comes
    /// back as a failed test with a kind and a message — never `ok`. (On a
    /// machine that can't reach OpenAI it retries until the timeout.)
    #[test]
    #[ignore = "needs a real `codex` (CODEX_BIN) that isn't signed in; run with --ignored"]
    fn reports_the_real_codex_error_when_not_signed_in() {
        let program = std::env::var("CODEX_BIN").expect("set CODEX_BIN to the codex binary");
        let result = test_connection(
            &RuntimeTurn(&CodexRuntime::with_program(program)),
            no_mcp,
            TEST_TIMEOUT,
        );
        eprintln!("{result:?}");
        assert!(!result.ok);
        assert!(result.error_kind.is_some() && result.message.is_some());
    }
}
