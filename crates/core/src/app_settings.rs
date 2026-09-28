//! App-wide settings (not per project): which AI the user connected, a
//! custom Godot path, and whether the first-run setup is done. Stored as
//! `settings.json` in the app data folder. Nothing secret is ever stored
//! here — the AI CLIs keep their own credentials.
//!
//! Wave 0 stub (Phase B plan, Task CN fills it in).

use std::path::Path;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::connect::ProviderId;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct AppSettings {
    pub ai_provider: Option<ProviderId>,
    /// A Godot binary the user chose in Advanced settings, used instead of
    /// the managed install.
    pub godot_path: Option<String>,
    pub first_run_done: bool,
}

/// Reads `<dir>/settings.json`; a missing file is the defaults.
pub fn load(dir: &Path) -> Result<AppSettings> {
    let _ = dir;
    Ok(AppSettings::default())
}

/// Writes `<dir>/settings.json` atomically.
pub fn save(dir: &Path, settings: &AppSettings) -> Result<()> {
    let _ = (dir, settings);
    bail!("not implemented yet")
}
