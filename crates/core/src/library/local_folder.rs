//! "My own files": a folder the person downloaded (a Kenney, Quaternius or
//! OpenGameArt pack, say). Items are the asset files inside; the license
//! comes from the pack's own license file when there is one, otherwise it is
//! unknown and the import asks. Wave 0 stub — task AL.

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use super::{LibraryItem, LibraryProvider, LibraryProviderInfo, LibraryQuery};

pub struct LocalFolder;

impl LibraryProvider for LocalFolder {
    fn info(&self) -> LibraryProviderInfo {
        LibraryProviderInfo {
            id: "local_folder".into(),
            name: "My own files".into(),
            blurb: "A folder you downloaded, such as a pack from Kenney, Quaternius or OpenGameArt. InfinaBox reads the pack's license file if it has one.".into(),
            needs_folder: true,
            site_url: None,
        }
    }

    fn search(&self, query: &LibraryQuery) -> Result<Vec<LibraryItem>> {
        let _ = query;
        bail!("not implemented yet (Phase C, task AL)")
    }

    fn fetch(&self, item: &LibraryItem, dest_dir: &Path) -> Result<Vec<PathBuf>> {
        let _ = (item, dest_dir);
        bail!("not implemented yet (Phase C, task AL)")
    }
}
