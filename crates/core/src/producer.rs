//! The Producer's journey (spec §6.3): Idea → Prototype → Vertical Slice →
//! Alpha → Beta → Launch, each with a checklist. An item is checked only by
//! a real signal in the project (cards, files, snapshots, license records) or
//! by the person; `evidence` says what was actually found.
//!
//! Manual ticks and the last successful game run live in
//! `.ibproject/journey.json`.
//!
//! Phase C contract (frozen). Wave 0 stub — task PJ fills in the bodies.

use std::path::Path;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Idea,
    Prototype,
    VerticalSlice,
    Alpha,
    Beta,
    Launch,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Signal {
    /// Checked by the app from something real in the project.
    Auto,
    /// Only the person can say.
    Manual,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Criterion {
    /// Stable id (`prototype.core_loop_cards`).
    pub id: String,
    pub title: String,
    pub signal: Signal,
    pub done: bool,
    /// What was found, in plain words ("3 mechanic cards"), or why it isn't done.
    pub evidence: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct StageStatus {
    pub stage: Stage,
    pub title: String,
    pub criteria: Vec<Criterion>,
    pub complete: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct NextStep {
    pub criterion_id: String,
    pub title: String,
    /// A ready-to-send message for the Producer role ("Help me finish: …").
    pub ask: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Journey {
    /// The first stage that isn't complete (Launch when everything is).
    pub current: Stage,
    pub stages: Vec<StageStatus>,
    pub next_step: Option<NextStep>,
}

pub fn compute(project: &Path) -> Result<Journey> {
    let _ = project;
    bail!("not implemented yet (Phase C, task PJ)")
}

/// Ticks or unticks a manual criterion. Refuses an unknown id or an
/// automatic criterion.
pub fn set_manual(project: &Path, id: &str, done: bool) -> Result<()> {
    let _ = (project, id, done);
    bail!("not implemented yet (Phase C, task PJ)")
}

/// Records whether the game last ran without errors (called by the app
/// after a run); feeds the "runs without errors" criterion.
pub fn record_boot(project: &Path, ok: bool) -> Result<()> {
    let _ = (project, ok);
    bail!("not implemented yet (Phase C, task PJ)")
}
