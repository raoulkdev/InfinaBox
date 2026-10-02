//! The automatic error-fix loop (spec §6.1 step 6): errors from the running
//! game are sent back to the agent without the person asking — at most
//! `MAX_ATTEMPTS` times in a row, and never while a turn is running.
//!
//! Plain state machine with no timers or threads of its own, so it's
//! tested directly; the app calls `poll` on a timer.
//!
//! The rules (Phase B plan, "Auto-fix scope"):
//! - Fixes go to the project's most recent thread (the last one a turn
//!   started on). With no thread known, nothing is ever started.
//! - Errors are collected only while no turn is running: during a turn the
//!   agent checks the game's errors itself. Starting a turn drops whatever
//!   was collected before it, and a fresh game run starts a fresh list.
//! - Errors are deduplicated (same message, file and line), and a fix waits
//!   until no new error has arrived for `debounce` — a game repeating the
//!   same error every frame still settles.
//! - At most `MAX_ATTEMPTS` fix turns in a row. Any turn the person started
//!   (a message, a plan approval, the first build) starts the count over.
//!   When the attempts are used up, or a fix brings back exactly the errors
//!   it was meant to fix, InfinaBox stops once with `GiveUp` and stays quiet
//!   until the count starts over.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use crate::agent::MessageOrigin;
use crate::godot::GameError;

/// Fix turns in a row before InfinaBox stops and tells the person.
pub const MAX_ATTEMPTS: u32 = 2;

/// Errors listed in a fix message; the rest are counted ("and 3 more").
const MAX_LISTED: usize = 5;

/// Distinct errors remembered between fixes. A game printing a new error
/// every frame (one with a changing number in it, say) can't grow this
/// without bound; past it the count in the message becomes "more than N".
const MAX_TRACKED: usize = 500;

/// However busy the error stream, a fix starts at most this many
/// `debounce` periods after the first error: a stream of ever-different
/// errors would otherwise never settle.
const MAX_WAIT_FACTOR: u32 = 4;

/// A single error message is cut to this many characters in the fix
/// message (a runaway one would otherwise swamp it).
const MAX_MESSAGE_CHARS: usize = 400;

#[derive(Debug, Clone, PartialEq)]
pub enum AutoFixDecision {
    Nothing,
    StartFix {
        thread_id: String,
        message: String,
        attempt: u32,
    },
    GiveUp {
        thread_id: String,
    },
}

/// What makes two errors "the same" for deduplication and for comparing the
/// errors a fix was for with the ones that came back.
type ErrorKey = (String, Option<String>, Option<u32>);

fn key_of(err: &GameError) -> ErrorKey {
    (err.message.clone(), err.file.clone(), err.line)
}

/// One project's auto-fix state.
pub struct AutoFix {
    debounce: Duration,
    /// The thread the latest turn started on: where fixes go.
    thread_id: Option<String>,
    /// Threads with a turn in progress (normally at most one).
    running: Vec<String>,
    /// Fix turns started since the count last started over.
    attempts: u32,
    /// The error set the latest fix turn was started for.
    last_fixed: Option<BTreeSet<ErrorKey>>,
    /// `GiveUp` was returned; quiet until the count starts over.
    gave_up: bool,
    /// Errors collected since the last fix, run or turn.
    pending: Pending,
}

#[derive(Default)]
struct Pending {
    /// Every distinct error seen (up to `MAX_TRACKED`).
    keys: BTreeSet<ErrorKey>,
    /// The first `MAX_LISTED` distinct errors, in the order they arrived.
    listed: Vec<GameError>,
    /// More distinct errors arrived than `MAX_TRACKED` could hold.
    overflowed: bool,
    first_at: Option<Instant>,
    /// When the latest *new* (not yet seen) error arrived.
    last_new_at: Option<Instant>,
}

impl Pending {
    fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    fn add(&mut self, err: &GameError, now: Instant) {
        let key = key_of(err);
        if self.keys.contains(&key) {
            return;
        }
        if self.keys.len() >= MAX_TRACKED {
            self.overflowed = true;
            return;
        }
        self.keys.insert(key);
        if self.listed.len() < MAX_LISTED {
            self.listed.push(err.clone());
        }
        self.first_at.get_or_insert(now);
        self.last_new_at = Some(now);
    }
}

impl AutoFix {
    pub fn new(debounce: Duration) -> Self {
        Self {
            debounce,
            thread_id: None,
            running: Vec::new(),
            attempts: 0,
            last_fixed: None,
            gave_up: false,
            pending: Pending::default(),
        }
    }

    /// A turn started on `thread_id`, which becomes the thread fixes go to.
    /// Anything collected before it is dropped (the agent sees the game's
    /// errors itself during the turn). A turn the person started begins the
    /// attempt count over; a fix turn doesn't.
    pub fn on_turn_started(&mut self, thread_id: &str, origin: MessageOrigin) {
        self.thread_id = Some(thread_id.to_string());
        if !self.running.iter().any(|t| t == thread_id) {
            self.running.push(thread_id.to_string());
        }
        self.pending = Pending::default();
        match origin {
            MessageOrigin::User | MessageOrigin::PlanApproval | MessageOrigin::FirstBuild => {
                self.attempts = 0;
                self.last_fixed = None;
                self.gave_up = false;
            }
            MessageOrigin::AutoFix => {}
        }
    }

    pub fn on_turn_finished(&mut self, thread_id: &str) {
        self.running.retain(|t| t != thread_id);
    }

    /// A fresh run of the game: errors from the previous run no longer apply.
    pub fn on_game_started(&mut self) {
        self.pending = Pending::default();
    }

    pub fn on_game_error(&mut self, err: &GameError, now: Instant) {
        if !self.running.is_empty() {
            return;
        }
        self.pending.add(err, now);
    }

    /// Called on a timer. `turn_running` is the app's own view (any turn in
    /// progress for this project); `enabled` is the project's "Fix errors
    /// automatically" setting.
    pub fn poll(&mut self, now: Instant, turn_running: bool, enabled: bool) -> AutoFixDecision {
        if self.pending.is_empty() {
            return AutoFixDecision::Nothing;
        }
        // Errors that can't lead to a fix are dropped rather than kept for
        // later: a fix must be about the game as it is when it starts.
        let (Some(thread_id), false, true, false) = (
            self.thread_id.clone(),
            turn_running || !self.running.is_empty(),
            enabled,
            self.gave_up,
        ) else {
            self.pending = Pending::default();
            return AutoFixDecision::Nothing;
        };

        let quiet = self
            .pending
            .last_new_at
            .is_some_and(|t| now.saturating_duration_since(t) >= self.debounce);
        let waited_long = self
            .pending
            .first_at
            .is_some_and(|t| now.saturating_duration_since(t) >= self.debounce * MAX_WAIT_FACTOR);
        if !quiet && !waited_long {
            return AutoFixDecision::Nothing;
        }

        let pending = std::mem::take(&mut self.pending);
        let same_as_last_fix = self.last_fixed.as_ref() == Some(&pending.keys);
        if self.attempts >= MAX_ATTEMPTS || same_as_last_fix {
            self.gave_up = true;
            return AutoFixDecision::GiveUp { thread_id };
        }

        self.attempts += 1;
        let message = fix_message(&pending);
        self.last_fixed = Some(pending.keys);
        AutoFixDecision::StartFix {
            thread_id,
            message,
            attempt: self.attempts,
        }
    }
}

/// The message a fix turn sends. The chat shows it too, so it reads as
/// plain, friendly instructions rather than a machine dump.
fn fix_message(pending: &Pending) -> String {
    let mut text = String::from("Something broke while the game was running. ");
    text.push_str(if pending.keys.len() == 1 {
        "This error showed up:\n\n"
    } else {
        "These errors showed up:\n\n"
    });
    for err in &pending.listed {
        text.push_str("- ");
        text.push_str(&one_line(&err.message));
        match (&err.file, err.line) {
            (Some(file), Some(line)) => text.push_str(&format!(" (at {file}:{line})")),
            (Some(file), None) => text.push_str(&format!(" (in {file})")),
            _ => {}
        }
        text.push('\n');
    }
    let unlisted = pending.keys.len() - pending.listed.len();
    if pending.overflowed {
        text.push_str(&format!("- …and more than {unlisted} others.\n"));
    } else if unlisted > 0 {
        text.push_str(&format!("- …and {unlisted} more.\n"));
    }
    text.push_str(
        "\nPlease fix this, then run the game again and check that the errors are gone. \
         When you're done, explain briefly what was wrong.",
    );
    text
}

/// Collapses whitespace (Godot's messages can span lines) and cuts a very
/// long message short.
fn one_line(s: &str) -> String {
    let flat = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= MAX_MESSAGE_CHARS {
        return flat;
    }
    let cut: String = flat.chars().take(MAX_MESSAGE_CHARS - 1).collect();
    format!("{}…", cut.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEBOUNCE: Duration = Duration::from_millis(1500);
    const THREAD: &str = "20260928T120000000-a1b2c3d4";

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn err(message: &str, file: Option<&str>, line: Option<u32>) -> GameError {
        GameError {
            message: message.into(),
            file: file.map(str::to_string),
            line,
            raw: format!("SCRIPT ERROR: {message}"),
        }
    }

    fn nil_call() -> GameError {
        err(
            "Invalid call. Nonexistent function 'jump' in base 'Nil'.",
            Some("res://player.gd"),
            Some(14),
        )
    }

    fn missing_node() -> GameError {
        err(
            "Node not found: \"Coin\" (relative to \"/root/Main\").",
            Some("res://main.gd"),
            Some(3),
        )
    }

    /// An `AutoFix` that has seen one finished turn on `THREAD`.
    fn ready() -> AutoFix {
        let mut fix = AutoFix::new(DEBOUNCE);
        fix.on_turn_started(THREAD, MessageOrigin::User);
        fix.on_turn_finished(THREAD);
        fix
    }

    fn started(decision: AutoFixDecision) -> (String, u32) {
        match decision {
            AutoFixDecision::StartFix {
                thread_id,
                message,
                attempt,
            } => {
                assert_eq!(thread_id, THREAD);
                (message, attempt)
            }
            other => panic!("expected StartFix, got {other:?}"),
        }
    }

    #[test]
    fn waits_for_errors_to_settle_then_starts_a_fix() {
        let mut fix = ready();
        let t0 = Instant::now();
        assert_eq!(fix.poll(t0, false, true), AutoFixDecision::Nothing, "no errors");

        fix.on_game_error(&nil_call(), t0);
        assert_eq!(fix.poll(t0 + ms(1000), false, true), AutoFixDecision::Nothing);
        // A new, different error restarts the wait.
        fix.on_game_error(&missing_node(), t0 + ms(1200));
        assert_eq!(fix.poll(t0 + ms(2000), false, true), AutoFixDecision::Nothing);
        assert_eq!(fix.poll(t0 + ms(2699), false, true), AutoFixDecision::Nothing);

        let (message, attempt) = started(fix.poll(t0 + ms(2700), false, true));
        assert_eq!(attempt, 1);
        assert_eq!(
            message,
            "Something broke while the game was running. These errors showed up:\n\n\
             - Invalid call. Nonexistent function 'jump' in base 'Nil'. (at res://player.gd:14)\n\
             - Node not found: \"Coin\" (relative to \"/root/Main\"). (at res://main.gd:3)\n\
             \nPlease fix this, then run the game again and check that the errors are gone. \
             When you're done, explain briefly what was wrong."
        );

        // The buffer was used up: nothing more until new errors arrive.
        assert_eq!(fix.poll(t0 + ms(9000), false, true), AutoFixDecision::Nothing);
    }

    #[test]
    fn repeats_of_the_same_error_are_deduplicated_and_do_not_delay_the_fix() {
        let mut fix = ready();
        let t0 = Instant::now();
        // The same error every frame, for longer than the debounce.
        for i in 0..200 {
            fix.on_game_error(&nil_call(), t0 + ms(i * 16));
        }
        let (message, _) = started(fix.poll(t0 + ms(3200), false, true));
        assert_eq!(message.matches("Nonexistent function").count(), 1);
        assert!(message.contains("This error showed up:"), "{message}");

        // Same message at a different line is a different error.
        let mut fix = ready();
        fix.on_game_error(&nil_call(), t0);
        fix.on_game_error(
            &err(&nil_call().message, Some("res://player.gd"), Some(20)),
            t0,
        );
        let (message, _) = started(fix.poll(t0 + DEBOUNCE, false, true));
        assert!(message.contains("player.gd:14") && message.contains("player.gd:20"));
    }

    #[test]
    fn lists_at_most_five_errors_and_counts_the_rest() {
        let mut fix = ready();
        let t0 = Instant::now();
        for i in 0..8 {
            fix.on_game_error(&err(&format!("Error number {i}"), None, None), t0);
        }
        let (message, _) = started(fix.poll(t0 + DEBOUNCE, false, true));
        for i in 0..5 {
            assert!(message.contains(&format!("- Error number {i}\n")), "{message}");
        }
        assert!(!message.contains("Error number 5"));
        assert!(message.contains("- …and 3 more.\n"), "{message}");

        // A file without a line, and a multi-line message.
        let mut fix = ready();
        fix.on_game_error(
            &err("Failed loading resource:\n   res://coin.png", Some("res://main.tscn"), None),
            t0,
        );
        let (message, _) = started(fix.poll(t0 + DEBOUNCE, false, true));
        assert!(
            message.contains("- Failed loading resource: res://coin.png (in res://main.tscn)\n"),
            "{message}"
        );
    }

    #[test]
    fn an_endless_stream_of_new_errors_still_gets_a_fix() {
        let mut fix = ready();
        let t0 = Instant::now();
        let mut t = t0;
        let mut decision = AutoFixDecision::Nothing;
        for i in 0..1000u64 {
            fix.on_game_error(&err(&format!("frame {i}"), None, None), t);
            decision = fix.poll(t, false, true);
            if decision != AutoFixDecision::Nothing {
                break;
            }
            t += ms(100);
        }
        let (message, _) = started(decision);
        assert!(t - t0 >= DEBOUNCE * MAX_WAIT_FACTOR && t - t0 < DEBOUNCE * (MAX_WAIT_FACTOR + 1));
        assert!(message.contains("…and "), "{message}");
    }

    #[test]
    fn the_tracked_set_is_bounded_and_says_so() {
        let mut fix = ready();
        let t0 = Instant::now();
        for i in 0..(MAX_TRACKED + 10) {
            fix.on_game_error(&err(&format!("e{i}"), None, None), t0);
        }
        let (message, _) = started(fix.poll(t0 + DEBOUNCE, false, true));
        assert!(
            message.contains(&format!("- …and more than {} others.", MAX_TRACKED - MAX_LISTED)),
            "{message}"
        );
    }

    #[test]
    fn nothing_starts_without_a_known_thread() {
        let mut fix = AutoFix::new(DEBOUNCE);
        let t0 = Instant::now();
        fix.on_game_error(&nil_call(), t0);
        assert_eq!(fix.poll(t0 + ms(5000), false, true), AutoFixDecision::Nothing);
        // Those errors were dropped, not kept for when a thread is known.
        fix.on_turn_started(THREAD, MessageOrigin::User);
        fix.on_turn_finished(THREAD);
        assert_eq!(fix.poll(t0 + ms(9000), false, true), AutoFixDecision::Nothing);
    }

    #[test]
    fn fixes_go_to_the_most_recent_thread() {
        let mut fix = ready();
        fix.on_turn_started("20260928T130000000-00000002", MessageOrigin::User);
        fix.on_turn_finished("20260928T130000000-00000002");
        let t0 = Instant::now();
        fix.on_game_error(&nil_call(), t0);
        match fix.poll(t0 + DEBOUNCE, false, true) {
            AutoFixDecision::StartFix { thread_id, .. } => {
                assert_eq!(thread_id, "20260928T130000000-00000002")
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn errors_during_a_turn_are_ignored_and_earlier_ones_dropped_when_it_starts() {
        let mut fix = ready();
        let t0 = Instant::now();
        // Collected, then a turn starts: dropped.
        fix.on_game_error(&nil_call(), t0);
        fix.on_turn_started(THREAD, MessageOrigin::User);
        // Errors while it runs are the agent's own business.
        fix.on_game_error(&missing_node(), t0 + ms(100));
        assert_eq!(fix.poll(t0 + ms(5000), false, true), AutoFixDecision::Nothing);
        fix.on_turn_finished(THREAD);
        assert_eq!(fix.poll(t0 + ms(9000), false, true), AutoFixDecision::Nothing);

        // Errors after it finished count.
        fix.on_game_error(&missing_node(), t0 + ms(10_000));
        started(fix.poll(t0 + ms(12_000), false, true));
    }

    #[test]
    fn the_apps_own_turn_running_flag_also_holds_fixes_back() {
        let mut fix = ready();
        let t0 = Instant::now();
        fix.on_game_error(&nil_call(), t0);
        assert_eq!(fix.poll(t0 + ms(5000), true, true), AutoFixDecision::Nothing);
        // Dropped, not postponed.
        assert_eq!(fix.poll(t0 + ms(6000), false, true), AutoFixDecision::Nothing);
    }

    #[test]
    fn a_new_game_run_clears_collected_errors() {
        let mut fix = ready();
        let t0 = Instant::now();
        fix.on_game_error(&nil_call(), t0);
        fix.on_game_started();
        assert_eq!(fix.poll(t0 + ms(5000), false, true), AutoFixDecision::Nothing);
        fix.on_game_error(&missing_node(), t0 + ms(5000));
        let (message, _) = started(fix.poll(t0 + ms(7000), false, true));
        assert!(!message.contains("Nonexistent function") && message.contains("Coin"));
    }

    #[test]
    fn disabled_drops_errors_and_does_nothing() {
        let mut fix = ready();
        let t0 = Instant::now();
        fix.on_game_error(&nil_call(), t0);
        assert_eq!(fix.poll(t0 + ms(5000), false, false), AutoFixDecision::Nothing);
        // Turning it back on doesn't resurrect errors from while it was off.
        assert_eq!(fix.poll(t0 + ms(6000), false, true), AutoFixDecision::Nothing);
        fix.on_game_error(&nil_call(), t0 + ms(7000));
        started(fix.poll(t0 + ms(9000), false, true));
    }

    /// One auto-fix round trip: the fix turn runs and finishes, then the
    /// game reports `errors` at `at`. Returns the next decision.
    fn after_fix_turn(fix: &mut AutoFix, errors: &[GameError], at: Instant) -> AutoFixDecision {
        fix.on_turn_started(THREAD, MessageOrigin::AutoFix);
        fix.on_turn_finished(THREAD);
        fix.on_game_started();
        for e in errors {
            fix.on_game_error(e, at);
        }
        fix.poll(at + DEBOUNCE, false, true)
    }

    #[test]
    fn at_most_two_attempts_then_gives_up_once() {
        let mut fix = ready();
        let t0 = Instant::now();
        fix.on_game_error(&nil_call(), t0);
        assert_eq!(started(fix.poll(t0 + DEBOUNCE, false, true)).1, 1);

        // The fix changed something: a different error now.
        let second = after_fix_turn(&mut fix, &[missing_node()], t0 + ms(10_000));
        assert_eq!(started(second).1, 2);

        // Still broken (differently again): attempts are used up.
        let third = after_fix_turn(
            &mut fix,
            &[err("Yet another error", None, None)],
            t0 + ms(20_000),
        );
        assert_eq!(
            third,
            AutoFixDecision::GiveUp {
                thread_id: THREAD.into()
            }
        );

        // Quiet from then on: once is enough.
        fix.on_game_started();
        fix.on_game_error(&nil_call(), t0 + ms(30_000));
        assert_eq!(fix.poll(t0 + ms(40_000), false, true), AutoFixDecision::Nothing);
        fix.on_game_error(&missing_node(), t0 + ms(41_000));
        assert_eq!(fix.poll(t0 + ms(50_000), false, true), AutoFixDecision::Nothing);
    }

    #[test]
    fn the_same_errors_coming_back_right_after_a_fix_gives_up_early() {
        let mut fix = ready();
        let t0 = Instant::now();
        fix.on_game_error(&nil_call(), t0);
        fix.on_game_error(&missing_node(), t0);
        assert_eq!(started(fix.poll(t0 + DEBOUNCE, false, true)).1, 1);

        // Exactly the same set (in a different order): no second attempt.
        let next = after_fix_turn(&mut fix, &[missing_node(), nil_call()], t0 + ms(10_000));
        assert_eq!(
            next,
            AutoFixDecision::GiveUp {
                thread_id: THREAD.into()
            }
        );

        // A subset is a different set, so it's worth another try.
        let mut fix = ready();
        fix.on_game_error(&nil_call(), t0);
        fix.on_game_error(&missing_node(), t0);
        started(fix.poll(t0 + DEBOUNCE, false, true));
        let next = after_fix_turn(&mut fix, &[nil_call()], t0 + ms(10_000));
        assert_eq!(started(next).1, 2);
    }

    #[test]
    fn a_turn_the_person_starts_resets_the_attempts() {
        for origin in [
            MessageOrigin::User,
            MessageOrigin::PlanApproval,
            MessageOrigin::FirstBuild,
        ] {
            let mut fix = ready();
            let t0 = Instant::now();
            fix.on_game_error(&nil_call(), t0);
            started(fix.poll(t0 + DEBOUNCE, false, true));
            let gave_up = after_fix_turn(&mut fix, &[nil_call()], t0 + ms(10_000));
            assert!(matches!(gave_up, AutoFixDecision::GiveUp { .. }), "{origin:?}");

            fix.on_turn_started(THREAD, origin);
            fix.on_turn_finished(THREAD);
            // The very same error is fair game again, from attempt 1.
            fix.on_game_error(&nil_call(), t0 + ms(20_000));
            assert_eq!(
                started(fix.poll(t0 + ms(22_000), false, true)).1,
                1,
                "{origin:?}"
            );
        }
    }

    #[test]
    fn a_fix_turn_does_not_reset_the_attempts() {
        let mut fix = ready();
        let t0 = Instant::now();
        fix.on_game_error(&nil_call(), t0);
        started(fix.poll(t0 + DEBOUNCE, false, true));
        let second = after_fix_turn(&mut fix, &[missing_node()], t0 + ms(10_000));
        assert_eq!(started(second).1, 2);
        let third = after_fix_turn(&mut fix, &[nil_call()], t0 + ms(20_000));
        assert!(matches!(third, AutoFixDecision::GiveUp { .. }));
    }

    #[test]
    fn long_messages_are_cut_short() {
        let long = "x".repeat(2000);
        let line = one_line(&long);
        assert_eq!(line.chars().count(), MAX_MESSAGE_CHARS);
        assert!(line.ends_with('…'));
        assert_eq!(one_line("a\n  b\tc"), "a b c");
    }
}
