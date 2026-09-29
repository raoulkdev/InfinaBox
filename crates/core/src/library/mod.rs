//! Free asset libraries (spec §7.3): search and download from places that
//! license their assets for reuse. Each provider implements
//! `LibraryProvider`; the app lists them and calls them by id.
//!
//! Phase C contract (frozen). Wave 0 stub — task AL fills in the providers.

pub mod local_folder;
pub mod polyhaven;

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::assets::{AssetKind, LicenseInfo};

/// What the Assets → Library tab shows for a provider.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LibraryProviderInfo {
    pub id: String,
    pub name: String,
    /// One plain sentence: what it has and under what license.
    pub blurb: String,
    /// True when search needs a folder path instead of a text query (My own files).
    pub needs_folder: bool,
    /// Where the provider's own site is, for "Browse the website".
    pub site_url: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct LibraryQuery {
    /// Search text (or the folder path for `local_folder`).
    pub text: String,
    /// Limit to one kind, when the provider distinguishes them.
    pub kind: Option<AssetKind>,
    pub limit: Option<u32>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LibraryItem {
    pub provider: String,
    /// Opaque to the app; the provider's own id (or a path for `local_folder`).
    pub id: String,
    pub title: String,
    pub kind: AssetKind,
    pub license: LicenseInfo,
    pub thumbnail_url: Option<String>,
    /// Human page for the item.
    pub page_url: Option<String>,
}

pub trait LibraryProvider: Send + Sync {
    fn info(&self) -> LibraryProviderInfo;
    fn search(&self, query: &LibraryQuery) -> Result<Vec<LibraryItem>>;
    /// Downloads (or copies) the item's files into `dest_dir` (created if
    /// missing) and returns their paths. Never overwrites.
    fn fetch(&self, item: &LibraryItem, dest_dir: &Path) -> Result<Vec<PathBuf>>;
}

/// All built-in providers.
pub fn providers() -> Vec<Box<dyn LibraryProvider>> {
    vec![
        Box::new(polyhaven::PolyHaven::from_env()),
        Box::new(local_folder::LocalFolder),
    ]
}

pub fn provider(id: &str) -> Option<Box<dyn LibraryProvider>> {
    providers().into_iter().find(|p| p.info().id == id)
}
