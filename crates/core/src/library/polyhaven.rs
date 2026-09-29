//! Poly Haven (polyhaven.com): CC0 HDRIs, textures and 3D models through its
//! public API (`INFINABOX_POLYHAVEN_BASE` overrides `https://api.polyhaven.com`
//! for tests). Wave 0 stub — task AL.

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use super::{LibraryItem, LibraryProvider, LibraryProviderInfo, LibraryQuery};

pub const DEFAULT_BASE: &str = "https://api.polyhaven.com";
pub const BASE_ENV: &str = "INFINABOX_POLYHAVEN_BASE";

pub struct PolyHaven {
    pub base: String,
}

impl PolyHaven {
    pub fn from_env() -> Self {
        Self {
            base: std::env::var(BASE_ENV).unwrap_or_else(|_| DEFAULT_BASE.to_string()),
        }
    }
}

impl LibraryProvider for PolyHaven {
    fn info(&self) -> LibraryProviderInfo {
        LibraryProviderInfo {
            id: "polyhaven".into(),
            name: "Poly Haven".into(),
            blurb: "Free 3D models, textures and HDRIs, all CC0 (no credit needed).".into(),
            needs_folder: false,
            site_url: Some("https://polyhaven.com".into()),
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
