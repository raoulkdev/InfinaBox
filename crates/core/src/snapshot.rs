//! Snapshots (spec §7.5): every AI change becomes a plain-titled git commit
//! carrying `InfinaBox-*` trailers; going back is always a new commit, never
//! a history rewrite. Phase A Task D fills this in.

use std::path::Path;

use anyhow::Result;
use serde::Serialize;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Snapshot {
    /// The commit sha.
    pub id: String,
    pub title: String,
    /// Unix seconds.
    pub timestamp: i64,
    pub thread_id: Option<String>,
    pub turn: Option<u32>,
    pub files_changed: usize,
}

/// Commits everything (respecting `.gitignore`). `None` when nothing
/// changed. `origin` is the (thread id, turn number) that produced it.
pub fn create_snapshot(
    _project: &Path,
    _title: &str,
    _origin: Option<(&str, u32)>,
) -> Result<Option<Snapshot>> {
    anyhow::bail!("not implemented yet: snapshot::create_snapshot")
}

/// Newest first.
pub fn list_snapshots(_project: &Path, _limit: usize) -> Result<Vec<Snapshot>> {
    anyhow::bail!("not implemented yet: snapshot::list_snapshots")
}

/// Makes the project look exactly like `snapshot_id` again, as a new
/// commit (after auto-saving any uncommitted work). Returns that commit.
pub fn restore_to(_project: &Path, _snapshot_id: &str) -> Result<Snapshot> {
    anyhow::bail!("not implemented yet: snapshot::restore_to")
}

/// Restores to the snapshot before the latest one. `None` when there is
/// nothing to undo.
pub fn undo_last(_project: &Path) -> Result<Option<Snapshot>> {
    anyhow::bail!("not implemented yet: snapshot::undo_last")
}
