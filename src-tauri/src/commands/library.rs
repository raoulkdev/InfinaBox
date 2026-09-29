//! Asset library commands (`infinabox_core::library`).
//!
//! Wave 0 stubs — task X2 fills in the bodies.

use infinabox_core::assets::AssetInfo;
use infinabox_core::library::{LibraryItem, LibraryProviderInfo, LibraryQuery};

const NOT_YET: &str = "Not implemented yet (Phase C, task X2)";

#[tauri::command(async)]
pub fn library_providers() -> Result<Vec<LibraryProviderInfo>, String> {
    Err(NOT_YET.into())
}

#[tauri::command(async)]
pub fn library_search(provider: String, query: LibraryQuery) -> Result<Vec<LibraryItem>, String> {
    let _ = (provider, query);
    Err(NOT_YET.into())
}

#[tauri::command(async)]
pub fn library_import(
    project_path: String,
    provider: String,
    item: LibraryItem,
    dest_subdir: Option<String>,
) -> Result<Vec<AssetInfo>, String> {
    let _ = (project_path, provider, item, dest_subdir);
    Err(NOT_YET.into())
}
