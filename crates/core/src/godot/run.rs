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
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

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

pub struct GameProcess {
    child: Arc<Mutex<Child>>,
    /// Set by `stop()` so the exit watcher reports `Stopped`, not `Crashed`,
    /// for a process we killed ourselves.
    stop_requested: Arc<AtomicBool>,
}

impl GameProcess {
    /// `on_line` is called from reader threads for every output line;
    /// `on_exit` once, with `Stopped` or `Crashed`, when the process ends
    /// (after every output line has been delivered).
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
        let mut child = Command::new(godot)
            .arg("--path")
            .arg(project)
            .args(extra_args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("couldn't start Godot at {}", godot.display()))?;

        let stdout = child.stdout.take().expect("stdout was piped");
        let stderr = child.stderr.take().expect("stderr was piped");
        let on_line = Arc::new(Mutex::new(on_line));
        let readers = [
            spawn_reader(stdout, OutputStream::Stdout, on_line.clone()),
            spawn_reader(stderr, OutputStream::Stderr, on_line),
        ];

        let child = Arc::new(Mutex::new(child));
        let stop_requested = Arc::new(AtomicBool::new(false));

        let watch_child = child.clone();
        let watch_stop = stop_requested.clone();
        thread::spawn(move || {
            // Pipes hit EOF when the process exits; join first so every line
            // is delivered before `on_exit`.
            for reader in readers {
                let _ = reader.join();
            }
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
        })
    }

    /// Kills the game and waits for it to exit. `on_exit` then reports
    /// `Stopped`. Doesn't wait for the reader threads, so it's safe to call
    /// while holding a lock that `on_line`/`on_exit` also take.
    pub fn stop(&mut self) -> Result<()> {
        let mut child = self.child.lock().unwrap();
        if child.try_wait()?.is_some() {
            // Already exited on its own: leave its Stopped/Crashed as is.
            return Ok(());
        }
        // Set while holding the lock, so the watcher can't observe the exit
        // before it sees the flag.
        self.stop_requested.store(true, Ordering::SeqCst);
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

fn spawn_reader<R, F>(
    stream: R,
    kind: OutputStream,
    on_line: Arc<Mutex<F>>,
) -> thread::JoinHandle<()>
where
    R: Read + Send + 'static,
    F: FnMut(GameOutputLine) + Send + 'static,
{
    thread::spawn(move || {
        let mut reader = BufReader::new(stream);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    // Game output isn't guaranteed UTF-8; don't drop lines.
                    let text = String::from_utf8_lossy(&buf)
                        .trim_end_matches(['\n', '\r'])
                        .to_string();
                    (on_line.lock().unwrap())(GameOutputLine { stream: kind, text });
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::godot::test_support::{fixture_project, real_godot};
    use std::sync::mpsc;
    use std::time::Instant;

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
