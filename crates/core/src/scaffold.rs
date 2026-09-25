//! Creates new InfinaBox game projects from the bundled template
//! (`templates/blank-2d/`) and installs/updates the InfinaBox Godot addon
//! (`godot-addon/infinabox/`). Phase A Task E fills this in.

use std::path::{Path, PathBuf};

use anyhow::Result;

/// Creates `<parent_dir>/<name>` as a new project (Phase A: blank 2D only)
/// and returns its path. Refuses an existing non-empty directory.
pub fn create_project(_parent_dir: &Path, _name: &str) -> Result<PathBuf> {
    anyhow::bail!("not implemented yet: scaffold::create_project")
}

/// Installs or updates the addon files and its autoload entry. Returns
/// true if anything on disk changed.
pub fn ensure_addon(_project: &Path) -> Result<bool> {
    anyhow::bail!("not implemented yet: scaffold::ensure_addon")
}
