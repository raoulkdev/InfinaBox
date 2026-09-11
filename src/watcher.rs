//! Milestone 2 (Phase 0): a recursive file-system watcher that notices every
//! add/rename/move/delete in a real project directory in real time.
//! This is the "eyes" of InfinaBox's shared project memory — everything
//! downstream (the graph, the agents) trusts what this module reports.

use anyhow::Result;

pub fn run(path: &str) -> Result<()> {
    todo!("watch {path} recursively and print structured create/modify/rename/remove events")
}
