//! Per-project settings, stored in `.ibproject/settings.json` and committed
//! with the game: how the agent plans, explains, and fixes errors.
//!
//! Wave 0 stub (Phase B plan, Task PL fills it in).

use std::path::Path;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::agent::PlanPolicy;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct ProjectSettings {
    pub plan_policy: PlanPolicy,
    /// "Teach me": explanations grow into short lessons.
    pub teach: bool,
    /// Send the running game's errors back to the agent automatically.
    pub auto_fix: bool,
}

impl Default for ProjectSettings {
    fn default() -> Self {
        Self {
            plan_policy: PlanPolicy::AlwaysPlan,
            teach: false,
            auto_fix: true,
        }
    }
}

/// Reads `.ibproject/settings.json`; a missing file is the defaults.
pub fn load(project: &Path) -> Result<ProjectSettings> {
    let _ = project;
    Ok(ProjectSettings::default())
}

pub fn save(project: &Path, settings: &ProjectSettings) -> Result<()> {
    let _ = (project, settings);
    bail!("not implemented yet")
}
