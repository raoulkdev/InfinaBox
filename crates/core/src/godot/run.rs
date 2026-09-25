//! Runs the game as a child Godot process with stdout/stderr captured line
//! by line. Phase A Task C fills this in.

use std::path::Path;

use anyhow::Result;

use super::types::{GameOutputLine, GameState};

/// Where to put the game window: beside the InfinaBox window when known.
#[derive(Clone, Copy, Debug)]
pub struct WindowHint {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

pub struct GameProcess {}

impl GameProcess {
    /// `on_line` is called from reader threads for every output line;
    /// `on_exit` once, with `Stopped` or `Crashed`, when the process ends.
    pub fn start(
        _godot: &Path,
        _project: &Path,
        _window_hint: Option<WindowHint>,
        _on_line: impl FnMut(GameOutputLine) + Send + 'static,
        _on_exit: impl FnOnce(GameState) + Send + 'static,
    ) -> Result<Self> {
        anyhow::bail!("not implemented yet: GameProcess::start")
    }

    pub fn stop(&mut self) -> Result<()> {
        anyhow::bail!("not implemented yet: GameProcess::stop")
    }

    pub fn is_running(&mut self) -> bool {
        false
    }
}
