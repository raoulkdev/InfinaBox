//! The automatic error-fix loop (spec §6.1 step 6): errors from the running
//! game are sent back to the agent without the person asking — at most
//! `MAX_ATTEMPTS` times in a row, and never while a turn is running.
//!
//! Plain state machine with no timers or threads of its own, so it's
//! tested directly; the app calls `poll` on a timer.
//!
//! Wave 0 stub (Phase B plan, Task AF fills it in).

use std::time::{Duration, Instant};

use crate::agent::MessageOrigin;
use crate::godot::GameError;

/// Fix turns in a row before InfinaBox stops and tells the person.
pub const MAX_ATTEMPTS: u32 = 2;

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

/// One project's auto-fix state.
pub struct AutoFix {
    debounce: Duration,
}

impl AutoFix {
    pub fn new(debounce: Duration) -> Self {
        Self { debounce }
    }

    pub fn on_turn_started(&mut self, thread_id: &str, origin: MessageOrigin) {
        let _ = (thread_id, origin);
    }

    pub fn on_turn_finished(&mut self, thread_id: &str) {
        let _ = thread_id;
    }

    pub fn on_game_started(&mut self) {}

    pub fn on_game_error(&mut self, err: &GameError, now: Instant) {
        let _ = (err, now);
    }

    pub fn poll(&mut self, now: Instant, turn_running: bool, enabled: bool) -> AutoFixDecision {
        let _ = (now, turn_running, enabled, self.debounce);
        AutoFixDecision::Nothing
    }
}
