//! Godot integration (spec §10): managed install, locating the binary,
//! running the game with output capture, and parsing real Godot errors.
//! Plain Rust with callbacks — no Tauri types — so it's unit-testable here.

pub mod errors;
pub mod install;
pub mod locate;
pub mod run;
pub mod types;
pub mod validate;

pub use types::*;

/// The one Godot version InfinaBox manages (spec §10.1). Upgrading means
/// changing this and every checksum in `install::PINNED_ASSETS` together.
pub const PINNED_VERSION: &str = "4.7.2-stable";
