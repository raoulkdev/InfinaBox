//! Producer journey commands (`infinabox_core::producer`).
//!

use std::path::Path;

use infinabox_core::producer::{self, Journey};


#[tauri::command(async)]
pub fn journey_get(project_path: String) -> Result<Journey, String> {
    producer::compute(Path::new(&project_path)).map_err(|e| format!("{e:#}"))
}

#[tauri::command(async)]
pub fn journey_set_manual(project_path: String, id: String, done: bool) -> Result<Journey, String> {
    let project = Path::new(&project_path);
    producer::set_manual(project, &id, done).map_err(|e| format!("{e:#}"))?;
    producer::compute(project).map_err(|e| format!("{e:#}"))
}
