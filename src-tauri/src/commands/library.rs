//! Asset library commands (`infinabox_core::library`).
//!

use std::path::Path;

use infinabox_core::assets::{self, AssetInfo};
use super::generate::off_runtime;
use infinabox_core::library::{self, LibraryItem, LibraryProviderInfo, LibraryQuery};

#[tauri::command(async)]
pub fn library_providers() -> Result<Vec<LibraryProviderInfo>, String> {
    Ok(library::providers().iter().map(|p| p.info()).collect())
}

#[tauri::command(async)]
pub fn library_search(provider: String, query: LibraryQuery) -> Result<Vec<LibraryItem>, String> {
    let provider = library::provider(&provider)
        .ok_or_else(|| format!("There's no library called “{provider}”."))?;
    off_runtime(move || provider.search(&query).map_err(|e| format!("{e:#}")))
}

#[tauri::command(async)]
pub fn library_import(
    project_path: String,
    provider: String,
    item: LibraryItem,
    dest_subdir: Option<String>,
) -> Result<Vec<AssetInfo>, String> {
    let found = library::provider(&provider)
        .ok_or_else(|| format!("There's no library called “{provider}”."))?;
    let staging = std::env::temp_dir().join(format!("infinabox-library-{}", unique_id()));
    let cleanup = staging.clone();
    let result = off_runtime(move || {
        import_from(&*found, &item, &staging, Path::new(&project_path), dest_subdir.as_deref())
    });
    let _ = std::fs::remove_dir_all(&cleanup);
    result
}

/// A name that no other import in this process (or an earlier one) used.
fn unique_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!("{}-{nanos}", std::process::id())
}

fn import_from(
    found: &dyn library::LibraryProvider,
    item: &LibraryItem,
    staging: &Path,
    project: &Path,
    dest_subdir: Option<&str>,
) -> Result<Vec<AssetInfo>, String> {
    let files = found.fetch(item, staging).map_err(|e| format!("{e:#}"))?;
    if files.is_empty() {
        return Err("That item had no files to import.".into());
    }
    let mut imported = Vec::new();
    for file in &files {
        // A pack's extra files (textures for a model) get the item's title too.
        imported.push(
            assets::import_file(project, file, dest_subdir, &item.title, &item.license)
                .map_err(|e| format!("{e:#}"))?,
        );
    }
    Ok(imported)
}
