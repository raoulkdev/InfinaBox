//! Godot install/status and running the game from Studio's Play panel.
//! Owns the single running `GameProcess`, feeds its output through the
//! error parser, and emits `game-state` / `game-output` / `game-error`.
//!
//! Split the same way `terminal.rs`/`watcher.rs` are: `GameManager` holds
//! all the game-running logic and reports through a `GameHost` trait (which
//! Godot binary to use, where to put the window, where events go), so it's
//! unit-tested against real child processes without an `AppHandle`. The
//! `#[tauri::command]`s and the `run_game`/`stop_game`/`restart_if_running`
//! entry points are thin wrappers that plug in `AppHost`, which emits Tauri
//! events. The bridge (`bridge.rs`) drives the same `GameManager`.
//!
//! Locking: `run_lock` serialises whole runs (stop old game, install addon,
//! import, start), which can take seconds. `inner` is only ever held
//! briefly, so status/errors queries and `stop` never wait on an import:
//! stopping during one bumps `generation`, and the in-flight run notices
//! and gives up before starting anything.

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::thread;
use std::time::{Duration, Instant};

use infinabox_core::godot::errors::{ErrorParser, RecentLog};
use infinabox_core::godot::run::{GameProcess, WindowHint};
use infinabox_core::godot::{
    install, locate, validate, GameError, GameOutputLine, GameState, GodotStatus, OutputStream,
};
use infinabox_core::scaffold;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

/// Event names emitted by this module (frontend: `src/lib/studio-api.ts`).
pub const EVENT_INSTALL_PROGRESS: &str = "godot-install-progress";
pub const EVENT_GAME_STATE: &str = "game-state";
pub const EVENT_GAME_OUTPUT: &str = "game-output";
pub const EVENT_GAME_ERROR: &str = "game-error";

/// Shown when Play is pressed on a project without a `project.godot`
/// (e.g. an old-format InfinaBox project).
pub const NOT_A_GODOT_GAME: &str = "This project isn't a Godot game yet.";
pub const GODOT_NOT_INSTALLED: &str =
    "Godot isn't installed yet. Install it from the Play panel, then try again.";
pub const STOPPED_WHILE_STARTING: &str = "The game was stopped before it finished starting.";

/// How long stderr must be quiet before a half-finished error block is
/// flushed (Godot only ends a block by starting the next line).
const STDERR_QUIET: Duration = Duration::from_millis(300);
const FLUSH_POLL: Duration = Duration::from_millis(100);
/// Recent history kept for `game_recent_errors` and the bridge.
const MAX_RECENT_LINES: usize = 2000;
const MAX_RECENT_ERRORS: usize = 200;
/// Past this many files, `fingerprint` gives up and the project is always
/// imported before a run (correct, just slower).
const FINGERPRINT_MAX_ENTRIES: usize = 100_000;

#[derive(Serialize, Clone)]
pub struct GameStatePayload {
    pub state: GameState,
}

/// How to start Godot: the normal windowed game (optionally placed beside
/// InfinaBox), or with explicit extra arguments (tests run `--headless`).
#[derive(Clone, Debug)]
pub enum Launch {
    Window(Option<WindowHint>),
    // Only tests use this in Phase A (no display in CI-like environments).
    #[cfg_attr(not(test), allow(dead_code))]
    Args(Vec<String>),
}

/// Everything `GameManager` needs from the outside world.
pub trait GameHost: Send + Sync + 'static {
    /// The Godot binary to run, or a readable reason there isn't one.
    fn godot(&self) -> Result<PathBuf, String>;
    fn launch(&self) -> Launch;
    fn on_state(&self, state: GameState);
    fn on_output(&self, line: &GameOutputLine);
    fn on_error(&self, error: &GameError);
}

struct Inner {
    process: Option<GameProcess>,
    project_path: Option<String>,
    state: GameState,
    log: RecentLog,
    parser: ErrorParser,
    /// Bumped whenever the current run is replaced or stopped, so late
    /// callbacks from an old process (and an in-flight start) are ignored.
    generation: u64,
    last_stderr: Instant,
    /// This run's errors already reported by the import step, so the same
    /// parse error isn't listed twice when the game hits it again.
    import_errors: HashSet<String>,
    /// Project fingerprint after its last successful import, per project.
    imported: HashMap<PathBuf, u64>,
}

impl Inner {
    fn set_state(&mut self, state: GameState, host: &dyn GameHost) {
        self.state = state;
        host.on_state(state);
    }

    fn record_error(&mut self, error: GameError, host: &dyn GameHost) {
        self.log.push_error(error.clone());
        host.on_error(&error);
    }

    fn record_line(&mut self, mut line: GameOutputLine, host: &dyn GameHost) {
        line.text = strip_ansi(&line.text);
        if line.stream == OutputStream::Stderr {
            self.last_stderr = Instant::now();
        }
        // Errors completed by this line came before it.
        for error in self.parser.push(&line) {
            if !self.import_errors.contains(&error.raw) {
                self.record_error(error, host);
            }
        }
        self.log.push_line(line.clone());
        host.on_output(&line);
    }

    /// Emits an error block still waiting for its continuation lines.
    fn flush(&mut self, host: &dyn GameHost) {
        for error in self.parser.finish() {
            if !self.import_errors.contains(&error.raw) {
                self.record_error(error, host);
            }
        }
    }

    /// Ends the current run: ignores its later callbacks, flushes its last
    /// error, and kills the process. True if a game was starting or running.
    /// (`GameProcess::stop` is documented safe to call under this lock.)
    fn stop_current(&mut self, host: &dyn GameHost) -> Result<bool, String> {
        self.generation += 1;
        let was_live = matches!(self.state, GameState::Starting | GameState::Running);
        self.flush(host);
        if let Some(mut process) = self.process.take() {
            if let Err(e) = process.stop() {
                // The process is out of our hands either way (dropping it
                // retries the kill), so don't keep reporting "running".
                if was_live {
                    self.set_state(GameState::Stopped, host);
                }
                return Err(format!("Couldn't stop the running game: {e:#}"));
            }
        }
        Ok(was_live)
    }
}

/// The one game this app runs at a time.
pub struct GameManager {
    inner: Mutex<Inner>,
    run_lock: Mutex<()>,
}

impl Default for GameManager {
    fn default() -> Self {
        GameManager {
            inner: Mutex::new(Inner {
                process: None,
                project_path: None,
                state: GameState::Stopped,
                log: RecentLog::new(MAX_RECENT_LINES, MAX_RECENT_ERRORS),
                parser: ErrorParser::new(),
                generation: 0,
                last_stderr: Instant::now(),
                import_errors: HashSet::new(),
                imported: HashMap::new(),
            }),
            run_lock: Mutex::new(()),
        }
    }
}

/// Locks ignoring poisoning: a panicking callback shouldn't take the Play
/// panel down with it for the rest of the session.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl GameManager {
    fn inner(&self) -> MutexGuard<'_, Inner> {
        lock(&self.inner)
    }

    pub fn state(&self) -> GameState {
        self.inner().state
    }

    /// The current state and the project it's about (the last one run).
    pub fn status(&self) -> (GameState, Option<String>) {
        let inner = self.inner();
        (inner.state, inner.project_path.clone())
    }

    pub fn recent_errors(&self, limit: usize) -> Vec<GameError> {
        self.inner().log.recent_errors(limit)
    }

    pub fn recent_output(&self, lines: usize) -> Vec<GameOutputLine> {
        self.inner().log.recent_lines(lines)
    }

    /// Starts `project`'s game, stopping whatever was running first. Returns
    /// once the game process has started (after installing the addon and,
    /// when files changed, importing assets), or with a readable error.
    pub fn run(self: &Arc<Self>, host: &Arc<dyn GameHost>, project: &str) -> Result<(), String> {
        check_project(project)?;
        let _run = lock(&self.run_lock);
        let godot = host.godot()?;
        self.run_locked(host, &godot, project)
    }

    /// Stops the game. Stopping when nothing runs is a no-op, not an error:
    /// either way no game is running afterwards.
    pub fn stop(&self, host: &Arc<dyn GameHost>) -> Result<(), String> {
        let mut inner = self.inner();
        if inner.stop_current(host.as_ref())? {
            inner.set_state(GameState::Stopped, host.as_ref());
        }
        Ok(())
    }

    /// Restarts the current game if one is starting or running; `Ok(false)`
    /// (and nothing happens) otherwise. Waits for an in-flight start first.
    pub fn restart_if_running(self: &Arc<Self>, host: &Arc<dyn GameHost>) -> Result<bool, String> {
        if self.live_project().is_none() {
            return Ok(false);
        }
        let _run = lock(&self.run_lock);
        // Re-checked under the run lock: it may have stopped meanwhile.
        let Some(project) = self.live_project() else {
            return Ok(false);
        };
        check_project(&project)?;
        let godot = host.godot()?;
        self.run_locked(host, &godot, &project).map(|()| true)
    }

    fn live_project(&self) -> Option<String> {
        let inner = self.inner();
        match inner.state {
            GameState::Starting | GameState::Running => inner.project_path.clone(),
            GameState::Stopped | GameState::Crashed => None,
        }
    }

    /// `run` with `run_lock` held.
    fn run_locked(
        self: &Arc<Self>,
        host: &Arc<dyn GameHost>,
        godot: &Path,
        project: &str,
    ) -> Result<(), String> {
        let generation = {
            let mut inner = self.inner();
            inner.stop_current(host.as_ref())?;
            inner.generation += 1;
            inner.log.clear();
            inner.parser = ErrorParser::new();
            inner.import_errors.clear();
            inner.project_path = Some(project.to_string());
            inner.set_state(GameState::Starting, host.as_ref());
            inner.generation
        };
        let result = self.prepare_and_start(host, godot, Path::new(project), generation);
        if result.is_err() {
            let mut inner = self.inner();
            // Unless a stop or a newer run already took over the state.
            if inner.generation == generation {
                inner.flush(host.as_ref());
                inner.set_state(GameState::Stopped, host.as_ref());
            }
        }
        result
    }

    fn prepare_and_start(
        self: &Arc<Self>,
        host: &Arc<dyn GameHost>,
        godot: &Path,
        project: &Path,
        generation: u64,
    ) -> Result<(), String> {
        scaffold::ensure_addon(project)
            .map_err(|e| format!("Couldn't install the InfinaBox addon into the project: {e:#}"))?;

        // `godot --path` never imports new assets itself (a new icon.png
        // fails with "No loader found for resource"), so import whenever
        // the project's files changed since the last import.
        let key = project.to_path_buf();
        let before = fingerprint(project);
        let up_to_date = before.is_some() && self.inner().imported.get(&key) == before.as_ref();
        if !up_to_date {
            // Not cancellable (core's `import_assets` runs to completion): a
            // Stop meanwhile only keeps the game from starting afterwards.
            let errors = validate::import_assets(godot, project)
                .map_err(|e| format!("Godot couldn't import the project's assets: {e:#}"))?;
            // Importing writes `.import`/`.uid` files; fingerprint after.
            let after = fingerprint(project);
            let mut inner = self.inner();
            if inner.generation != generation {
                return Err(STOPPED_WHILE_STARTING.into());
            }
            for error in errors {
                let error = clean_error(error);
                inner.import_errors.insert(error.raw.clone());
                inner.record_error(error, host.as_ref());
            }
            match after {
                Some(fp) => inner.imported.insert(key, fp),
                None => inner.imported.remove(&key),
            };
        }

        let launch = host.launch();
        let mut inner = self.inner();
        if inner.generation != generation {
            return Err(STOPPED_WHILE_STARTING.into());
        }

        let weak = Arc::downgrade(self);
        let line_host = host.clone();
        let on_line = move |line: GameOutputLine| {
            let Some(manager) = weak.upgrade() else {
                return;
            };
            let mut inner = manager.inner();
            if inner.generation == generation {
                inner.record_line(line, line_host.as_ref());
            }
        };
        let weak = Arc::downgrade(self);
        let exit_host = host.clone();
        let on_exit = move |state: GameState| {
            let Some(manager) = weak.upgrade() else {
                return;
            };
            let mut inner = manager.inner();
            if inner.generation == generation {
                inner.flush(exit_host.as_ref());
                // Reaps it and ends anything it left running.
                inner.process = None;
                inner.set_state(state, exit_host.as_ref());
            }
        };
        let process = match launch {
            Launch::Window(hint) => GameProcess::start(godot, project, hint, on_line, on_exit),
            Launch::Args(args) => {
                GameProcess::start_with_args(godot, project, &args, on_line, on_exit)
            }
        }
        .map_err(|e| format!("{e:#}"))?;
        inner.process = Some(process);
        inner.set_state(GameState::Running, host.as_ref());
        drop(inner);

        self.spawn_flusher(host.clone(), generation);
        Ok(())
    }

    /// Flushes a pending error once stderr has been quiet for a moment, so
    /// the last error of a still-running game isn't held back. Ends with
    /// the run.
    fn spawn_flusher(self: &Arc<Self>, host: Arc<dyn GameHost>, generation: u64) {
        let weak: Weak<Self> = Arc::downgrade(self);
        thread::spawn(move || loop {
            thread::sleep(FLUSH_POLL);
            let Some(manager) = weak.upgrade() else {
                return;
            };
            let mut inner = manager.inner();
            if inner.generation != generation || inner.process.is_none() {
                return;
            }
            if inner.parser.has_pending() && inner.last_stderr.elapsed() >= STDERR_QUIET {
                inner.flush(host.as_ref());
            }
        });
    }
}

fn check_project(project: &str) -> Result<(), String> {
    let path = Path::new(project);
    if !path.is_absolute() {
        return Err(format!(
            "The project path must be absolute (got \"{project}\")."
        ));
    }
    if !path.join("project.godot").is_file() {
        return Err(NOT_A_GODOT_GAME.into());
    }
    Ok(())
}

/// A cheap summary of the project's files (paths, sizes, modified times),
/// to tell whether anything changed since the last import. Skips dot
/// entries (`.godot/` is Godot's own cache, `.git/`, `.ibproject/`), which
/// Godot doesn't import either. `None` if it couldn't be read or is huge.
fn fingerprint(project: &Path) -> Option<u64> {
    let mut hasher = DefaultHasher::new();
    let mut stack = vec![project.to_path_buf()];
    let mut seen = 0usize;
    while let Some(dir) = stack.pop() {
        let mut entries: Vec<_> = std::fs::read_dir(&dir)
            .ok()?
            .filter_map(Result::ok)
            .collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('.') {
                continue;
            }
            seen += 1;
            if seen > FINGERPRINT_MAX_ENTRIES {
                return None;
            }
            let path = entry.path();
            path.strip_prefix(project).ok()?.hash(&mut hasher);
            let file_type = entry.file_type().ok()?;
            if file_type.is_dir() {
                // Not following symlinked folders keeps this loop-free.
                stack.push(path);
            } else if let Ok(meta) = std::fs::metadata(&path) {
                meta.len().hash(&mut hasher);
                meta.modified().ok().hash(&mut hasher);
            }
        }
    }
    Some(hasher.finish())
}

/// Removes ANSI escape sequences (colours, cursor moves). Godot's
/// `--import` output has them even when piped.
pub fn strip_ansi(text: &str) -> String {
    if !text.contains('\u{1b}') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.next() {
            // CSI: parameter/intermediate bytes, then one final byte.
            Some('[') => {
                for n in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&n) {
                        break;
                    }
                }
            }
            // OSC: up to BEL or ESC \.
            Some(']') => {
                while let Some(n) = chars.next() {
                    if n == '\u{7}' {
                        break;
                    }
                    if n == '\u{1b}' {
                        if chars.peek() == Some(&'\\') {
                            chars.next();
                        }
                        break;
                    }
                }
            }
            // Any other two-character escape, or a trailing ESC.
            _ => {}
        }
    }
    out
}

fn clean_error(error: GameError) -> GameError {
    GameError {
        message: strip_ansi(&error.message),
        file: error.file.map(|f| strip_ansi(&f)),
        line: error.line,
        raw: strip_ansi(&error.raw),
    }
}

/// A screen rectangle in physical pixels.
#[derive(Clone, Copy, Debug)]
struct Rect {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

const HINT_GAP: i64 = 16;
const HINT_MIN_WIDTH: i64 = 480;
const HINT_MIN_HEIGHT: i64 = 270;
const HINT_MAX_WIDTH: i64 = 1280;

/// Places the game window beside the InfinaBox window, on whichever side
/// of it has more room on its monitor, 16:9 and top-aligned with it.
/// `None` when there isn't room for a usable window (e.g. InfinaBox is
/// maximised); Godot then uses its own default placement.
fn hint_beside(window: Rect, monitor: Rect) -> Option<WindowHint> {
    let (wx, wy, ww) = (window.x as i64, window.y as i64, window.width as i64);
    let (mx, my, mw, mh) = (
        monitor.x as i64,
        monitor.y as i64,
        monitor.width as i64,
        monitor.height as i64,
    );
    let right = (mx + mw) - (wx + ww) - 2 * HINT_GAP;
    let left = wx - mx - 2 * HINT_GAP;
    let space = right.max(left);
    if space < HINT_MIN_WIDTH {
        return None;
    }
    let y = wy.clamp(my, my + mh);
    let mut width = space.min(HINT_MAX_WIDTH);
    let mut height = width * 9 / 16;
    let room_below = my + mh - y - HINT_GAP;
    if height > room_below {
        height = room_below;
        width = height * 16 / 9;
    }
    if width < HINT_MIN_WIDTH || height < HINT_MIN_HEIGHT {
        return None;
    }
    let x = if right >= left {
        wx + ww + HINT_GAP
    } else {
        wx - HINT_GAP - width
    };
    Some(WindowHint {
        x: x as i32,
        y: y as i32,
        width: width as u32,
        height: height as u32,
    })
}

/// Tauri-managed state: at most one running game at a time.
#[derive(Default)]
pub struct GodotState(pub Arc<GameManager>);

/// The app's `GameHost`: Godot from `locate` (managed install first), the
/// window beside the main window, events to the frontend.
pub struct AppHost(pub AppHandle);

impl AppHost {
    fn window_hint(&self) -> Option<WindowHint> {
        let window = self.0.get_webview_window("main")?;
        if window.is_minimized().unwrap_or(false) {
            return None;
        }
        let pos = window.outer_position().ok()?;
        let size = window.outer_size().ok()?;
        let monitor = window.current_monitor().ok()??;
        hint_beside(
            Rect {
                x: pos.x,
                y: pos.y,
                width: size.width,
                height: size.height,
            },
            Rect {
                x: monitor.position().x,
                y: monitor.position().y,
                width: monitor.size().width,
                height: monitor.size().height,
            },
        )
    }
}

impl GameHost for AppHost {
    fn godot(&self) -> Result<PathBuf, String> {
        let status = locate::status(&app_data_dir(&self.0)?);
        status
            .path
            .filter(|_| status.installed)
            .ok_or_else(|| GODOT_NOT_INSTALLED.into())
    }

    fn launch(&self) -> Launch {
        Launch::Window(self.window_hint())
    }

    fn on_state(&self, state: GameState) {
        let _ = self.0.emit(EVENT_GAME_STATE, GameStatePayload { state });
    }

    fn on_output(&self, line: &GameOutputLine) {
        let _ = self.0.emit(EVENT_GAME_OUTPUT, line);
    }

    fn on_error(&self, error: &GameError) {
        let _ = self.0.emit(EVENT_GAME_ERROR, error);
    }
}

fn app_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|e| format!("Couldn't find the app data folder: {e}"))
}

/// The app's game manager (shared with the bridge).
pub fn manager(app: &AppHandle) -> Arc<GameManager> {
    app.state::<GodotState>().0.clone()
}

pub fn app_host(app: &AppHandle) -> Arc<dyn GameHost> {
    Arc::new(AppHost(app.clone()))
}

/// Restarts the game if it's currently running (used after an AI turn or a
/// snapshot restore changed the project). No-op otherwise.
///
/// Blocking: a restart re-imports changed assets and can take seconds, so
/// call it from a background thread, without holding other locks. If a run
/// is still starting, it waits for that and then restarts it.
pub fn restart_if_running(app: &AppHandle) -> Result<(), String> {
    manager(app).restart_if_running(&app_host(app)).map(|_| ())
}

/// Starts (or restarts) the game for `project_path`. Shared by the
/// `game_run` command and the bridge's `RunGame`. Blocks until the game
/// process has started or failed to.
pub fn run_game(app: &AppHandle, project_path: &str) -> Result<(), String> {
    manager(app).run(&app_host(app), project_path)
}

pub fn stop_game(app: &AppHandle) -> Result<(), String> {
    manager(app).stop(&app_host(app))
}

// Every command here is `async` (or `(async)` for the sync ones), so none
// runs on the main thread: most spawn or kill processes (`godot --version`,
// `--import`, the game), and even the state reads take a lock a stop can
// briefly hold.

#[tauri::command(async)]
pub fn godot_status(app: AppHandle) -> Result<GodotStatus, String> {
    Ok(locate::status(&app_data_dir(&app)?))
}

#[tauri::command]
pub async fn godot_install(app: AppHandle) -> Result<GodotStatus, String> {
    let app_data = app_data_dir(&app)?;
    let emitter = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let installed = install::install(&app_data, &mut |progress| {
            let _ = emitter.emit(EVENT_INSTALL_PROGRESS, progress);
        })
        .map_err(|e| format!("Couldn't install Godot: {e:#}"))?;
        let status = locate::status(&app_data);
        if status.installed && status.path.as_deref() == Some(installed.as_path()) {
            Ok(status)
        } else {
            Err(format!(
                "Godot was installed at {}, but it doesn't run on this computer.",
                installed.display()
            ))
        }
    })
    .await
    .map_err(|e| format!("The Godot install stopped unexpectedly: {e}"))?
}

#[tauri::command(async)]
pub fn game_run(app: AppHandle, project_path: String) -> Result<(), String> {
    run_game(&app, &project_path)
}

#[tauri::command(async)]
pub fn game_stop(app: AppHandle) -> Result<(), String> {
    stop_game(&app)
}

/// The game's current state, so a panel opening mid-run can start from the
/// truth instead of assuming `Stopped` (the `game-state` event only reports
/// changes).
#[tauri::command(async)]
pub fn game_status(app: AppHandle) -> Result<GameState, String> {
    Ok(manager(&app).state())
}

#[tauri::command(async)]
pub fn game_recent_errors(app: AppHandle, limit: usize) -> Result<Vec<GameError>, String> {
    Ok(manager(&app).recent_errors(limit))
}

/// Shared by this module's and `bridge.rs`'s tests: temp projects, fake
/// Godot binaries, and a `GameHost` that records every event.
#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use std::sync::Condvar;

    /// A temp folder removed on drop (src-tauri has no `tempfile`).
    pub struct TempDir(pub PathBuf);

    impl TempDir {
        pub fn new(tag: &str) -> Self {
            use std::sync::atomic::{AtomicU32, Ordering};
            static N: AtomicU32 = AtomicU32::new(0);
            let path = std::env::temp_dir().join(format!(
                "infinabox-{tag}-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::SeqCst)
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            TempDir(path)
        }

        pub fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A copy of a recorded fixture project from `crates/core`.
    pub fn fixture_project(name: &str) -> TempDir {
        let src = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../crates/core/tests/fixtures/godot/projects")
            .join(name);
        let dir = TempDir::new(name);
        for entry in std::fs::read_dir(&src).unwrap() {
            let entry = entry.unwrap();
            std::fs::copy(entry.path(), dir.path().join(entry.file_name())).unwrap();
        }
        dir
    }

    /// An executable `/bin/sh` script standing in for Godot. `$*` holds
    /// `--path <project> [args]`; `--import` runs get `import_body`.
    #[cfg(unix)]
    pub fn fake_godot(dir: &Path, import_body: &str, run_body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join("fake-godot");
        std::fs::write(
            &path,
            format!(
                "#!/bin/sh\ncase \"$*\" in\n*--import*)\n{import_body}\n;;\n*)\n{run_body}\n;;\nesac\n"
            ),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[derive(Clone, Debug, PartialEq)]
    pub enum Event {
        State(GameState),
        Output(GameOutputLine),
        Error(GameError),
    }

    pub struct TestHost {
        pub godot: Result<PathBuf, String>,
        pub args: Vec<String>,
        pub events: Mutex<Vec<Event>>,
        changed: Condvar,
    }

    impl TestHost {
        pub fn new(godot: Result<PathBuf, String>, args: &[&str]) -> Arc<Self> {
            Arc::new(TestHost {
                godot,
                args: args.iter().map(|s| s.to_string()).collect(),
                events: Mutex::new(Vec::new()),
                changed: Condvar::new(),
            })
        }

        pub fn events(&self) -> Vec<Event> {
            self.events.lock().unwrap().clone()
        }

        pub fn states(&self) -> Vec<GameState> {
            self.events()
                .into_iter()
                .filter_map(|e| match e {
                    Event::State(s) => Some(s),
                    _ => None,
                })
                .collect()
        }

        pub fn errors(&self) -> Vec<GameError> {
            self.events()
                .into_iter()
                .filter_map(|e| match e {
                    Event::Error(e) => Some(e),
                    _ => None,
                })
                .collect()
        }

        /// Waits until `done(events)` holds; panics with the events if not.
        pub fn wait_for(&self, timeout: Duration, done: impl Fn(&[Event]) -> bool) {
            let deadline = Instant::now() + timeout;
            let mut events = self.events.lock().unwrap();
            while !done(&events) {
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    panic!("timed out; events so far: {events:#?}");
                }
                events = self.changed.wait_timeout(events, left).unwrap().0;
            }
        }

        fn push(&self, event: Event) {
            self.events.lock().unwrap().push(event);
            self.changed.notify_all();
        }
    }

    impl GameHost for TestHost {
        fn godot(&self) -> Result<PathBuf, String> {
            self.godot.clone()
        }
        fn launch(&self) -> Launch {
            Launch::Args(self.args.clone())
        }
        fn on_state(&self, state: GameState) {
            self.push(Event::State(state));
        }
        fn on_output(&self, line: &GameOutputLine) {
            self.push(Event::Output(line.clone()));
        }
        fn on_error(&self, error: &GameError) {
            self.push(Event::Error(error.clone()));
        }
    }

    pub fn host_of(host: &Arc<TestHost>) -> Arc<dyn GameHost> {
        host.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;

    const WAIT: Duration = Duration::from_secs(20);

    fn has_state(state: GameState) -> impl Fn(&[Event]) -> bool {
        move |events| events.contains(&Event::State(state))
    }

    fn count(path: &Path) -> usize {
        std::fs::read_to_string(path)
            .map(|s| s.lines().count())
            .unwrap_or(0)
    }

    /// Waits for a fake Godot to have logged `n` lines to `path` (a run
    /// returns once the process is spawned, not once its script ran).
    fn wait_for_count(path: &Path, n: usize) {
        let deadline = Instant::now() + WAIT;
        while count(path) < n {
            assert!(
                Instant::now() < deadline,
                "{} never reached {n}",
                path.display()
            );
            thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(count(path), n);
    }

    #[test]
    fn strips_ansi_from_real_godot_import_output() {
        // Recorded from the real 4.7.2 `--headless --import`.
        let raw = "[   0% ] \u{1b}[90m\u{1b}[1mfirst_scan_filesystem\u{1b}[22m | Started Project \
                   initialization (5 steps)\u{1b}[39m\u{1b}[0m";
        assert_eq!(
            strip_ansi(raw),
            "[   0% ] first_scan_filesystem | Started Project initialization (5 steps)"
        );
        assert_eq!(
            strip_ansi("\u{1b}[92m[ DONE ]\u{1b}[39m \u{1b}[1mfirst_scan_filesystem\u{1b}[22m"),
            "[ DONE ] first_scan_filesystem"
        );
        assert_eq!(strip_ansi("\u{1b}]0;title\u{7}plain"), "plain");
        assert_eq!(strip_ansi("no escapes: ümlaut"), "no escapes: ümlaut");
        assert_eq!(strip_ansi("trailing\u{1b}"), "trailing");
    }

    fn rect(x: i32, y: i32, width: u32, height: u32) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn window_hint_goes_beside_the_app_window() {
        let monitor = rect(0, 0, 2560, 1440);
        // Room on the right.
        let hint = hint_beside(rect(100, 100, 1000, 800), monitor).unwrap();
        assert_eq!(
            (hint.x, hint.y, hint.width, hint.height),
            (1116, 100, 1280, 720)
        );
        // More room on the left.
        let hint = hint_beside(rect(1500, 50, 1000, 800), monitor).unwrap();
        assert_eq!(hint.x + hint.width as i32 + HINT_GAP as i32, 1500);
        assert!(hint.x >= 0);
        // Maximised: no room, no hint.
        assert!(hint_beside(rect(0, 0, 2560, 1440), monitor).is_none());
        // A second monitor at a negative offset.
        let hint = hint_beside(rect(-1900, 0, 1000, 800), rect(-1920, 0, 1920, 1080)).unwrap();
        assert_eq!(hint.x, -1900 + 1000 + 16);
        // Near the bottom: shrinks to fit rather than running off-screen.
        let hint = hint_beside(rect(0, 900, 800, 400), rect(0, 0, 2560, 1440)).unwrap();
        assert!(hint.y as u32 + hint.height <= 1440);
    }

    #[test]
    fn fingerprint_changes_with_project_files_but_not_hidden_ones() {
        let dir = TempDir::new("fingerprint");
        std::fs::write(dir.path().join("project.godot"), "x").unwrap();
        std::fs::create_dir_all(dir.path().join(".godot/imported")).unwrap();
        let first = fingerprint(dir.path()).unwrap();
        assert_eq!(fingerprint(dir.path()), Some(first));

        std::fs::write(dir.path().join(".godot/imported/cache"), "c").unwrap();
        std::fs::write(dir.path().join(".gitignore"), "g").unwrap();
        assert_eq!(fingerprint(dir.path()), Some(first));

        std::fs::create_dir_all(dir.path().join("art")).unwrap();
        std::fs::write(dir.path().join("art/icon.png"), "png").unwrap();
        let second = fingerprint(dir.path()).unwrap();
        assert_ne!(second, first);

        std::fs::write(dir.path().join("art/icon.png"), "bigger png").unwrap();
        assert_ne!(fingerprint(dir.path()).unwrap(), second);
    }

    #[test]
    fn a_project_without_project_godot_gets_a_plain_error_and_nothing_starts() {
        let dir = TempDir::new("not-godot");
        let host = TestHost::new(Err("should not be asked".into()), &[]);
        let manager = Arc::new(GameManager::default());
        let err = manager
            .run(&host_of(&host), dir.path().to_str().unwrap())
            .unwrap_err();
        assert_eq!(err, NOT_A_GODOT_GAME);
        assert_eq!(host.events(), vec![]);
        assert_eq!(manager.state(), GameState::Stopped);

        let err = manager.run(&host_of(&host), "relative/game").unwrap_err();
        assert!(err.contains("absolute"), "{err}");
    }

    #[test]
    fn missing_godot_is_reported_before_anything_starts() {
        let project = fixture_project("clean");
        let host = TestHost::new(Err(GODOT_NOT_INSTALLED.into()), &[]);
        let manager = Arc::new(GameManager::default());
        let err = manager
            .run(&host_of(&host), project.path().to_str().unwrap())
            .unwrap_err();
        assert_eq!(err, GODOT_NOT_INSTALLED);
        assert_eq!(host.events(), vec![]);
    }

    /// The full lifecycle against a real child process: addon installed,
    /// assets imported once, output streamed without colour codes, the
    /// last error flushed while the game keeps running, then stopped.
    #[cfg(unix)]
    #[test]
    fn runs_streams_flushes_errors_and_stops() {
        let tools = TempDir::new("tools");
        let imports = tools.path().join("imports");
        let godot = fake_godot(
            tools.path(),
            &format!("echo import >> '{}'", imports.display()),
            "printf '\\033[31mhello\\033[0m\\n'\n\
             echo 'SCRIPT ERROR: boom' >&2\n\
             echo '   at: _ready (res://main.gd:3)' >&2\n\
             exec sleep 30",
        );
        let project = fixture_project("clean");
        let path = project.path().to_str().unwrap().to_string();
        let host = TestHost::new(Ok(godot), &[]);
        let manager = Arc::new(GameManager::default());

        manager.run(&host_of(&host), &path).unwrap();
        assert!(project.path().join("addons/infinabox").is_dir());
        assert_eq!(count(&imports), 1);
        // The error arrives without any further stderr (quiet flush).
        host.wait_for(WAIT, |events| {
            events.iter().any(|e| matches!(e, Event::Error(_)))
                && events.iter().any(|e| matches!(e, Event::Output(_)))
        });
        assert_eq!(manager.state(), GameState::Running);
        assert_eq!(manager.status().1.as_deref(), Some(path.as_str()));
        assert_eq!(host.states(), vec![GameState::Starting, GameState::Running]);
        let expected = GameError {
            message: "boom".into(),
            file: Some("res://main.gd".into()),
            line: Some(3),
            raw: "SCRIPT ERROR: boom\n   at: _ready (res://main.gd:3)".into(),
        };
        assert_eq!(host.errors(), vec![expected.clone()]);
        assert_eq!(manager.recent_errors(10), vec![expected]);
        assert!(manager.recent_output(10).contains(&GameOutputLine {
            stream: OutputStream::Stdout,
            text: "hello".into()
        }));

        manager.stop(&host_of(&host)).unwrap();
        assert_eq!(manager.state(), GameState::Stopped);
        // The killed process's own exit report is ignored: no Crashed later.
        thread::sleep(Duration::from_millis(500));
        assert_eq!(
            host.states(),
            vec![GameState::Starting, GameState::Running, GameState::Stopped]
        );
        // Stopping again is a harmless no-op.
        manager.stop(&host_of(&host)).unwrap();
        assert_eq!(host.states().len(), 3);
    }

    #[cfg(unix)]
    #[test]
    fn a_non_zero_exit_is_crashed_and_its_last_error_is_flushed() {
        let tools = TempDir::new("tools");
        let godot = fake_godot(
            tools.path(),
            "exit 0",
            "echo 'ERROR: fatal thing' >&2\necho '   at: f (core/x.cpp:1)' >&2\nexit 3",
        );
        let project = fixture_project("clean");
        let host = TestHost::new(Ok(godot), &[]);
        let manager = Arc::new(GameManager::default());
        manager
            .run(&host_of(&host), project.path().to_str().unwrap())
            .unwrap();
        host.wait_for(WAIT, has_state(GameState::Crashed));
        assert_eq!(
            host.states(),
            vec![GameState::Starting, GameState::Running, GameState::Crashed]
        );
        assert_eq!(host.errors().len(), 1);
        assert_eq!(host.errors()[0].message, "fatal thing");
        assert_eq!(manager.state(), GameState::Crashed);
        // Nothing live, so nothing to restart.
        assert!(!manager.restart_if_running(&host_of(&host)).unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn restarts_only_when_running_and_imports_only_after_changes() {
        let tools = TempDir::new("tools");
        let imports = tools.path().join("imports");
        let runs = tools.path().join("runs");
        let godot = fake_godot(
            tools.path(),
            &format!("echo import >> '{}'", imports.display()),
            &format!("echo run >> '{}'\necho up\nexec sleep 30", runs.display()),
        );
        let project = fixture_project("clean");
        let host = TestHost::new(Ok(godot), &[]);
        let manager = Arc::new(GameManager::default());

        // Nothing running yet: restart is a no-op.
        assert!(!manager.restart_if_running(&host_of(&host)).unwrap());
        assert_eq!(host.events(), vec![]);

        manager
            .run(&host_of(&host), project.path().to_str().unwrap())
            .unwrap();
        host.wait_for(WAIT, |e| e.iter().any(|e| matches!(e, Event::Output(_))));
        assert_eq!((count(&imports), count(&runs)), (1, 1));

        // Unchanged files: restarted without importing again.
        assert!(manager.restart_if_running(&host_of(&host)).unwrap());
        assert_eq!(manager.state(), GameState::Running);
        assert_eq!(count(&imports), 1);
        wait_for_count(&runs, 2);

        // A new asset: imported before the restart.
        std::fs::write(project.path().join("icon.png"), "png").unwrap();
        assert!(manager.restart_if_running(&host_of(&host)).unwrap());
        assert_eq!(count(&imports), 2);
        wait_for_count(&runs, 3);

        // Each restart went straight from Running to Starting; the replaced
        // processes' exits were never reported.
        use GameState::*;
        assert_eq!(
            host.states(),
            vec![Starting, Running, Starting, Running, Starting, Running]
        );
        manager.stop(&host_of(&host)).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn stopping_during_the_import_cancels_the_start() {
        let tools = TempDir::new("tools");
        let runs = tools.path().join("runs");
        let godot = fake_godot(
            tools.path(),
            "sleep 1",
            &format!("echo run >> '{}'\nexec sleep 30", runs.display()),
        );
        let project = fixture_project("clean");
        let host = TestHost::new(Ok(godot), &[]);
        let manager = Arc::new(GameManager::default());

        let runner = {
            let (manager, host, path) = (
                manager.clone(),
                host_of(&host),
                project.path().to_str().unwrap().to_string(),
            );
            thread::spawn(move || manager.run(&host, &path))
        };
        host.wait_for(WAIT, has_state(GameState::Starting));
        // Doesn't wait for the import to finish.
        let started = Instant::now();
        manager.stop(&host_of(&host)).unwrap();
        assert!(started.elapsed() < Duration::from_millis(500));
        assert_eq!(manager.state(), GameState::Stopped);

        assert_eq!(runner.join().unwrap().unwrap_err(), STOPPED_WHILE_STARTING);
        assert_eq!(host.states(), vec![GameState::Starting, GameState::Stopped]);
        assert_eq!(count(&runs), 0, "the game must never have started");
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_import_is_an_error_with_godots_output() {
        let tools = TempDir::new("tools");
        let godot = fake_godot(
            tools.path(),
            "echo 'import went wrong' >&2\nexit 1",
            "exit 0",
        );
        let project = fixture_project("clean");
        let host = TestHost::new(Ok(godot), &[]);
        let manager = Arc::new(GameManager::default());
        let err = manager
            .run(&host_of(&host), project.path().to_str().unwrap())
            .unwrap_err();
        assert!(err.starts_with("Godot couldn't import"), "{err}");
        assert!(err.contains("import went wrong"), "{err}");
        assert_eq!(host.states(), vec![GameState::Starting, GameState::Stopped]);
    }

    fn real_godot() -> PathBuf {
        let path = std::env::var_os(locate::GODOT_PATH_ENV)
            .map(PathBuf::from)
            .expect("set INFINABOX_GODOT to a real Godot binary");
        assert!(path.is_file(), "{} is not a file", path.display());
        path
    }

    #[test]
    #[ignore = "needs a real Godot (INFINABOX_GODOT); run with --ignored"]
    fn runs_a_real_project_and_reports_its_runtime_error() {
        let project = fixture_project("runtime_error");
        let host = TestHost::new(Ok(real_godot()), &["--headless", "--quit-after", "30"]);
        let manager = Arc::new(GameManager::default());
        manager
            .run(&host_of(&host), project.path().to_str().unwrap())
            .unwrap();
        host.wait_for(Duration::from_secs(90), |e| {
            e.contains(&Event::State(GameState::Stopped))
        });
        assert_eq!(
            host.states(),
            vec![GameState::Starting, GameState::Running, GameState::Stopped]
        );
        let errors = host.errors();
        let jump = errors
            .iter()
            .find(|e| e.message == "Invalid call. Nonexistent function 'jump' in base 'Nil'.")
            .unwrap_or_else(|| panic!("{errors:#?}"));
        assert_eq!(jump.file.as_deref(), Some("res://main.gd"));
        assert!(host.events().contains(&Event::Output(GameOutputLine {
            stream: OutputStream::Stdout,
            text: "starting".into()
        })));
        // No output line kept any colour codes.
        assert!(manager
            .recent_output(MAX_RECENT_LINES)
            .iter()
            .all(|l| !l.text.contains('\u{1b}')));
    }

    /// Task C's finding: `godot --path` alone doesn't import a newly added
    /// asset. Adding one between two runs must still load on the second.
    #[test]
    #[ignore = "needs a real Godot (INFINABOX_GODOT); run with --ignored"]
    fn a_real_asset_added_between_runs_is_imported_before_the_next_run() {
        const PNG_1X1: &[u8] =
            b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR\x00\x00\x00\x01\x00\x00\x00\x01\
            \x08\x02\x00\x00\x00\x90\x77\x53\xde\x00\x00\x00\x0cIDAT\x78\x9c\x63\xf8\xcf\xc0\x00\
            \x00\x03\x01\x01\x00\xc9\xfe\x92\xef\x00\x00\x00\x00IEND\xae\x42\x60\x82";
        let project = fixture_project("clean");
        let host = TestHost::new(Ok(real_godot()), &["--headless", "--quit-after", "30"]);
        let manager = Arc::new(GameManager::default());
        let path = project.path().to_str().unwrap().to_string();
        let stopped = |n: usize| {
            move |e: &[Event]| {
                e.iter()
                    .filter(|e| **e == Event::State(GameState::Stopped))
                    .count()
                    >= n
            }
        };
        manager.run(&host_of(&host), &path).unwrap();
        host.wait_for(Duration::from_secs(90), stopped(1));

        std::fs::write(project.path().join("icon.png"), PNG_1X1).unwrap();
        std::fs::write(
            project.path().join("main.gd"),
            "extends Node2D\n\nfunc _ready() -> void:\n\
             \tprint(\"[infinabox] icon loaded: \", load(\"res://icon.png\") != null)\n",
        )
        .unwrap();
        manager.run(&host_of(&host), &path).unwrap();
        host.wait_for(Duration::from_secs(90), stopped(2));

        let output = manager.recent_output(MAX_RECENT_LINES);
        assert!(
            output
                .iter()
                .any(|l| l.text == "[infinabox] icon loaded: true"),
            "{output:#?}"
        );
        assert_eq!(manager.recent_errors(100), vec![]);
    }

    /// A script parse error is reported by both `--import` and the run;
    /// the user sees it once.
    #[test]
    #[ignore = "needs a real Godot (INFINABOX_GODOT); run with --ignored"]
    fn a_real_parse_error_is_reported_once() {
        let project = fixture_project("parse_error");
        let host = TestHost::new(Ok(real_godot()), &["--headless", "--quit-after", "30"]);
        let manager = Arc::new(GameManager::default());
        manager
            .run(&host_of(&host), project.path().to_str().unwrap())
            .unwrap();
        host.wait_for(Duration::from_secs(90), |e| {
            e.contains(&Event::State(GameState::Stopped))
        });
        let errors = host.errors();
        let parse: Vec<_> = errors
            .iter()
            .filter(|e| e.message.starts_with("Parse Error: Expected expression"))
            .collect();
        assert_eq!(parse.len(), 1, "{errors:#?}");
        assert_eq!(parse[0].file.as_deref(), Some("res://main.gd"));
        assert_eq!(parse[0].line, Some(4));
    }
}
