//! `AgentRuntime` backed by the user's own installed Claude Code CLI, driven
//! headless with streaming JSON output (no PTY). The CLI is already signed
//! in outside InfinaBox; this module never touches credentials.
//!
//! One turn is one `claude -p` process, run in the project directory:
//!
//! ```text
//! claude -p --output-format stream-json --verbose
//!        [--resume <session id>]
//!        --mcp-config <temp file> --strict-mcp-config
//!        --tools Read,Edit,Write,Glob,Grep
//!        --allowedTools Read,Edit,Write,Glob,Grep,mcp__infinabox__*
//!        --permission-mode acceptEdits
//!        --append-system-prompt <prompts/director.md>
//!        -- <the user's message>
//! ```
//!
//! Each flag was checked against the recorded `claude --help` (2.1.283, in
//! `crates/core/tests/fixtures/claude/help.txt`) and a real run:
//! - `--tools` limits the *available* built-in tools (no shell in Phase A);
//!   `--allowedTools` alone doesn't (see the fixtures README). With `--tools`
//!   set, the MCP tools are listed directly instead of behind `ToolSearch`.
//! - `--allowedTools` pre-approves the tools so nothing waits on a
//!   permission prompt nobody can answer. The `mcp__infinabox__*` wildcard
//!   was verified to allow the InfinaBox tools (without it, a real run
//!   recorded the MCP call in `permission_denials`).
//! - `--permission-mode acceptEdits` lets file edits through headless.
//! - `--tools`, `--allowedTools` and `--mcp-config` take a variable number
//!   of values, so the message goes last, after `--`, which also keeps a
//!   message starting with `-` from being read as a flag (verified).
//!
//! The message is passed as a plain argument (never through a shell), and
//! stdout is parsed line by line by `claude_stream::ClaudeStream` as it
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

use super::claude_stream::{ClaudeStream, StreamEnd};
use super::path::{find_on_path, login_shell_path};
use super::types::{
    AgentErrorKind, AgentEvent, AgentRuntime, McpLaunch, RuntimeStatus, TurnRequest,
};

/// The Director system prompt, appended to Claude Code's own.
pub const DIRECTOR_PROMPT: &str = include_str!("prompts/director.md");

/// The name the InfinaBox MCP server is registered under, which makes its
/// tools `mcp__infinabox__<tool>` (the naming seen in the `d_mcp_tool`
/// recording).
pub const MCP_SERVER_NAME: &str = "infinabox";

/// The only built-in tools the agent gets in Phase A: read, edit, write,
/// search. No shell.
pub const BUILTIN_TOOLS: &str = "Read,Edit,Write,Glob,Grep";

/// Pre-approved tools: the built-ins above plus every InfinaBox MCP tool
/// (the ten in `TOOL_NAMES` in `crates/mcp-server/src/server.rs`), via the
/// server wildcard verified against a real run.
pub const ALLOWED_TOOLS: &str = "Read,Edit,Write,Glob,Grep,mcp__infinabox__*";

/// What the CLI is called on `PATH`.
const PROGRAM: &str = "claude";

/// How long `claude --version` gets before detection gives up on a version.
const VERSION_TIMEOUT: Duration = Duration::from_secs(10);

/// After a cancel asks the CLI to stop (SIGTERM), how long it gets before
/// it's killed outright.
const CANCEL_GRACE: Duration = Duration::from_secs(3);

/// How much stderr is kept for error classification.
const MAX_STDERR_BYTES: usize = 64 * 1024;

const NOT_INSTALLED_MESSAGE: &str = "InfinaBox couldn't find Claude Code (the `claude` command) \
on this computer. Install Claude Code and sign in to it, then try again.";

struct RunningTurn {
    child: Arc<Mutex<Child>>,
    cancelled: Arc<AtomicBool>,
}

type RunningMap = Arc<Mutex<HashMap<String, RunningTurn>>>;

pub struct ClaudeCodeRuntime {
    /// `claude` (looked up on the login-shell `PATH`), or a path to a
    /// specific executable.
    program: String,
    running: RunningMap,
}

impl Default for ClaudeCodeRuntime {
    fn default() -> Self {
        Self::with_program(PROGRAM)
    }
}

impl ClaudeCodeRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    /// A runtime that runs `program` instead of `claude`: a name looked up
    /// on `PATH`, or a path. Used by tests with a stand-in CLI.
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

impl AgentRuntime for ClaudeCodeRuntime {
    fn detect(&self) -> RuntimeStatus {
        let program = self.resolve_program().filter(|p| p.is_file());
        RuntimeStatus {
            name: PROGRAM.to_string(),
            installed: program.is_some(),
            version: program.and_then(|p| read_version(&p)),
        }
    }

    /// Runs one turn and blocks until it ends.
    ///
    /// When this returns `Ok`, the last event delivered was always a
    /// `TurnCompleted`, whatever happened: the CLI not being installed
    /// (`Error{NotInstalled}`), failing to start or dying without a result
    /// (`Error{ProcessFailed}` or a classified kind, with the real stderr
    /// text), or a cancel (`Error{Other, "Stopped."}`). `FilesChanged`
    /// always comes just before the `Error`/`TurnCompleted` that end the
    /// turn, including a cancelled one (edits made before the cancel are
    /// real). It returns `Err` without emitting anything only when the turn
    /// couldn't be set up at all (project folder missing, the temp MCP
    /// config couldn't be written, or a turn is already running for this
    /// thread) — the caller must end the turn itself then.
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
        if self.running.lock().unwrap().contains_key(&req.thread_id) {
            anyhow::bail!("A turn is already running in this chat.");
        }

        let config = TempMcpConfig::write(&req.mcp)?;
        let mut cmd = Command::new(&program);
        cmd.args(build_args(&req, config.path()))
            .current_dir(&req.project_path)
            .env("PATH", login_shell_path())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            // Its own process group, so a cancel stops the CLI and whatever
            // it started (the MCP server) together.
            cmd.process_group(0);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
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
                    format!("Claude Code couldn't be started: {e}"),
                );
                return Ok(());
            }
        };
        let stdout = child.stdout.take().context("claude stdout was not piped")?;
        let stderr = child.stderr.take().context("claude stderr was not piped")?;

        let turn = RunningGuard::register(&self.running, &req.thread_id, child);
        let stderr_reader = std::thread::spawn(move || read_capped(stderr, MAX_STDERR_BYTES));

        let mut roots = vec![req.project_path.clone()];
        if let Ok(canonical) = req.project_path.canonicalize()
            && canonical != req.project_path
        {
            roots.push(canonical);
        }
        let mut stream = ClaudeStream::new(roots);

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
        drop(config);
        Ok(())
    }

    /// Stops the thread's in-flight turn: asks the CLI to exit (SIGTERM to
    /// its process group on Unix), then kills it if it's still running after
    /// `CANCEL_GRACE`. `run_turn` then ends the turn with
    /// `Error{Other, "Stopped."}` + `TurnCompleted{is_error: true}` (unless
    /// the CLI had already sent its own result).
    fn cancel(&self, thread_id: &str) {
        let target = self
            .running
            .lock()
            .unwrap()
            .get(thread_id)
            .map(|t| (t.child.clone(), t.cancelled.clone()));
        let Some((child, cancelled)) = target else {
            return;
        };
        cancelled.store(true, Ordering::SeqCst);
        signal(&child, false);
        std::thread::spawn(move || {
            let deadline = Instant::now() + CANCEL_GRACE;
            while Instant::now() < deadline {
                if has_exited(&child) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            signal(&child, true);
        });
    }
}

/// The CLI arguments for one turn (see the module docs for why each).
pub(crate) fn build_args(req: &TurnRequest, mcp_config: &Path) -> Vec<OsString> {
    let mut args: Vec<OsString> = ["-p", "--output-format", "stream-json", "--verbose"]
        .into_iter()
        .map(OsString::from)
        .collect();
    if let Some(id) = &req.resume_provider_session_id {
        args.push("--resume".into());
        args.push(id.into());
    }
    args.push("--mcp-config".into());
    args.push(mcp_config.into());
    args.push("--strict-mcp-config".into());
    args.push("--tools".into());
    args.push(BUILTIN_TOOLS.into());
    args.push("--allowedTools".into());
    args.push(ALLOWED_TOOLS.into());
    args.push("--permission-mode".into());
    args.push("acceptEdits".into());
    args.push("--append-system-prompt".into());
    args.push(DIRECTOR_PROMPT.into());
    args.push("--".into());
    args.push(req.message.clone().into());
    args
}

/// The `--mcp-config` JSON registering the InfinaBox server.
pub(crate) fn mcp_config_json(mcp: &McpLaunch) -> serde_json::Value {
    let env: serde_json::Map<String, serde_json::Value> = mcp
        .env
        .iter()
        .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
        .collect();
    serde_json::json!({
        "mcpServers": {
            MCP_SERVER_NAME: {
                "command": mcp.command.to_string_lossy(),
                "args": mcp.args,
                "env": env,
            }
        }
    })
}

/// The MCP config file for one turn. It holds the bridge token, so it's
/// readable only by the user (Unix) and deleted when the turn ends.
struct TempMcpConfig(PathBuf);

impl TempMcpConfig {
    fn write(mcp: &McpLaunch) -> anyhow::Result<Self> {
        use std::io::Write;
        let path =
            std::env::temp_dir().join(format!("infinabox-mcp-{}.json", uuid::Uuid::new_v4()));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&path)
            .with_context(|| format!("couldn't create the MCP config at {}", path.display()))?;
        let config = Self(path);
        file.write_all(serde_json::to_string(&mcp_config_json(mcp))?.as_bytes())
            .context("couldn't write the MCP config")?;
        Ok(config)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempMcpConfig {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Keeps a running turn in the cancel map for as long as it lives. Dropping
/// it removes the entry and, if the process is somehow still running (e.g.
/// the event callback panicked), kills it rather than leaking it.
struct RunningGuard {
    running: RunningMap,
    thread_id: String,
    child: Arc<Mutex<Child>>,
    cancelled: Arc<AtomicBool>,
}

impl RunningGuard {
    fn register(running: &RunningMap, thread_id: &str, child: Child) -> Self {
        let child = Arc::new(Mutex::new(child));
        let cancelled = Arc::new(AtomicBool::new(false));
        running.lock().unwrap().insert(
            thread_id.to_string(),
            RunningTurn {
                child: child.clone(),
                cancelled: cancelled.clone(),
            },
        );
        Self {
            running: running.clone(),
            thread_id: thread_id.to_string(),
            child,
            cancelled,
        }
    }

    /// Waits for the process to exit, without holding its lock while
    /// waiting (so `cancel` can still reach it).
    fn wait(&self) -> Option<ExitStatus> {
        loop {
            match self.child.lock().unwrap().try_wait() {
                Ok(Some(status)) => return Some(status),
                Ok(None) => {}
                Err(_) => return None,
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn was_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

impl Drop for RunningGuard {
    fn drop(&mut self) {
        self.running.lock().unwrap().remove(&self.thread_id);
        if !has_exited(&self.child) {
            signal(&self.child, true);
            let _ = self.child.lock().unwrap().wait();
        }
    }
}

fn has_exited(child: &Mutex<Child>) -> bool {
    !matches!(child.lock().unwrap().try_wait(), Ok(None))
}

/// Asks the process (and its group, on Unix) to stop, or kills it when
/// `hard`. Only signals a process that hasn't been reaped yet, checked
/// under the same lock `wait` uses, so a reused pid is never signalled.
fn signal(child: &Mutex<Child>, hard: bool) {
    let mut child = child.lock().unwrap();
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

/// `<program> --version`, e.g. `2.1.283 (Claude Code)` → `2.1.283`. `None`
/// if it fails, prints nothing, or takes longer than `VERSION_TIMEOUT`.
fn read_version(program: &Path) -> Option<String> {
    let mut cmd = Command::new(program);
    cmd.arg("--version")
        .env("PATH", login_shell_path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd.spawn().ok()?;
    let stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || read_capped(stdout, 4096));
    let deadline = Instant::now() + VERSION_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    if !status.success() {
        return None;
    }
    parse_version(&reader.join().ok()?)
}

fn parse_version(output: &str) -> Option<String> {
    let line = output.lines().map(str::trim).find(|l| !l.is_empty())?;
    let first = line.split_whitespace().next()?;
    if first.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        Some(first.to_string())
    } else {
        Some(line.to_string())
    }
}

#[cfg(test)]
mod tests;
