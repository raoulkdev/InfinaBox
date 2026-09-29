//! Producer journey commands (`infinabox_core::producer`).
//!
//! Wave 0 stubs — task X1 fills in the bodies.

use infinabox_core::producer::Journey;

const NOT_YET: &str = "Not implemented yet (Phase C, task X1)";

#[tauri::command(async)]
pub fn journey_get(project_path: String) -> Result<Journey, String> {
    let _ = project_path;
    Err(NOT_YET.into())
}

#[tauri::command(async)]
pub fn journey_set_manual(project_path: String, id: String, done: bool) -> Result<Journey, String> {
    let _ = (project_path, id, done);
    Err(NOT_YET.into())
}
