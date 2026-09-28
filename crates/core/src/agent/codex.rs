//! `CodexRuntime`: the second `AgentRuntime` (spec §8.1), backed by the
//! user's own installed Codex CLI, driven headless with JSON output (no
//! PTY). The CLI is already signed in outside InfinaBox; this module never
//! touches credentials.
//!
//! One turn is one `codex exec` process, run in the project directory:
//!
//! ```text
//! codex exec --json --skip-git-repo-check
//!            --ignore-user-config --ignore-rules
//!            --disable apps --disable plugins --disable hooks
//!            --sandbox workspace-write -C <project>
//!            -c mcp_servers.infinabox.command="<app exe>"
//!            -c mcp_servers.infinabox.args=["--mcp-server"]
//!            -c mcp_servers.infinabox.env_vars=["INFINABOX_PROJECT", ...]
//!            -c mcp_servers.infinabox.default_tools_approval_mode="approve"
//!            -c shell_environment_policy.exclude=["INFINABOX_PROJECT", ...]
//!            -c developer_instructions="<prompt::system_prompt>"
//!            [resume <thread id>]
//!            -- <the user's message>
//! ```
//!
//! Each flag was checked against the recorded `codex exec --help` (0.157.1,
//! in `crates/core/tests/fixtures/codex/help.txt`) and real runs against the
//! scripted endpoint (`scripts/fixtures/mock-responses-server.mjs`, which
//! logs what the CLI sends):
//! - `--ignore-user-config` skips `$CODEX_HOME/config.toml` — the user's own
//!   MCP servers, instructions and settings (verified: a user MCP server and
//!   `developer_instructions` there no longer reached the request) — while
//!   sign-in still comes from `CODEX_HOME` as the help says. It also means
//!   a project's `.codex/config.toml` is never loaded: Codex only loads it
//!   for projects the user config marks trusted (verified: a project config
//!   with its own MCP server and instructions loaded without the flag, and
//!   not with it, even with the project marked trusted).
//! - `--ignore-rules` skips user and project execpolicy `.rules` files;
//!   `--disable apps/plugins/hooks` keep connectors, plugins (which can
//!   bring MCP servers) and hooks out, so the InfinaBox server is the only
//!   extra tool source.
//! - `--sandbox workspace-write`: edits and commands confined to the
//!   project. `exec resume` has no `--sandbox`/`-C` of its own, but the
//!   same flags given before `resume` apply to it (verified). Side effect
//!   worth knowing: in this version a `workspace-write` run records the
//!   project as trusted in the user's `$CODEX_HOME/config.toml` (Codex's own
//!   behaviour, with or without `--ignore-user-config`; verified).
//! - `--skip-git-repo-check`: InfinaBox projects are git repos, but a
//!   folder that isn't would otherwise fail with "Not inside a trusted
//!   directory".
//! - The MCP server is configured entirely with `-c` overrides. Its env
//!   (the bridge token) is never on the command line, which other users
//!   can read: the values go into the CLI's own environment and
//!   `env_vars` forwards them by name (verified: the server saw the token).
//!   `default_tools_approval_mode = "approve"` lets its tools run headless
//!   (without it every call failed with "MCP tool call requires approval,
//!   but approval policy is never"). `shell_environment_policy.exclude`
//!   keeps those variables out of the commands the agent runs (verified
//!   with `printenv` in the `g_shell_command` recording).
//! - `developer_instructions` carries `prompt::system_prompt` as an extra
//!   developer message next to Codex's own instructions (verified in the
//!   request). The project's `AGENTS.md` is *not* added to it: Codex reads
//!   `AGENTS.md` itself, even with `--ignore-user-config` (verified), so it
//!   would be there twice.
//! - The message goes last, after `--`, so one starting with `-` isn't read
//!   as a flag (verified, also after `resume <id>`); stdin is closed so
//!   Codex doesn't wait to append it.
//!
//! stdout is parsed line by line by `codex_stream::CodexStream` as it
//! arrives.

use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Context as _;

use super::codex_stream::{CodexStream, MCP_SERVER_NAME, StreamEnd};
use super::path::{find_on_path, login_shell_path};
use super::prompt::system_prompt;
use super::types::{AgentErrorKind, AgentEvent, AgentRuntime, RuntimeStatus, TurnRequest};

/// The CLI's program name, looked up on the login-shell PATH.
pub const PROGRAM: &str = "codex";

/// How long `codex --version` and `codex login status` each get before
/// detection gives up on them.
const DETECT_TIMEOUT: Duration = Duration::from_secs(10);

/// After a cancel asks the CLI to stop (SIGTERM), how long it gets before
/// it's killed outright.
const CANCEL_GRACE: Duration = Duration::from_secs(3);

/// How much stderr is kept for error classification.
const MAX_STDERR_BYTES: usize = 64 * 1024;

/// What `codex login status` prints (on stderr, exit code 1) when nobody is
/// signed in (`tests/fixtures/codex/login_status_signed_out.txt`).
const NOT_LOGGED_IN: &str = "Not logged in";

const NOT_INSTALLED_MESSAGE: &str = "InfinaBox couldn't find Codex (the `codex` command) \
on this computer. Install Codex and sign in to it, then try again.";

/// One thread's in-flight turn. Reserved in the map (with no child yet)
/// before the CLI is spawned, so the one-turn-per-thread check and the
/// insert happen under a single lock, and a cancel that arrives before the
/// process exists is still honoured once it does.
struct TurnSlot {
    child: Mutex<Option<Child>>,
    cancelled: AtomicBool,
}

type RunningMap = Arc<Mutex<HashMap<String, Arc<TurnSlot>>>>;

pub struct CodexRuntime {
    /// `codex` (looked up on the login-shell `PATH`), or a path to a
    /// specific executable.
    program: String,
    running: RunningMap,
}

impl CodexRuntime {
    pub fn new() -> Self {
        Self::with_program(PROGRAM)
    }

    /// Uses `program` instead of `codex`: a name looked up on `PATH`, or a
    /// path (tests point this at a fake CLI).
    pub fn with_program(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            running: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// The executable to run, if it can be found.
    fn resolve_program(&self) -> Option<PathBuf> {
        if self.program.contains('/') || self.program.contains(std::path::MAIN_SEPARATOR) {
            return Some(PathBuf::from(&self.program));
        }
        find_on_path(&self.program, login_shell_path())
    }
}

impl Default for CodexRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentRuntime for CodexRuntime {
    fn detect(&self) -> RuntimeStatus {
        let program = self.resolve_program().filter(|p| p.is_file());
        let (version, logged_in) = match &program {
            Some(p) => (read_version(p), read_logged_in(p)),
            None => (None, None),
        };
        RuntimeStatus {
            name: PROGRAM.to_string(),
            installed: program.is_some(),
            version,
            logged_in,
        }
    }

    /// Runs one turn and blocks until it ends.
    ///
    /// When this returns `Ok`, the last event delivered was always a
    /// `TurnCompleted`, whatever happened: the CLI not being installed
    /// (`Error{NotInstalled}`), failing to start or dying without ending
    /// the turn (`Error{ProcessFailed}` or a classified kind, with the real
    /// error text), or a cancel (`Error{Cancelled, "Stopped."}`).
    /// `FilesChanged` always comes just before the `Error`/`TurnCompleted`
    /// that end the turn, including a cancelled one (edits made before the
    /// cancel are real). It returns `Err` without emitting anything only
    /// when the turn couldn't be set up at all (project folder missing, or a
    /// turn is already running for this thread) — the caller must end the
    /// turn itself then.
    fn run_turn(
        &self,
        req: TurnRequest,
        on_event: &mut dyn FnMut(AgentEvent),
    ) -> anyhow::Result<()> {
        if !req.project_path.is_dir() {
            anyhow::bail!(
                "The project folder {} doesn't exist.",
                req.project_path.display()
            );
        }
        let Some(program) = self.resolve_program() else {
            emit_failure(
                on_event,
                AgentErrorKind::NotInstalled,
                NOT_INSTALLED_MESSAGE.to_string(),
            );
            return Ok(());
        };
        let Some(turn) = RunningGuard::reserve(&self.running, &req.thread_id) else {
            anyhow::bail!("A turn is already running in this chat.");
        };

        // Codex reads the project's AGENTS.md itself (see the module docs).
        let instructions = system_prompt(&req.options, None);
        let mut cmd = Command::new(&program);
        cmd.args(build_args(&req, &instructions))
            .current_dir(&req.project_path)
            .env("PATH", login_shell_path())
            .envs(req.mcp.env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            // Its own process group, so a cancel stops the CLI and whatever
            // it started (the MCP server, shell commands) together.
            cmd.process_group(0);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        if turn.was_cancelled() {
            // Stopped before the CLI even started.
            for event in CodexStream::new(Vec::new()).finish(StreamEnd::Cancelled) {
                on_event(event);
            }
            return Ok(());
        }
        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                emit_failure(
                    on_event,
                    AgentErrorKind::NotInstalled,
                    NOT_INSTALLED_MESSAGE.to_string(),
                );
                return Ok(());
            }
            Err(e) => {
                emit_failure(
                    on_event,
                    AgentErrorKind::ProcessFailed,
                    format!("Codex couldn't be started: {e}"),
                );
                return Ok(());
            }
        };
        let stdout = child.stdout.take().context("codex stdout was not piped")?;
        let stderr = child.stderr.take().context("codex stderr was not piped")?;

        turn.slot.set_child(child);
        let stderr_reader = std::thread::spawn(move || read_capped(stderr, MAX_STDERR_BYTES));

        let mut roots = vec![req.project_path.clone()];
        if let Ok(canonical) = req.project_path.canonicalize()
            && canonical != req.project_path
        {
            roots.push(canonical);
        }
        let mut stream = CodexStream::new(roots);

        let mut reader = BufReader::new(stdout);
        let mut line = Vec::new();
        loop {
            line.clear();
            match reader.read_until(b'\n', &mut line) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    for event in stream.parse_line(&String::from_utf8_lossy(&line)) {
                        on_event(event);
                    }
                }
            }
        }

        let status = turn.wait();
        let stderr = stderr_reader.join().unwrap_or_default();
        // Codex exits 0 on SIGTERM without ending the turn (the
        // `h_cancelled` recording), so a cancel is known from our own flag.
        let end = if turn.was_cancelled() {
            StreamEnd::Cancelled
        } else {
            StreamEnd::Exited {
                code: status.and_then(|s| s.code()),
                stderr,
            }
        };
        for event in stream.finish(end) {
            on_event(event);
        }
        drop(turn);
        Ok(())
    }

    /// Stops the thread's in-flight turn: asks the CLI to exit (SIGTERM to
    /// its process group on Unix), then kills it if it's still running after
    /// `CANCEL_GRACE`. `run_turn` then ends the turn with
    /// `Error{Cancelled, "Stopped."}` + `TurnCompleted{is_error: true}`
    /// (unless the CLI had already ended the turn itself).
    ///
    /// A cancel that arrives after the turn is reserved but before the CLI
    /// has been spawned is kept and applied as soon as it is (or the spawn
    /// is skipped).
    fn cancel(&self, thread_id: &str) {
        let slot = self.running.lock().unwrap().get(thread_id).cloned();
        if let Some(slot) = slot {
            slot.cancel();
        }
    }
}

/// The CLI arguments for one turn (see the module docs for why each).
/// `instructions` is the full `prompt::system_prompt` text.
pub(crate) fn build_args(req: &TurnRequest, instructions: &str) -> Vec<OsString> {
    let env_names: Vec<&str> = req.mcp.env.iter().map(|(k, _)| k.as_str()).collect();
    let server = format!("mcp_servers.{MCP_SERVER_NAME}");
    let mut args: Vec<OsString> = [
        "exec",
        "--json",
        "--skip-git-repo-check",
        "--ignore-user-config",
        "--ignore-rules",
        "--disable",
        "apps",
        "--disable",
        "plugins",
        "--disable",
        "hooks",
        "--sandbox",
        "workspace-write",
        "-C",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    args.push(req.project_path.clone().into());
    let overrides = [
        format!(
            "{server}.command={}",
            toml_string(&req.mcp.command.to_string_lossy())
        ),
        format!("{server}.args={}", toml_list(&req.mcp.args)),
        format!("{server}.env_vars={}", toml_list(&env_names)),
        format!("{server}.default_tools_approval_mode=\"approve\""),
        format!("shell_environment_policy.exclude={}", toml_list(&env_names)),
        format!("developer_instructions={}", toml_string(instructions)),
    ];
    for value in overrides {
        args.push("-c".into());
        args.push(value.into());
    }
    if let Some(id) = &req.resume_provider_session_id {
        args.push("resume".into());
        args.push(id.into());
    }
    args.push("--".into());
    args.push(req.message.clone().into());
    args
}

/// `value` as a TOML basic string (what `-c key=value` parses), with
/// everything TOML doesn't allow raw escaped.
pub(crate) fn toml_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn toml_list<S: AsRef<str>>(items: &[S]) -> String {
    let items: Vec<String> = items.iter().map(|s| toml_string(s.as_ref())).collect();
    format!("[{}]", items.join(","))
}

impl TurnSlot {
    fn new() -> Self {
        Self {
            child: Mutex::new(None),
            cancelled: AtomicBool::new(false),
        }
    }

    /// Marks the turn cancelled and, if its process exists, stops it. The
    /// flag is set before taking the child lock and `set_child` checks it
    /// after storing the child under that lock, so one of the two always
    /// sees the other.
    fn cancel(self: &Arc<Self>) {
        self.cancelled.store(true, Ordering::SeqCst);
        if self.child.lock().unwrap().is_some() {
            self.stop();
        }
    }

    fn set_child(self: &Arc<Self>, child: Child) {
        let mut slot = self.child.lock().unwrap();
        *slot = Some(child);
        drop(slot);
        if self.cancelled.load(Ordering::SeqCst) {
            self.stop();
        }
    }

    /// SIGTERM now, SIGKILL after `CANCEL_GRACE` if it's still running.
    fn stop(self: &Arc<Self>) {
        signal(&self.child, false);
        let slot = self.clone();
        std::thread::spawn(move || {
            let deadline = Instant::now() + CANCEL_GRACE;
            while Instant::now() < deadline {
                if has_exited(&slot.child) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            signal(&slot.child, true);
        });
    }
}

/// Holds a thread's reserved slot in the cancel map for as long as the turn
/// runs. Dropping it removes the entry (only if it's still this turn's) and
/// kills the process if it's somehow still running.
struct RunningGuard {
    running: RunningMap,
    thread_id: String,
    slot: Arc<TurnSlot>,
}

impl RunningGuard {
    /// Reserves `thread_id`'s slot, or `None` if a turn already holds it.
    /// Check and insert happen under one lock.
    fn reserve(running: &RunningMap, thread_id: &str) -> Option<Self> {
        let mut map = running.lock().unwrap();
        if map.contains_key(thread_id) {
            return None;
        }
        let slot = Arc::new(TurnSlot::new());
        map.insert(thread_id.to_string(), slot.clone());
        Some(Self {
            running: running.clone(),
            thread_id: thread_id.to_string(),
            slot,
        })
    }

    /// Waits for the process to exit, without holding its lock while
    /// waiting (so `cancel` can still reach it).
    fn wait(&self) -> Option<ExitStatus> {
        loop {
            match self
                .slot
                .child
                .lock()
                .unwrap()
                .as_mut()
                .map(Child::try_wait)
            {
                Some(Ok(Some(status))) => return Some(status),
                Some(Ok(None)) => {}
                None | Some(Err(_)) => return None,
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn was_cancelled(&self) -> bool {
        self.slot.cancelled.load(Ordering::SeqCst)
    }
}

impl Drop for RunningGuard {
    fn drop(&mut self) {
        {
            let mut map = self.running.lock().unwrap();
            if map
                .get(&self.thread_id)
                .is_some_and(|s| Arc::ptr_eq(s, &self.slot))
            {
                map.remove(&self.thread_id);
            }
        }
        if !has_exited(&self.slot.child) {
            signal(&self.slot.child, true);
            if let Some(child) = self.slot.child.lock().unwrap().as_mut() {
                let _ = child.wait();
            }
        }
    }
}

/// True unless there's a process that is still running (no process yet
/// counts as exited: there's nothing to wait for).
fn has_exited(child: &Mutex<Option<Child>>) -> bool {
    !matches!(
        child.lock().unwrap().as_mut().map(Child::try_wait),
        Some(Ok(None))
    )
}

/// Asks the process (and its group, on Unix) to stop, or kills it when
/// `hard`. Only signals a process that hasn't been reaped yet, checked
/// under the same lock `wait` uses, so a reused pid is never signalled.
fn signal(child: &Mutex<Option<Child>>, hard: bool) {
    let mut guard = child.lock().unwrap();
    let Some(child) = guard.as_mut() else {
        return;
    };
    if !matches!(child.try_wait(), Ok(None)) {
        return;
    }
    #[cfg(unix)]
    {
        if let Ok(pid) = i32::try_from(child.id()) {
            let sig = if hard { libc::SIGKILL } else { libc::SIGTERM };
            // SAFETY: plain syscall; a negative pid addresses the process
            // group we created for this child with `process_group(0)`.
            unsafe {
                libc::kill(-pid, sig);
            }
        }
        if hard {
            let _ = child.kill();
        }
    }
    #[cfg(not(unix))]
    {
        let _ = hard;
        let _ = child.kill();
    }
}

fn emit_failure(on_event: &mut dyn FnMut(AgentEvent), kind: AgentErrorKind, message: String) {
    on_event(AgentEvent::Error { kind, message });
    on_event(AgentEvent::TurnCompleted {
        is_error: true,
        duration_ms: None,
        usage: None,
    });
}

/// Reads everything from `source`, keeping at most the last `max` bytes.
fn read_capped(mut source: impl Read, max: usize) -> String {
    let mut kept: Vec<u8> = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        match source.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                kept.extend_from_slice(&buf[..n]);
                if kept.len() > max {
                    kept.drain(..kept.len() - max);
                }
            }
        }
    }
    String::from_utf8_lossy(&kept).into_owned()
}

/// Runs `<program> <args>` and returns its exit code and its stdout and
/// stderr together, or `None` if it can't start, is ended by a signal, or
/// takes longer than `DETECT_TIMEOUT`.
fn run_detect_command(program: &Path, args: &[&str]) -> Option<(i32, String)> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .env("PATH", login_shell_path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let child = Arc::new(Mutex::new(Some(cmd.spawn().ok()?)));
    let (stdout, stderr) = {
        let mut guard = child.lock().unwrap();
        let c = guard.as_mut()?;
        (c.stdout.take()?, c.stderr.take()?)
    };
    let out = std::thread::spawn(move || read_capped(stdout, 16 * 1024));
    let err = std::thread::spawn(move || read_capped(stderr, 16 * 1024));
    let deadline = Instant::now() + DETECT_TIMEOUT;
    let status = loop {
        let polled = child.lock().unwrap().as_mut()?.try_wait();
        match polled {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                signal(&child, true);
                if let Some(c) = child.lock().unwrap().as_mut() {
                    let _ = c.wait();
                }
                return None;
            }
        }
    };
    let text = format!("{}{}", out.join().ok()?, err.join().ok()?);
    Some((status.code()?, text))
}

/// `codex --version`, e.g. `codex-cli 0.157.1` → `0.157.1`.
fn read_version(program: &Path) -> Option<String> {
    match run_detect_command(program, &["--version"])? {
        (0, output) => parse_version(&output),
        _ => None,
    }
}

/// `codex login status`: exit 0 is signed in (it prints how, e.g. "Logged
/// in using an API key - sk-…"); exit 1 with "Not logged in" is not.
/// Anything else (an error, a timeout) means it can't tell.
fn read_logged_in(program: &Path) -> Option<bool> {
    parse_login_status(run_detect_command(program, &["login", "status"])?)
}

fn parse_login_status((code, output): (i32, String)) -> Option<bool> {
    match code {
        0 => Some(true),
        1 if output.lines().any(|l| l.trim() == NOT_LOGGED_IN) => Some(false),
        _ => None,
    }
}

fn parse_version(output: &str) -> Option<String> {
    // Codex prints warnings before the version on some setups, so look for
    // the `codex-cli <version>` line rather than taking the first line.
    let line = output
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("codex"))
        .or_else(|| output.lines().map(str::trim).find(|l| !l.is_empty()))?;
    let version = line
        .split_whitespace()
        .find(|w| w.chars().next().is_some_and(|c| c.is_ascii_digit()));
    Some(version.unwrap_or(line).to_string())
}

#[cfg(test)]
mod tests;
