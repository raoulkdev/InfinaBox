//! Incremental parser turning real Godot output lines into `GameError`s,
//! plus bounded buffers of recent errors/output for the bridge. Phase A
//! Task C fills this in, matched against `tests/fixtures/godot/`.

use super::types::{GameError, GameOutputLine};

#[derive(Default)]
pub struct ErrorParser {}

impl ErrorParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds one output line; returns any errors completed by it.
    pub fn push(&mut self, _line: &GameOutputLine) -> Vec<GameError> {
        Vec::new()
    }

    /// Flushes an error still waiting for a location line (e.g. at exit).
    pub fn finish(&mut self) -> Vec<GameError> {
        Vec::new()
    }
}

/// Bounded history of recent output and errors, oldest dropped first.
pub struct RecentLog {
    pub max_lines: usize,
    pub max_errors: usize,
}

impl RecentLog {
    pub fn new(max_lines: usize, max_errors: usize) -> Self {
        Self { max_lines, max_errors }
    }

    pub fn push_line(&mut self, _line: GameOutputLine) {}

    pub fn push_error(&mut self, _error: GameError) {}

    pub fn recent_lines(&self, _n: usize) -> Vec<GameOutputLine> {
        Vec::new()
    }

    pub fn recent_errors(&self, _n: usize) -> Vec<GameError> {
        Vec::new()
    }
}
