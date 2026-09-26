//! Phase A contract types (frozen — see the Phase A plan's "Wave 0
//! contracts"). Mirrored in `src/lib/studio-types.ts`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct GodotStatus {
    pub installed: bool,
    pub version: Option<String>,
    pub path: Option<PathBuf>,
    /// True when this is InfinaBox's own managed install, false when it
    /// came from a user-configured path.
    pub managed: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GameState {
    Stopped,
    Starting,
    Running,
    Crashed,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GameOutputLine {
    pub stream: OutputStream,
    pub text: String,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutputStream {
    Stdout,
    Stderr,
}

/// One error parsed from real Godot output (script error, parse error,
/// engine error), with its location when Godot printed one.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GameError {
    pub message: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub raw: String,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct InstallProgress {
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub phase: String,
}
