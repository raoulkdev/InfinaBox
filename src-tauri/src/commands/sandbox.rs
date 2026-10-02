//! Trial changes: the AI's work in a separate copy of the game, reviewed
//! before it reaches the real one (`infinabox_core::sandbox`). The turn that
//! makes the copy is started by `agent_send` in trial mode.

use std::path::{Path, PathBuf};

use infinabox_core::sandbox::{self, ApplyReport, Change, Sandbox};
use infinabox_core::snapshot::{self, Snapshot};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::godot;
use crate::commands::snapshot::EVENT_SNAPSHOTS_CHANGED;

/// The list of trials changed (one began, ended, was applied or thrown away).
pub const EVENT_SANDBOXES_CHANGED: &str = "sandboxes-changed";

pub fn note_changed(app: &AppHandle) {
    let _ = app.emit(EVENT_SANDBOXES_CHANGED, ());
}

/// Where trial copies live: the app's data folder, outside every project.
pub fn sandboxes_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Couldn't find the app data folder: {e}"))?;
    Ok(dir.join("sandboxes"))
}

fn err(e: anyhow::Error) -> String {
    format!("{e:#}")
}

#[derive(Serialize)]
pub struct Trial {
    #[serde(flatten)]
    pub sandbox: Sandbox,
    pub changes: Vec<Change>,
}

/// The project's trial copies that still exist, each with what it changed.
#[tauri::command(async)]
pub fn sandbox_list(app: AppHandle, project_path: String) -> Result<Vec<Trial>, String> {
    let base = sandboxes_dir(&app)?;
    sandbox::list(&base, Path::new(&project_path))
        .map_err(err)?
        .into_iter()
        .map(|sandbox| {
            let changes = sandbox::changes(&base, &sandbox.id).map_err(err)?;
            Ok(Trial { sandbox, changes })
        })
        .collect()
}

#[derive(Serialize)]
pub struct Applied {
    pub report: ApplyReport,
    pub snapshot: Option<Snapshot>,
}

/// Brings a trial's changes into the real game (all of them, or `only`
/// these paths), except files the game changed meanwhile, then saves a
/// snapshot so Undo works like for any other change. The trial stays until
/// it's thrown away.
#[tauri::command(async)]
pub fn sandbox_apply(
    app: AppHandle,
    project_path: String,
    id: String,
    only: Option<Vec<String>>,
) -> Result<Applied, String> {
    let base = sandboxes_dir(&app)?;
    let project = Path::new(&project_path);
    let trial = sandbox::get(&base, &id).map_err(err)?;
    if trial.path == project {
        return Err("That isn't a trial copy.".into());
    }
    let report = sandbox::apply(&base, &id, only.as_deref()).map_err(err)?;
    let snapshot = if report.applied.is_empty() {
        None
    } else {
        let title = format!("Trial: {}", trial.title);
        snapshot::create_snapshot(project, &title, None).map_err(err)?
    };
    if snapshot.is_some() {
        let _ = app.emit(EVENT_SNAPSHOTS_CHANGED, ());
        // A running game is on the old files.
        let _ = godot::restart_if_running(&app);
    }
    note_changed(&app);
    Ok(Applied { report, snapshot })
}

/// Throws a trial copy away.
#[tauri::command(async)]
pub fn sandbox_discard(app: AppHandle, id: String) -> Result<(), String> {
    let base = sandboxes_dir(&app)?;
    sandbox::discard(&base, &id).map_err(err)?;
    note_changed(&app);
    Ok(())
}

/// Runs the game from a trial copy, in the Play panel.
#[tauri::command(async)]
pub fn sandbox_play(app: AppHandle, id: String) -> Result<(), String> {
    let base = sandboxes_dir(&app)?;
    let trial = sandbox::get(&base, &id).map_err(err)?;
    godot::run_game(&app, &trial.path.to_string_lossy())
}
