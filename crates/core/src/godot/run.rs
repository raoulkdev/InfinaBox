//! Runs the game as a child Godot process with stdout/stderr captured line
//! by line.
//!
//! Flags confirmed against the real 4.7.2 `--help` (`tests/fixtures/godot/
//! help.txt`): `--path <directory>`, `--windowed`, `--position <X>,<Y>`,
//! `--resolution <W>x<H>`.

use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use super::types::{GameOutputLine, GameState, OutputStream};

/// Where to put the game window: beside the InfinaBox window when known.
#[derive(Clone, Copy, Debug)]
pub struct WindowHint {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl WindowHint {
    fn args(&self) -> Vec<String> {
        vec![
            "--windowed".into(),
            "--position".into(),
            format!("{},{}", self.x, self.y),
            "--resolution".into(),
            format!("{}x{}", self.width, self.height),
        ]
    }
}

/// Lines longer than this are cut here (and marked) instead of buffered
/// without limit; the rest of that line is skipped.
pub const MAX_LINE_BYTES: usize = 64 * 1024;
pub const TRUNCATED_MARKER: &str = "…[line truncated]";

/// After the process exits, how long to wait for the readers to drain the
/// last output before reporting the exit anyway. Output pipes only close
/// when every holder closes them, and a process the game started (e.g.
/// `OS.create_process`) inherits them.
const DRAIN_GRACE: Duration = Duration::from_millis(1500);

pub struct GameProcess {
    child: Arc<Mutex<Child>>,
    /// Set by `stop()` so the exit watcher reports `Stopped`, not `Crashed`,
    /// for a process we killed ourselves.
    stop_requested: Arc<AtomicBool>,
    /// Whether the process group has already been killed (so a later
    /// `stop()` never signals a group id the OS may have reused).
    group_killed: bool,
}

impl GameProcess {
    /// `on_line` is called from reader threads for every output line;
    /// `on_exit` once, with `Stopped` or `Crashed`, when the process ends
    /// (after its output has been delivered, or after a short grace period
    /// if something the game started still holds its output open).
    pub fn start(
        godot: &Path,
        project: &Path,
        window_hint: Option<WindowHint>,
        on_line: impl FnMut(GameOutputLine) + Send + 'static,
        on_exit: impl FnOnce(GameState) + Send + 'static,
    ) -> Result<Self> {
        let args = window_hint.map(|h| h.args()).unwrap_or_default();
        Self::start_with_args(godot, project, &args, on_line, on_exit)
    }

    /// Like [`GameProcess::start`], with extra Godot arguments placed after
    /// `--path <project>` (e.g. `--headless --quit-after 30` for the boot
    /// check and for tests on machines without a display).
    pub fn start_with_args(
        godot: &Path,
        project: &Path,
        extra_args: &[String],
        on_line: impl FnMut(GameOutputLine) + Send + 'static,
        on_exit: impl FnOnce(GameState) + Send + 'static,
    ) -> Result<Self> {
        let mut command = Command::new(godot);
        command
            .arg("--path")
            .arg(project)
            .args(extra_args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Its own process group, so `stop()` also ends anything the game
        // started. TODO(windows): assign the child to a job object with
        // JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE for the same effect; for now
        // only the Godot process itself is killed there.
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        let mut child = command
            .spawn()
            .with_context(|| format!("couldn't start Godot at {}", godot.display()))?;

        let stdout = child.stdout.take().expect("stdout was piped");
        let stderr = child.stderr.take().expect("stderr was piped");
        let on_line = Arc::new(Mutex::new(on_line));
        let (done_tx, done_rx) = mpsc::channel();
        spawn_reader(
            stdout,
            OutputStream::Stdout,
            on_line.clone(),
            done_tx.clone(),
        );
        spawn_reader(stderr, OutputStream::Stderr, on_line, done_tx);

        let child = Arc::new(Mutex::new(child));
        let stop_requested = Arc::new(AtomicBool::new(false));

        let watch_child = child.clone();
        let watch_stop = stop_requested.clone();
        thread::spawn(move || {
            // Poll rather than block in `wait()` while holding the lock, so
            // `stop()` can always get in to kill the process.
            let status = loop {
                match watch_child.lock().unwrap().try_wait() {
                    Ok(Some(status)) => break Some(status),
                    Ok(None) => {}
                    Err(_) => break None,
                }
                thread::sleep(Duration::from_millis(50));
            };
            // Let the readers deliver the last lines, but never wait on a
            // pipe some leftover process keeps open.
            let deadline = Instant::now() + DRAIN_GRACE;
            for _ in 0..2 {
                let left = deadline.saturating_duration_since(Instant::now());
                if done_rx.recv_timeout(left).is_err() {
                    break;
                }
            }
            let state = if watch_stop.load(Ordering::SeqCst) || status.is_some_and(|s| s.success())
            {
                GameState::Stopped
            } else {
                GameState::Crashed
            };
            on_exit(state);
        });

        Ok(Self {
            child,
            stop_requested,
            group_killed: false,
        })
    }

    /// Kills the game (and, on Unix, every process it started) and waits for
    /// it to exit. `on_exit` then reports `Stopped`. Doesn't wait for the
    /// reader threads, so it's safe to call while holding a lock that
    /// `on_line`/`on_exit` also take.
    pub fn stop(&mut self) -> Result<()> {
        let mut child = self.child.lock().unwrap();
        if child.try_wait()?.is_some() {
            // Already exited on its own: leave its Stopped/Crashed as is,
            // but still end anything it left running.
            kill_group(&mut self.group_killed, child.id());
            return Ok(());
        }
        // Set while holding the lock, so the watcher can't observe the exit
        // before it sees the flag.
        self.stop_requested.store(true, Ordering::SeqCst);
        kill_group(&mut self.group_killed, child.id());
        // Kill can race a natural exit; either way `wait` reaps it.
        let _ = child.kill();
        child.wait().context("couldn't wait for Godot to exit")?;
        Ok(())
    }

    pub fn is_running(&mut self) -> bool {
        matches!(self.child.lock().unwrap().try_wait(), Ok(None))
    }

    /// The exit status once the process has ended, `None` while it runs.
    pub fn exit_status(&mut self) -> Option<ExitStatus> {
        self.child.lock().unwrap().try_wait().ok().flatten()
    }

    /// The OS process id, for diagnostics.
    pub fn pid(&self) -> u32 {
        self.child.lock().unwrap().id()
    }
}

impl Drop for GameProcess {
    /// Never leave an orphaned game window behind.
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

/// SIGKILLs the game's process group (its id is the game's pid, from
/// `process_group(0)`), at most once per game.
#[cfg(unix)]
fn kill_group(already_killed: &mut bool, pid: u32) {
    if std::mem::replace(already_killed, true) {
        return;
    }
    // SAFETY: killpg only sends a signal; the id is our own child's group,
    // created at spawn. An already-empty group just returns ESRCH.
    unsafe {
        libc::killpg(pid as libc::pid_t, libc::SIGKILL);
    }
}

#[cfg(not(unix))]
fn kill_group(already_killed: &mut bool, _pid: u32) {
    *already_killed = true;
}

/// Reads `stream` line by line on its own thread, calling `on_line` for
/// each, then signals `done` at EOF.
fn spawn_reader<R, F>(stream: R, kind: OutputStream, on_line: Arc<Mutex<F>>, done: mpsc::Sender<()>)
where
    R: Read + Send + 'static,
    F: FnMut(GameOutputLine) + Send + 'static,
{
    thread::spawn(move || {
        read_lines(stream, |text| {
            (on_line.lock().unwrap())(GameOutputLine { stream: kind, text })
        });
        let _ = done.send(());
    });
}

/// Splits `stream` into lines (without `\n`/`\r\n`), holding at most
/// `MAX_LINE_BYTES` of any one line: a longer line is emitted cut, with
/// `TRUNCATED_MARKER`, and the rest of it is skipped. A final line without
/// a newline is emitted at EOF.
fn read_lines(stream: impl Read, mut emit: impl FnMut(String)) {
    let mut reader = BufReader::new(stream);
    let mut line: Vec<u8> = Vec::new();
    // True while skipping the remainder of an already-truncated line.
    let mut skipping = false;
    let finish = |line: &mut Vec<u8>, emit: &mut dyn FnMut(String)| {
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        // Game output isn't guaranteed UTF-8; don't drop lines.
        emit(String::from_utf8_lossy(line).into_owned());
        line.clear();
    };
    loop {
        let available = match reader.fill_buf() {
            Ok([]) => break,
            Ok(buf) => buf,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        };
        let newline = available.iter().position(|&b| b == b'\n');
        let chunk = &available[..newline.unwrap_or(available.len())];
        if !skipping {
            let room = MAX_LINE_BYTES - line.len();
            if chunk.len() > room {
                line.extend_from_slice(&chunk[..room]);
                let mut text = String::from_utf8_lossy(&line).into_owned();
                text.push_str(TRUNCATED_MARKER);
                emit(text);
                line.clear();
                skipping = true;
            } else {
                line.extend_from_slice(chunk);
            }
        }
        let consumed = match newline {
            Some(i) => {
                if !skipping {
                    finish(&mut line, &mut emit);
                }
                skipping = false;
                i + 1
            }
            None => available.len(),
        };
        reader.consume(consumed);
    }
    if !skipping && !line.is_empty() {
        finish(&mut line, &mut emit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use crate::godot::test_support::fake_binary;
    use crate::godot::test_support::{fixture_project, real_godot};

    fn collect_lines(input: &[u8]) -> Vec<String> {
        let mut lines = Vec::new();
        read_lines(input, |l| lines.push(l));
        lines
    }

    #[test]
    fn splits_lines_and_keeps_an_unterminated_last_line() {
        assert_eq!(
            collect_lines(b"one\r\n\ntwo\nthree"),
            vec!["one", "", "two", "three"]
        );
        assert_eq!(collect_lines(b""), Vec::<String>::new());
        assert_eq!(collect_lines(&[0xff, b'x', b'\n']), vec!["\u{fffd}x"]);
    }

    /// A game printing an endless line without a newline used to be
    /// buffered in full; now it's cut at MAX_LINE_BYTES and reading goes on.
    #[test]
    fn an_overlong_line_is_truncated_and_the_rest_skipped() {
        let mut input = vec![b'a'; MAX_LINE_BYTES * 3 + 17];
        input.extend_from_slice(b"\nnext\n");
        input.extend_from_slice(&vec![b'b'; MAX_LINE_BYTES]);
        input.extend_from_slice(b"\n");
        input.extend_from_slice(&vec![b'c'; MAX_LINE_BYTES + 1]);

        let lines = collect_lines(&input);
        assert_eq!(
            lines.len(),
            4,
            "{:?}",
            lines.iter().map(String::len).collect::<Vec<_>>()
        );
        assert_eq!(
            lines[0],
            format!("{}{TRUNCATED_MARKER}", "a".repeat(MAX_LINE_BYTES))
        );
        assert_eq!(lines[1], "next");
        // Exactly at the limit isn't truncated.
        assert_eq!(lines[2], "b".repeat(MAX_LINE_BYTES));
        // An overlong line cut off by EOF is emitted once, truncated.
        assert_eq!(
            lines[3],
            format!("{}{TRUNCATED_MARKER}", "c".repeat(MAX_LINE_BYTES))
        );
    }

    /// True once `pid` no longer runs (gone, or a zombie nobody reaped).
    #[cfg(unix)]
    fn process_gone(pid: i32) -> bool {
        // SAFETY: signal 0 only checks for existence.
        if unsafe { libc::kill(pid, 0) } != 0 {
            return true;
        }
        std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .map(|s| {
                s.rsplit(')')
                    .next()
                    .is_some_and(|rest| rest.trim_start().starts_with('Z'))
            })
            .unwrap_or(false)
    }

    #[cfg(unix)]
    fn wait_until(timeout: Duration, mut done: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if done() {
                return true;
            }
            thread::sleep(Duration::from_millis(50));
        }
        done()
    }

    /// A game that started a background process (which inherits its output
    /// pipes) used to leave `on_exit` unfired after `stop()`, and the
    /// background process running.
    #[cfg(unix)]
    #[test]
    fn stop_kills_processes_the_game_started_and_still_reports_the_exit() {
        let dir = tempfile::tempdir().unwrap();
        let fake = fake_binary(dir.path(), "godot", "sleep 30 &\necho $!\nsleep 30");
        let (tx, rx) = mpsc::channel();
        let (exit_tx, exit_rx) = mpsc::channel();
        let mut game = GameProcess::start(
            &fake,
            dir.path(),
            None,
            move |line| {
                let _ = tx.send(line);
            },
            move |state| exit_tx.send(state).unwrap(),
        )
        .unwrap();
        let background: i32 = rx
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .text
            .parse()
            .unwrap();
        assert!(!process_gone(background));

        let stopped_at = Instant::now();
        game.stop().unwrap();
        let state = exit_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("on_exit should fire after stop()");
        assert_eq!(state, GameState::Stopped);
        assert!(
            stopped_at.elapsed() < Duration::from_millis(2500),
            "{:?}",
            stopped_at.elapsed()
        );
        assert!(
            wait_until(Duration::from_secs(2), || process_gone(background)),
            "background process {background} survived stop()"
        );
    }

    /// Same leftover-process case, but the game exits by itself: `on_exit`
    /// fires after the grace period, and dropping the handle cleans up.
    #[cfg(unix)]
    #[test]
    fn a_natural_exit_is_reported_even_if_a_leftover_process_holds_the_pipes() {
        let dir = tempfile::tempdir().unwrap();
        let fake = fake_binary(dir.path(), "godot", "sleep 30 &\necho $!\nexit 0");
        let (tx, rx) = mpsc::channel();
        let (exit_tx, exit_rx) = mpsc::channel();
        let started = Instant::now();
        let game = GameProcess::start(
            &fake,
            dir.path(),
            None,
            move |line| {
                let _ = tx.send(line);
            },
            move |state| exit_tx.send(state).unwrap(),
        )
        .unwrap();
        let state = exit_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("on_exit should fire");
        assert_eq!(state, GameState::Stopped);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
        // Its output was still delivered before on_exit.
        let background: i32 = rx.try_recv().unwrap().text.parse().unwrap();
        assert!(!process_gone(background));

        drop(game);
        assert!(
            wait_until(Duration::from_secs(2), || process_gone(background)),
            "background process {background} survived drop"
        );
    }

    #[test]
    fn window_hint_uses_the_flags_from_godot_help() {
        let help = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/godot/help.txt"),
        )
        .unwrap();
        for flag in [
            "--windowed",
            "--position <X>,<Y>",
            "--resolution <W>x<H>",
            "--path <directory>",
        ] {
            assert!(help.contains(flag), "{flag} missing from real --help");
        }
        let hint = WindowHint {
            x: -20,
            y: 40,
            width: 800,
            height: 600,
        };
        assert_eq!(
            hint.args(),
            vec![
                "--windowed",
                "--position",
                "-20,40",
                "--resolution",
                "800x600"
            ]
        );
    }

    #[test]
    fn a_missing_binary_is_a_clear_error() {
        let err = GameProcess::start(
            Path::new("/definitely/not/godot"),
            Path::new("."),
            None,
            |_| {},
            |_| {},
        )
        .err()
        .expect("should fail");
        assert!(err.to_string().contains("couldn't start Godot"), "{err}");
    }

    #[test]
    #[ignore = "needs a real Godot (INFINABOX_GODOT); run with --ignored"]
    fn runs_a_real_project_to_completion_and_streams_its_output() {
        let godot = real_godot();
        let project = fixture_project("runtime_error");
        let (tx, rx) = mpsc::channel();
        let (exit_tx, exit_rx) = mpsc::channel();
        let args: Vec<String> = ["--headless", "--quit-after", "30"]
            .map(String::from)
            .into();
        let _game = GameProcess::start_with_args(
            &godot,
            project.path(),
            &args,
            move |line| tx.send(line).unwrap(),
            move |state| exit_tx.send(state).unwrap(),
        )
        .unwrap();

        let state = exit_rx
            .recv_timeout(Duration::from_secs(60))
            .expect("game should exit");
        // Godot exits 0 even when a script fails.
        assert_eq!(state, GameState::Stopped);
        let lines: Vec<GameOutputLine> = rx.try_iter().collect();
        assert!(lines.contains(&GameOutputLine {
            stream: OutputStream::Stdout,
            text: "starting".into()
        }));
        assert!(lines.iter().any(|l| l.stream == OutputStream::Stderr
            && l.text == "SCRIPT ERROR: Invalid call. Nonexistent function 'jump' in base 'Nil'."));
    }

    #[test]
    #[ignore = "needs a real Godot (INFINABOX_GODOT); run with --ignored"]
    fn stops_a_real_running_project() {
        let godot = real_godot();
        let project = fixture_project("clean");
        let (tx, rx) = mpsc::channel();
        let (exit_tx, exit_rx) = mpsc::channel();
        // No --quit-after: runs until stopped.
        let args: Vec<String> = vec!["--headless".into()];
        let mut game = GameProcess::start_with_args(
            &godot,
            project.path(),
            &args,
            move |line| {
                let _ = tx.send(line);
            },
            move |state| exit_tx.send(state).unwrap(),
        )
        .unwrap();

        // Wait for the project's own ready line so we know it's really running.
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let line = rx
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("never saw the ready line");
            if line.text == "[infinabox] ready 1" {
                break;
            }
        }
        assert!(game.is_running());

        game.stop().unwrap();
        assert!(!game.is_running());
        let state = exit_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("on_exit should fire");
        assert_eq!(state, GameState::Stopped);
        // Stopping twice is fine.
        game.stop().unwrap();
    }

    #[test]
    #[ignore = "needs a real Godot (INFINABOX_GODOT); run with --ignored"]
    fn a_non_zero_exit_is_reported_as_crashed() {
        let godot = real_godot();
        let (exit_tx, exit_rx) = mpsc::channel();
        // A real Godot failure: it prints "Invalid project path specified"
        // and exits 1.
        let _game = GameProcess::start_with_args(
            &godot,
            Path::new("/nonexistent/infinabox/project"),
            &["--headless".to_string()],
            |_| {},
            move |state| exit_tx.send(state).unwrap(),
        )
        .unwrap();
        let state = exit_rx.recv_timeout(Duration::from_secs(60)).unwrap();
        assert_eq!(state, GameState::Crashed);
    }
}
