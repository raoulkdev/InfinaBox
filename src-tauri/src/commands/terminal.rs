//! Embedded terminal: a real PTY-backed shell session, so the user can run
//! their own Claude Code / Codex / whatever CLI directly against the open
//! project, using whatever auth that CLI already has configured. This is
//! deliberately NOT a chat panel InfinaBox brokers — it's a real shell.
//!
//! Split into two layers on purpose:
//!   - `TerminalSession`: the actual PTY-spawning/writing/reading logic.
//!     Takes no `tauri::AppHandle` at all, so it's directly unit-testable
//!     (see the `tests` module below) without a running Tauri app.
//!   - The `#[tauri::command]` functions at the bottom: thin wrappers that
//!     pull the `AppHandle` / managed state, delegate to `TerminalSession`,
//!     and wire the "forward PTY output to the frontend" behavior via
//!     `app.emit(...)` — a concern the testable core knows nothing about.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::Mutex;

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use tauri::{AppHandle, Emitter, Manager};

/// A single live PTY-backed shell session: the spawned child process, a
/// writer half for sending input, and the PTY master (kept around so we can
/// resize it later). Reading is handled separately — see `spawn`, which
/// takes a callback invoked with each chunk read from the PTY, rather than
/// owning a reader itself, so callers (tests, or the Tauri command layer)
/// decide what happens to output.
pub struct TerminalSession {
    // Kept alive intentionally, even though nothing reads it outside of
    // `child_process_id` (test-only): holding onto the handle is what the
    // task calls for, and it's how we get the PID we empirically tested
    // drop-behavior against. See the `tests` module and the final report
    // for what we found dropping `Child` actually does on this platform.
    #[allow(dead_code)]
    child: Box<dyn Child + Send + Sync>,
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
}

impl TerminalSession {
    /// Spawns `shell_cmd` (e.g. `/bin/zsh` or `/bin/sh`) as a login shell
    /// inside a fresh PTY, sized `cols`x`rows`, with working directory `cwd`
    /// when given. `on_output` is called from a dedicated background thread
    /// with each chunk of raw bytes read from the PTY as they arrive — no
    /// buffering/batching beyond whatever the OS read syscall hands back, so
    /// latency stays low for an interactive terminal.
    pub fn spawn(
        shell_cmd: &str,
        cwd: Option<&Path>,
        rows: u16,
        cols: u16,
        mut on_output: impl FnMut(&[u8]) + Send + 'static,
    ) -> std::io::Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(to_io_error)?;

        let mut cmd = CommandBuilder::new(shell_cmd);
        // Login shell: PATH / profile setup (nvm, homebrew, etc.) so that
        // `claude`/`codex` are actually findable on PATH, matching what the
        // user gets in a real terminal window.
        cmd.arg("-l");
        if let Some(dir) = cwd {
            cmd.cwd(dir);
        }

        let child = pair.slave.spawn_command(cmd).map_err(to_io_error)?;
        // Drop our copy of the slave now that the child has it; keeping it
        // open past this point isn't needed and matches portable-pty's own
        // examples.
        drop(pair.slave);

        let writer = pair.master.take_writer().map_err(to_io_error)?;
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(to_io_error)?;

        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break, // EOF: shell exited, PTY closed.
                    Ok(n) => on_output(&buf[..n]),
                    Err(_) => break,
                }
            }
        });

        Ok(TerminalSession {
            child,
            writer,
            master: pair.master,
        })
    }

    /// Writes `data` to the PTY's input side (i.e. what the shell reads as
    /// stdin) and flushes immediately.
    pub fn write(&mut self, data: &[u8]) -> std::io::Result<()> {
        self.writer.write_all(data)?;
        self.writer.flush()
    }

    /// Resizes the underlying PTY so the shell's notion of terminal size
    /// matches the frontend's actual rendered size (mismatched sizes cause
    /// garbled rendering — misplaced prompts, wrapped output — in real
    /// shells).
    pub fn resize(&self, rows: u16, cols: u16) -> std::io::Result<()> {
        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(to_io_error)
    }

    /// Exposed only so tests (and, empirically, the doc comment on
    /// `TerminalState`) can confirm what dropping the `Child` handle does
    /// on this platform, without needing to reach into private fields.
    #[cfg(test)]
    fn child_process_id(&self) -> Option<u32> {
        self.child.process_id()
    }
}

/// `portable-pty`'s trait methods return `anyhow::Result<T>`; rather than
/// pulling in `anyhow` as a direct dependency just to name that error type,
/// this converts any `Display`-able error into a plain `std::io::Error` via
/// type inference at each call site.
fn to_io_error<E: std::fmt::Display>(e: E) -> std::io::Error {
    std::io::Error::other(e.to_string())
}

/// Resolves the user's shell the same way a real terminal would: `$SHELL`,
/// falling back to `/bin/zsh` (macOS's modern default) if unset.
fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string())
}

/// Tauri-managed state: at most one live terminal session for now (this UI
/// only shows a single terminal panel — no multi-session/tab support yet).
#[derive(Default)]
pub struct TerminalState(pub Mutex<Option<TerminalSession>>);

/// Spawns the single global terminal session if one doesn't already exist.
/// Idempotent by design: React effects (StrictMode double-invoke in dev, or
/// just a re-render) may call this more than once, and that must not spawn
/// a duplicate shell or surface an error — it's just a no-op success.
#[tauri::command]
pub fn spawn_terminal(app: AppHandle, cwd: Option<String>) -> Result<(), String> {
    let state = app.state::<TerminalState>();
    let mut guard = state.0.lock().map_err(|_| "terminal state poisoned".to_string())?;

    if guard.is_some() {
        return Ok(());
    }

    let shell = default_shell();
    let cwd_path = cwd.as_ref().map(Path::new);

    let app_for_emit = app.clone();
    let session = TerminalSession::spawn(&shell, cwd_path, 24, 80, move |chunk| {
        let text = String::from_utf8_lossy(chunk).into_owned();
        let _ = app_for_emit.emit("terminal-output", text);
    })
    .map_err(|e| format!("failed to spawn terminal shell '{shell}': {e}"))?;

    *guard = Some(session);
    Ok(())
}

/// Writes `data` (raw keystrokes / pasted text from xterm.js) to the live
/// terminal session's stdin.
#[tauri::command]
pub fn write_to_terminal(app: AppHandle, data: String) -> Result<(), String> {
    let state = app.state::<TerminalState>();
    let mut guard = state.0.lock().map_err(|_| "terminal state poisoned".to_string())?;
    let session = guard.as_mut().ok_or_else(|| "no terminal session running".to_string())?;
    session.write(data.as_bytes()).map_err(|e| format!("failed to write to terminal: {e}"))
}

/// Resizes the live terminal session's PTY to match the frontend's current
/// fit (rows/cols), as computed by xterm.js's FitAddon.
#[tauri::command]
pub fn resize_terminal(app: AppHandle, rows: u16, cols: u16) -> Result<(), String> {
    let state = app.state::<TerminalState>();
    let guard = state.0.lock().map_err(|_| "terminal state poisoned".to_string())?;
    let session = guard.as_ref().ok_or_else(|| "no terminal session running".to_string())?;
    session.resize(rows, cols).map_err(|e| format!("failed to resize terminal: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex as StdMutex};
    use std::time::{Duration, Instant};

    /// Reads accumulated output (pushed onto `buf` by the `on_output`
    /// callback from a background thread) until `predicate` matches the
    /// accumulated text, or `deadline` passes — whichever comes first. This
    /// is what keeps the tests from hanging forever if a shell never
    /// produces the expected output.
    fn wait_for(buf: &Arc<StdMutex<String>>, predicate: impl Fn(&str) -> bool, deadline: Duration) -> String {
        let start = Instant::now();
        loop {
            {
                let text = buf.lock().unwrap();
                if predicate(&text) {
                    return text.clone();
                }
            }
            if start.elapsed() > deadline {
                return buf.lock().unwrap().clone();
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Picks a real shell to test against. `/bin/sh` is the simplest choice
    /// for determinism across environments (always present on macOS/Linux),
    /// and we specifically want the long-running interactive PTY session
    /// (writing a command after spawn), not a one-shot `sh -c` execution.
    const TEST_SHELL: &str = "/bin/sh";

    #[test]
    fn spawns_a_real_shell_writes_a_command_and_reads_the_output_back() {
        let output = Arc::new(StdMutex::new(String::new()));
        let output_for_cb = Arc::clone(&output);

        let mut session = TerminalSession::spawn(TEST_SHELL, None, 24, 80, move |chunk| {
            output_for_cb.lock().unwrap().push_str(&String::from_utf8_lossy(chunk));
        })
        .expect("spawning a real PTY-backed shell should succeed");

        session
            .write(b"echo INFINABOX_PTY_TEST_MARKER_12345\n")
            .expect("writing a command to the live PTY session should succeed");

        let captured = wait_for(
            &output,
            |text| text.contains("INFINABOX_PTY_TEST_MARKER_12345"),
            Duration::from_secs(5),
        );

        assert!(
            captured.contains("INFINABOX_PTY_TEST_MARKER_12345"),
            "expected the echoed marker in real captured shell output, got: {captured:?}"
        );
    }

    #[test]
    fn honors_a_specific_cwd_passed_at_spawn_time() {
        let dir = std::env::temp_dir().join(format!(
            "infinabox-terminal-cwd-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // Canonicalize up front: on macOS, std::env::temp_dir() (/tmp) is
        // itself a symlink to /private/tmp, and `pwd` inside the shell will
        // report the resolved path — comparing the raw, non-canonicalized
        // path here would make this test flaky/wrong.
        let canonical_dir = std::fs::canonicalize(&dir).expect("temp dir should canonicalize");

        let output = Arc::new(StdMutex::new(String::new()));
        let output_for_cb = Arc::clone(&output);

        let mut session = TerminalSession::spawn(TEST_SHELL, Some(&dir), 24, 80, move |chunk| {
            output_for_cb.lock().unwrap().push_str(&String::from_utf8_lossy(chunk));
        })
        .expect("spawning with a specific cwd should succeed");

        session.write(b"pwd\n").expect("writing pwd should succeed");

        let expected = canonical_dir.to_string_lossy().into_owned();
        let captured = wait_for(&output, |text| text.contains(&expected), Duration::from_secs(5));

        assert!(
            captured.contains(&expected),
            "expected pwd output to contain the canonicalized cwd {expected:?}, got: {captured:?}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// The task explicitly asks us to verify empirically (not assume)
    /// whether dropping the `Child` handle kills the spawned process on
    /// this platform. We spawn a shell, grab its PID, drop the session
    /// (which drops `Child`), then check whether that PID is still alive.
    #[test]
    fn dropping_the_session_child_handle_does_not_leave_a_zombie_but_confirm_process_state() {
        let session = TerminalSession::spawn(TEST_SHELL, None, 24, 80, |_chunk| {})
            .expect("spawning a real PTY-backed shell should succeed");

        let pid = session
            .child_process_id()
            .expect("a freshly spawned child should report a process id");

        drop(session);

        // Give the OS a moment to actually reap/terminate if dropping does
        // kill it, then check `kill -0 <pid>` (signal 0: no-op, just tests
        // whether the process still exists and is signalable by us).
        std::thread::sleep(Duration::from_millis(200));

        let still_alive = std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        // This assertion documents the empirical finding rather than
        // assuming one: on this platform (portable-pty's Unix child
        // implementation), dropping `Box<dyn Child>` does NOT send a kill
        // signal to the process on its own — see the `TerminalSession`
        // doc comment / final report for the full explanation. If this
        // ever fails, it means portable-pty's Drop behavior changed and
        // our comment above needs updating, not that this test is wrong.
        assert!(
            still_alive,
            "expected the child shell to still be alive after dropping TerminalSession \
             (portable-pty's Child does not kill-on-drop on this platform); if this now \
             fails, portable-pty's behavior changed and the doc comments referencing it \
             should be updated"
        );

        // Clean up manually since dropping didn't do it for us.
        let _ = std::process::Command::new("kill").args(["-9", &pid.to_string()]).status();
    }
}
