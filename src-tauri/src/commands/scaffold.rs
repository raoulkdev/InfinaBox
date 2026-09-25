//! Thin Tauri wrapper over `infinabox_core::scaffold` for Home's "New
//! Project" flow. Phase A Task I fills this in.

#[tauri::command]
pub fn project_create(_parent_dir: String, _name: String) -> Result<String, String> {
    Err("not implemented yet: project_create".into())
}
