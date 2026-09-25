//! Headless boot check: runs the project for a few frames without a window
//! and returns the errors it printed. Phase A Task C fills this in.

use std::path::Path;

use anyhow::Result;

use super::types::GameError;

pub fn boot_check(_godot: &Path, _project: &Path) -> Result<Vec<GameError>> {
    anyhow::bail!("not implemented yet: godot::validate::boot_check")
}
