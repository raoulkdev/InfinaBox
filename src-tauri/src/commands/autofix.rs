//! The automatic error-fix loop in the app: feeds game errors and turn
//! starts/ends into `infinabox_core::autofix::AutoFix` (one per project),
//! polls it on a timer, starts fix turns through the same path as
//! `agent_send`, and emits `autofix-state`.
//!
//! Everything reaches the loop as a `Note` on one channel, handled by one
//! worker thread that owns all the state — no lock is shared with the
//! callers. That matters: `godot.rs` calls `note_game_state` and
//! `note_game_error` while holding its game manager's lock, and starting a
//! fix turn calls back into `note_turn_started`, so anything that locked
//! here could deadlock. Sending on the channel never blocks.
//!
//! The decisions themselves are in `FixLoop`, plain code over a `FixHost`
//! and explicit `Instant`s, tested below with a fake host and clock; the
//! worker only adds the timer and the app's host.
//!
//! Which project an error belongs to: the game the app last started. On
//! each `Starting` the worker reads the game manager's current project
//! (never from inside the hook, which runs under the manager's lock). A
//! later run's own `Starting` note always follows in the channel, and
//! clears the errors, so a note read "late" can't leave errors on the wrong
//! project. Only `Starting` counts as a fresh run: errors from importing
//! the project (a script that doesn't parse, say) arrive between `Starting`
//! and `Running`, and a fix should see them.

use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use infinabox_core::agent::MessageOrigin;
use infinabox_core::autofix::{AutoFix, AutoFixDecision, MAX_ATTEMPTS};
use infinabox_core::chat_store;
use infinabox_core::godot::{GameError, GameState};
use infinabox_core::project_settings;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::agent::{self, AgentState};

/// Event name (frontend: `src/lib/studio-api.ts`).
pub const EVENT_AUTOFIX_STATE: &str = "autofix-state";

/// How long a game's errors must settle before a fix starts.
const DEBOUNCE: Duration = Duration::from_millis(1500);

/// How often the worker polls every project's `AutoFix`.
const POLL_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutoFixPhase {
    Fixing,
    GaveUp,
    Idle,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AutoFixStatePayload {
    pub project_path: String,
    pub thread_id: String,
    pub state: AutoFixPhase,
    pub attempt: u32,
    pub max_attempts: u32,
}

// ---------------------------------------------------------------------------
// Hooks (called from `godot.rs` and `agent.rs`)
// ---------------------------------------------------------------------------

/// Called by `godot.rs` whenever the game's state changes.
pub fn note_game_state(app: &AppHandle, state: GameState) {
    send(app, Note::GameState(state));
}

/// Called by `godot.rs` for every error the running game reports.
pub fn note_game_error(app: &AppHandle, error: &GameError) {
    send(app, Note::GameError(error.clone(), Instant::now()));
}

/// Called by `agent.rs` when a turn has started (its message is saved).
/// `project` is exactly the path the frontend passed.
pub fn note_turn_started(app: &AppHandle, project: &str, thread_id: &str, origin: MessageOrigin) {
    send(
        app,
        Note::TurnStarted {
            project: project.to_string(),
            thread_id: thread_id.to_string(),
            origin,
        },
    );
}

/// Called by `agent.rs` when a turn is over (its thread is free again).
pub fn note_turn_finished(app: &AppHandle, project: &str, thread_id: &str) {
    send(
        app,
        Note::TurnFinished {
            project: project.to_string(),
            thread_id: thread_id.to_string(),
        },
    );
}

/// Tauri-managed state: the channel to the worker, which is started the
/// first time anything is noted.
#[derive(Default)]
pub struct AutoFixState {
    tx: OnceLock<Sender<Note>>,
}

fn send(app: &AppHandle, note: Note) {
    let Some(state) = app.try_state::<AutoFixState>() else {
        return;
    };
    let tx = state.tx.get_or_init(|| start_worker(app.clone()));
    // Fails only if the worker is gone, which it logged.
    let _ = tx.send(note);
}

fn start_worker(app: AppHandle) -> Sender<Note> {
    let (tx, rx) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("autofix".into())
        .spawn(move || run_worker(rx, &AppFixHost(app)));
    if let Err(e) = spawned {
        eprintln!("autofix: couldn't start; game errors won't be fixed automatically: {e}");
    }
    tx
}

/// Handles notes as they come and polls every `POLL_INTERVAL`, until the
/// app (and so the channel's sender) goes away. A panic in one step is
/// logged and the loop carries on, so one bad moment can't switch
/// auto-fix off for the rest of the session.
fn run_worker(rx: Receiver<Note>, host: &dyn FixHost) {
    let mut fixes = FixLoop::new(DEBOUNCE);
    let mut next_poll = Instant::now() + POLL_INTERVAL;
    loop {
        let now = Instant::now();
        let step = if now >= next_poll {
            next_poll = now + POLL_INTERVAL;
            catch_unwind(AssertUnwindSafe(|| fixes.tick(now, host)))
        } else {
            match rx.recv_timeout(next_poll - now) {
                Ok(note) => catch_unwind(AssertUnwindSafe(|| fixes.handle(note, host))),
                Err(RecvTimeoutError::Timeout) => Ok(()),
                Err(RecvTimeoutError::Disconnected) => return,
            }
        };
        if step.is_err() {
            eprintln!("autofix: a step panicked; carrying on");
        }
    }
}

// ---------------------------------------------------------------------------
// The testable loop
// ---------------------------------------------------------------------------

/// Something that happened, in the order it happened.
#[derive(Debug, Clone)]
pub(crate) enum Note {
    GameState(GameState),
    GameError(GameError, Instant),
    TurnStarted {
        project: String,
        thread_id: String,
        origin: MessageOrigin,
    },
    TurnFinished {
        project: String,
        thread_id: String,
    },
}

/// What the loop needs from the app.
pub(crate) trait FixHost {
    /// The project of the game the app last started (as the frontend
    /// passed it to `game_run`).
    fn game_project(&self) -> Option<String>;
    /// The project's newest chat thread: where fixes go until a turn
    /// starts somewhere (the thread the Studio chat opens on).
    fn latest_thread(&self, project: &str) -> Option<String>;
    /// Whether any turn is running in the project.
    fn turn_running(&self, project: &str) -> bool;
    /// The project's "Fix errors automatically" setting.
    fn enabled(&self, project: &str) -> bool;
    /// Starts a fix turn exactly like `agent_send` (origin `AutoFix`).
    fn start_fix(&self, project: &str, thread_id: &str, message: &str) -> Result<(), String>;
    fn emit(&self, payload: AutoFixStatePayload);
}

/// One project's auto-fix state, plus what's been told to the frontend.
struct ProjectFix {
    fix: AutoFix,
    /// `fix` knows a thread to send fixes to.
    has_thread: bool,
    /// Errors may be waiting in `fix`: set by every error, cleared whenever
    /// `fix` is known to have none (a new run, a turn, a decision, or a
    /// poll that dropped them). Only then is the settings file read — not
    /// four times a second for every project ever opened.
    maybe_pending: bool,
    /// The fix turn running now, if any.
    fixing: Option<String>,
    /// The latest fix attempt's number.
    attempt: u32,
    /// A fix turn on this thread has finished: `idle` is due, unless the
    /// next poll decides something else.
    idle_due: Option<String>,
}

pub(crate) struct FixLoop {
    debounce: Duration,
    /// By project path, exactly as the frontend passed it. Entries for
    /// closed projects are kept on purpose: they're tiny, and reopening the
    /// project picks its state back up.
    projects: HashMap<String, ProjectFix>,
    /// The project of the game started last.
    game_project: Option<String>,
}

impl FixLoop {
    pub(crate) fn new(debounce: Duration) -> Self {
        Self {
            debounce,
            projects: HashMap::new(),
            game_project: None,
        }
    }

    fn project(&mut self, project: &str, host: &dyn FixHost) -> &mut ProjectFix {
        let debounce = self.debounce;
        self.projects.entry(project.to_string()).or_insert_with(|| {
            let mut fix = AutoFix::new(debounce);
            // Until a turn starts, fixes go to the thread the chat opens
            // on. Told as a finished turn the person started, which is
            // also how a fresh `AutoFix` counts attempts: from zero.
            let latest = host.latest_thread(project);
            if let Some(thread) = &latest {
                fix.on_turn_started(thread, MessageOrigin::User);
                fix.on_turn_finished(thread);
            }
            ProjectFix {
                fix,
                has_thread: latest.is_some(),
                maybe_pending: false,
                fixing: None,
                attempt: 0,
                idle_due: None,
            }
        })
    }

    pub(crate) fn handle(&mut self, note: Note, host: &dyn FixHost) {
        match note {
            Note::GameState(GameState::Starting) => {
                self.game_project = host.game_project();
                if let Some(project) = self.game_project.clone() {
                    let entry = self.project(&project, host);
                    entry.fix.on_game_started();
                    entry.maybe_pending = false;
                }
            }
            Note::GameState(_) => {}
            Note::GameError(error, at) => {
                if self.game_project.is_none() {
                    self.game_project = host.game_project();
                }
                if let Some(project) = self.game_project.clone() {
                    let entry = self.project(&project, host);
                    entry.fix.on_game_error(&error, at);
                    entry.maybe_pending = true;
                }
            }
            Note::TurnStarted {
                project,
                thread_id,
                origin,
            } => {
                let entry = self.project(&project, host);
                entry.fix.on_turn_started(&thread_id, origin);
                entry.has_thread = true;
                entry.maybe_pending = false;
            }
            Note::TurnFinished { project, thread_id } => {
                let entry = self.project(&project, host);
                entry.fix.on_turn_finished(&thread_id);
                if entry.fixing.as_deref() == Some(thread_id.as_str()) {
                    entry.fixing = None;
                    entry.idle_due = Some(thread_id);
                }
            }
        }
    }

    pub(crate) fn tick(&mut self, now: Instant, host: &dyn FixHost) {
        for (project, entry) in self.projects.iter_mut() {
            let decision = if entry.maybe_pending {
                let running = host.turn_running(project);
                let enabled = !running && entry.has_thread && host.enabled(project);
                let decision = entry.fix.poll(now, running, enabled);
                if running || !enabled || decision != AutoFixDecision::Nothing {
                    entry.maybe_pending = false;
                }
                decision
            } else {
                AutoFixDecision::Nothing
            };
            let payload = |thread_id: &str, state, attempt| AutoFixStatePayload {
                project_path: project.clone(),
                thread_id: thread_id.to_string(),
                state,
                attempt,
                max_attempts: MAX_ATTEMPTS,
            };
            match decision {
                AutoFixDecision::StartFix {
                    thread_id,
                    message,
                    attempt,
                } => {
                    entry.idle_due = None;
                    entry.attempt = attempt;
                    match host.start_fix(project, &thread_id, &message) {
                        Ok(()) => {
                            entry.fixing = Some(thread_id.clone());
                            host.emit(payload(&thread_id, AutoFixPhase::Fixing, attempt));
                        }
                        // Another turn (usually the person's own message) took
                        // the thread first: that turn resets the attempts in
                        // core (`on_turn_started`), so this isn't a failed
                        // fix and nothing is shown.
                        Err(e) if host.turn_running(project) => {
                            eprintln!("autofix: a turn started first in {project}; no fix: {e}");
                        }
                        Err(e) => {
                            eprintln!("autofix: couldn't start a fix in {project}: {e}");
                            host.emit(payload(&thread_id, AutoFixPhase::GaveUp, attempt));
                        }
                    }
                }
                AutoFixDecision::GiveUp { thread_id } => {
                    entry.idle_due = None;
                    host.emit(payload(&thread_id, AutoFixPhase::GaveUp, entry.attempt));
                }
                AutoFixDecision::Nothing => {
                    if let Some(thread_id) = entry.idle_due.take() {
                        host.emit(payload(&thread_id, AutoFixPhase::Idle, entry.attempt));
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The app's host
// ---------------------------------------------------------------------------

struct AppFixHost(AppHandle);

impl FixHost for AppFixHost {
    fn game_project(&self) -> Option<String> {
        crate::commands::godot::manager(&self.0).status().1
    }

    fn latest_thread(&self, project: &str) -> Option<String> {
        // Newest first; a project without a chat yet has no thread.
        chat_store::list_threads(Path::new(project))
            .ok()?
            .into_iter()
            .next()
            .map(|thread| thread.id)
    }

    fn turn_running(&self, project: &str) -> bool {
        self.0.state::<AgentState>().turn_running_in(project)
    }

    /// A settings file that can't be read counts as "off": InfinaBox
    /// doesn't use the person's AI unasked when it can't tell whether they
    /// allowed it. (Their next message shows the settings error.)
    fn enabled(&self, project: &str) -> bool {
        match project_settings::load(Path::new(project)) {
            Ok(settings) => settings.auto_fix,
            Err(e) => {
                eprintln!("autofix: {project}: {e:#}; not fixing automatically");
                false
            }
        }
    }

    fn start_fix(&self, project: &str, thread_id: &str, message: &str) -> Result<(), String> {
        agent::send_turn(
            &self.0,
            project.to_string(),
            thread_id.to_string(),
            message.to_string(),
            MessageOrigin::AutoFix,
            Default::default(),
        )
    }

    fn emit(&self, payload: AutoFixStatePayload) {
        let _ = self.0.emit(EVENT_AUTOFIX_STATE, payload);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    const PROJECT: &str = "/games/space-dash";
    const OTHER: &str = "/games/other";
    const THREAD: &str = "20260928T120000000-a1b2c3d4";

    #[derive(Default)]
    struct FakeHost {
        game_project: RefCell<Option<String>>,
        latest: RefCell<HashMap<String, String>>,
        running: RefCell<Vec<String>>,
        disabled: RefCell<Vec<String>>,
        start_error: RefCell<Option<String>>,
        started: RefCell<Vec<(String, String, String)>>,
        emitted: RefCell<Vec<AutoFixStatePayload>>,
        settings_reads: RefCell<usize>,
        user_turn_wins: RefCell<bool>,
    }

    impl FixHost for FakeHost {
        fn game_project(&self) -> Option<String> {
            self.game_project.borrow().clone()
        }
        fn latest_thread(&self, project: &str) -> Option<String> {
            self.latest.borrow().get(project).cloned()
        }
        fn turn_running(&self, project: &str) -> bool {
            self.running.borrow().iter().any(|p| p == project)
        }
        fn enabled(&self, project: &str) -> bool {
            *self.settings_reads.borrow_mut() += 1;
            !self.disabled.borrow().iter().any(|p| p == project)
        }
        fn start_fix(&self, project: &str, thread_id: &str, message: &str) -> Result<(), String> {
            if *self.user_turn_wins.borrow() {
                // The person's message took the thread a moment earlier.
                self.running.borrow_mut().push(project.into());
                return Err("The AI is still working on this thread".into());
            }
            if let Some(e) = self.start_error.borrow().clone() {
                return Err(e);
            }
            self.started
                .borrow_mut()
                .push((project.into(), thread_id.into(), message.into()));
            Ok(())
        }
        fn emit(&self, payload: AutoFixStatePayload) {
            self.emitted.borrow_mut().push(payload);
        }
    }

    impl FakeHost {
        /// A game running in `PROJECT`, whose chat has `THREAD`.
        fn new() -> Self {
            let host = Self::default();
            *host.game_project.borrow_mut() = Some(PROJECT.into());
            host.latest
                .borrow_mut()
                .insert(PROJECT.into(), THREAD.into());
            host
        }
        fn take_emitted(&self) -> Vec<AutoFixStatePayload> {
            std::mem::take(&mut *self.emitted.borrow_mut())
        }
    }

    fn state(state: AutoFixPhase, attempt: u32) -> AutoFixStatePayload {
        AutoFixStatePayload {
            project_path: PROJECT.into(),
            thread_id: THREAD.into(),
            state,
            attempt,
            max_attempts: MAX_ATTEMPTS,
        }
    }

    fn nil_call() -> GameError {
        GameError {
            message: "Invalid call. Nonexistent function 'jump' in base 'Nil'.".into(),
            file: Some("res://player.gd".into()),
            line: Some(14),
            raw: "SCRIPT ERROR: Invalid call.".into(),
        }
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    const DEBOUNCE: Duration = Duration::from_millis(1500);

    /// The app runs a fix turn: it starts (through `start_fix`), the worker
    /// hears of it, and it ends.
    fn run_fix_turn(fixes: &mut FixLoop, host: &FakeHost) {
        fixes.handle(
            Note::TurnStarted {
                project: PROJECT.into(),
                thread_id: THREAD.into(),
                origin: MessageOrigin::AutoFix,
            },
            host,
        );
        fixes.handle(
            Note::TurnFinished {
                project: PROJECT.into(),
                thread_id: THREAD.into(),
            },
            host,
        );
    }

    #[test]
    fn errors_start_a_fix_turn_and_its_end_goes_back_to_idle() {
        let host = FakeHost::new();
        let mut fixes = FixLoop::new(DEBOUNCE);
        let t0 = Instant::now();
        fixes.handle(Note::GameState(GameState::Starting), &host);
        fixes.handle(Note::GameState(GameState::Running), &host);
        fixes.handle(Note::GameError(nil_call(), t0), &host);

        // Still settling.
        fixes.tick(t0 + ms(1000), &host);
        assert!(host.take_emitted().is_empty());
        assert!(host.started.borrow().is_empty());

        fixes.tick(t0 + DEBOUNCE, &host);
        assert_eq!(host.take_emitted(), vec![state(AutoFixPhase::Fixing, 1)]);
        let started = host.started.borrow().clone();
        assert_eq!(started.len(), 1);
        let (project, thread, message) = &started[0];
        assert_eq!((project.as_str(), thread.as_str()), (PROJECT, THREAD));
        assert!(
            message.contains("Nonexistent function 'jump'")
                && message.contains("res://player.gd:14"),
            "{message}"
        );

        // The fix turn runs: nothing new while it does.
        host.running.borrow_mut().push(PROJECT.into());
        fixes.handle(
            Note::TurnStarted {
                project: PROJECT.into(),
                thread_id: THREAD.into(),
                origin: MessageOrigin::AutoFix,
            },
            &host,
        );
        fixes.tick(t0 + ms(3000), &host);
        assert!(host.take_emitted().is_empty());

        // It ends, the game is fine: idle, once.
        host.running.borrow_mut().clear();
        fixes.handle(
            Note::TurnFinished {
                project: PROJECT.into(),
                thread_id: THREAD.into(),
            },
            &host,
        );
        fixes.tick(t0 + ms(4000), &host);
        assert_eq!(host.take_emitted(), vec![state(AutoFixPhase::Idle, 1)]);
        fixes.tick(t0 + ms(5000), &host);
        assert!(host.take_emitted().is_empty());
    }

    #[test]
    fn the_same_errors_after_a_fix_give_up_without_an_idle_first() {
        let host = FakeHost::new();
        let mut fixes = FixLoop::new(DEBOUNCE);
        let t0 = Instant::now();
        fixes.handle(Note::GameState(GameState::Starting), &host);
        fixes.handle(Note::GameError(nil_call(), t0), &host);
        fixes.tick(t0 + DEBOUNCE, &host);
        assert_eq!(host.take_emitted(), vec![state(AutoFixPhase::Fixing, 1)]);

        // The fix turn ends and the restarted game shows the same error
        // before the next poll: that decision replaces the idle.
        run_fix_turn(&mut fixes, &host);
        let t1 = t0 + ms(10_000);
        fixes.handle(Note::GameState(GameState::Starting), &host);
        fixes.handle(Note::GameError(nil_call(), t1), &host);
        fixes.tick(t1 + DEBOUNCE, &host);
        assert_eq!(host.take_emitted(), vec![state(AutoFixPhase::GaveUp, 1)]);
        assert_eq!(host.started.borrow().len(), 1);

        // Quiet from then on.
        fixes.handle(Note::GameState(GameState::Starting), &host);
        fixes.handle(Note::GameError(nil_call(), t1 + ms(5000)), &host);
        fixes.tick(t1 + ms(10_000), &host);
        assert!(host.take_emitted().is_empty());
    }

    #[test]
    fn two_attempts_then_give_up() {
        let host = FakeHost::new();
        let mut fixes = FixLoop::new(DEBOUNCE);
        let t0 = Instant::now();
        let other = |n: u32| GameError {
            message: format!("Error number {n}"),
            file: None,
            line: None,
            raw: format!("ERROR: Error number {n}"),
        };
        fixes.handle(Note::GameState(GameState::Starting), &host);
        fixes.handle(Note::GameError(other(1), t0), &host);
        fixes.tick(t0 + DEBOUNCE, &host);
        run_fix_turn(&mut fixes, &host);
        fixes.tick(t0 + ms(2000), &host);
        fixes.handle(Note::GameState(GameState::Starting), &host);
        fixes.handle(Note::GameError(other(2), t0 + ms(3000)), &host);
        fixes.tick(t0 + ms(4500), &host);
        run_fix_turn(&mut fixes, &host);
        fixes.handle(Note::GameState(GameState::Starting), &host);
        fixes.handle(Note::GameError(other(3), t0 + ms(6000)), &host);
        fixes.tick(t0 + ms(7500), &host);
        assert_eq!(
            host.take_emitted(),
            vec![
                state(AutoFixPhase::Fixing, 1),
                state(AutoFixPhase::Idle, 1),
                state(AutoFixPhase::Fixing, 2),
                state(AutoFixPhase::GaveUp, 2),
            ]
        );
    }

    #[test]
    fn a_fix_beaten_by_the_persons_own_turn_is_not_reported_as_given_up() {
        let host = FakeHost::new();
        *host.user_turn_wins.borrow_mut() = true;
        let mut fixes = FixLoop::new(DEBOUNCE);
        let t0 = Instant::now();
        fixes.handle(Note::GameState(GameState::Starting), &host);
        fixes.handle(Note::GameError(nil_call(), t0), &host);
        fixes.tick(t0 + DEBOUNCE, &host);
        assert_eq!(host.take_emitted(), vec![]);
    }

    #[test]
    fn a_fix_that_cannot_start_is_reported_as_given_up() {
        let host = FakeHost::new();
        *host.start_error.borrow_mut() = Some("The AI is still working".into());
        let mut fixes = FixLoop::new(DEBOUNCE);
        let t0 = Instant::now();
        fixes.handle(Note::GameState(GameState::Starting), &host);
        fixes.handle(Note::GameError(nil_call(), t0), &host);
        fixes.tick(t0 + DEBOUNCE, &host);
        assert_eq!(host.take_emitted(), vec![state(AutoFixPhase::GaveUp, 1)]);
    }

    #[test]
    fn nothing_happens_when_disabled_while_a_turn_runs_or_without_a_thread() {
        let t0 = Instant::now();

        // Switched off for this project.
        let host = FakeHost::new();
        host.disabled.borrow_mut().push(PROJECT.into());
        let mut fixes = FixLoop::new(DEBOUNCE);
        fixes.handle(Note::GameState(GameState::Starting), &host);
        fixes.handle(Note::GameError(nil_call(), t0), &host);
        fixes.tick(t0 + DEBOUNCE, &host);
        fixes.tick(t0 + ms(10_000), &host);
        assert!(host.take_emitted().is_empty());
        // The settings were read once, not on every poll after.
        assert_eq!(*host.settings_reads.borrow(), 1);

        // A turn is running in the project (the agent checks errors itself).
        let host = FakeHost::new();
        host.running.borrow_mut().push(PROJECT.into());
        let mut fixes = FixLoop::new(DEBOUNCE);
        fixes.handle(Note::GameState(GameState::Starting), &host);
        fixes.handle(Note::GameError(nil_call(), t0), &host);
        fixes.tick(t0 + DEBOUNCE, &host);
        host.running.borrow_mut().clear();
        fixes.tick(t0 + ms(10_000), &host);
        assert!(host.take_emitted().is_empty());

        // A project with no chat thread yet.
        let host = FakeHost::new();
        host.latest.borrow_mut().clear();
        let mut fixes = FixLoop::new(DEBOUNCE);
        fixes.handle(Note::GameState(GameState::Starting), &host);
        fixes.handle(Note::GameError(nil_call(), t0), &host);
        fixes.tick(t0 + ms(10_000), &host);
        assert!(host.take_emitted().is_empty());
        assert!(host.started.borrow().is_empty());
    }

    #[test]
    fn errors_belong_to_the_project_whose_game_started_last() {
        let host = FakeHost::new();
        host.latest
            .borrow_mut()
            .insert(OTHER.into(), "other-thread".into());
        let mut fixes = FixLoop::new(DEBOUNCE);
        let t0 = Instant::now();
        // A turn in `OTHER` (the person's last chat) doesn't claim errors
        // from `PROJECT`'s game, and doesn't block its fix either.
        fixes.handle(
            Note::TurnStarted {
                project: OTHER.into(),
                thread_id: "other-thread".into(),
                origin: MessageOrigin::User,
            },
            &host,
        );
        host.running.borrow_mut().push(OTHER.into());
        fixes.handle(Note::GameState(GameState::Starting), &host);
        fixes.handle(Note::GameError(nil_call(), t0), &host);
        fixes.tick(t0 + DEBOUNCE, &host);
        assert_eq!(host.take_emitted(), vec![state(AutoFixPhase::Fixing, 1)]);
        assert_eq!(host.started.borrow()[0].0, PROJECT);
    }

    #[test]
    fn the_payload_serializes_as_the_frontend_expects() {
        let json = serde_json::to_value(state(AutoFixPhase::GaveUp, 2)).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "projectPath": PROJECT,
                "threadId": THREAD,
                "state": "gave_up",
                "attempt": 2,
                "maxAttempts": 2,
            })
        );
    }
}
