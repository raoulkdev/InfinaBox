//! A project's assets (spec §7.3): what's in the game's folders, where each
//! file came from and under what license, whether anything is unused or
//! missing, and the credits.
//!
//! A file's license lives in its **Asset card** (`.ibproject/context/assets/<slug>.md`,
//! front-matter `type: asset`, `file`, `license`, `source`, `author`, `url`,
//! `generated_by`), so it is part of the one model of the game.
//!
//! Phase C contract (frozen). Wave 0 stub — task AS fills in the bodies.

use std::path::Path;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    Image,
    Audio,
    /// glTF / GLB (and OBJ).
    Model3d,
    Font,
    Other,
}

/// Where an asset came from. All fields are what the person or the importer
/// actually recorded; unknown ones stay `None` and show as unknown.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct LicenseInfo {
    /// SPDX-style when known ("CC0-1.0", "CC-BY-4.0"), else what the pack said.
    pub name: Option<String>,
    /// Where it came from ("Poly Haven", "My own files: kenney_platformer", "Generated").
    pub source: Option<String>,
    pub author: Option<String>,
    pub url: Option<String>,
    /// Provider and model when the asset was generated ("cloudflare / @cf/…").
    pub generated_by: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AssetInfo {
    /// Project-relative, `/`-separated (`assets/images/hero.png`).
    pub path: String,
    pub kind: AssetKind,
    pub size_bytes: u64,
    /// Images only.
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// From the Asset card, when there is one.
    pub license: Option<LicenseInfo>,
    /// The Asset card's path relative to the context folder.
    pub card: Option<String>,
    /// Project files that reference this asset via `res://`.
    pub used_by: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MissingRef {
    pub from: String,
    pub to: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Oversized {
    pub path: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub size_bytes: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct HealthReport {
    /// Assets nothing references.
    pub unused: Vec<String>,
    /// `res://` paths in scenes/scripts/resources that don't exist.
    pub missing_refs: Vec<MissingRef>,
    /// Images over 4096 px on a side, or any asset over 20 MB.
    pub oversized: Vec<Oversized>,
    /// Assets without a license record.
    pub unlicensed: Vec<String>,
}

/// A file read for preview, size-capped.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FilePayload {
    pub mime: String,
    pub base64: String,
    /// True when the file was bigger than the cap and `base64` is empty.
    pub truncated: bool,
    pub size: u64,
}

pub const PREVIEW_CAP_BYTES: u64 = 25 * 1024 * 1024;

const NOT_YET: &str = "not implemented yet (Phase C, task AS)";

/// Every asset file under the project (skipping `.git`, `.godot`, `.ibproject`,
/// `addons/infinabox`), sorted by path.
pub fn scan(project: &Path) -> Result<Vec<AssetInfo>> {
    let _ = project;
    bail!(NOT_YET)
}

/// Copies `source` (a file) into the project under `assets/<folder for kind>/`
/// (or `dest_subdir` inside `assets/`), never overwriting, writes its Asset
/// card, and returns it. `title` is the card's title.
pub fn import_file(
    project: &Path,
    source: &Path,
    dest_subdir: Option<&str>,
    title: &str,
    license: &LicenseInfo,
) -> Result<AssetInfo> {
    let _ = (project, source, dest_subdir, title, license);
    bail!(NOT_YET)
}

pub fn health(project: &Path) -> Result<HealthReport> {
    let _ = project;
    bail!(NOT_YET)
}

/// Markdown credits grouped by license, from the Asset cards.
pub fn credits(project: &Path) -> Result<String> {
    let _ = project;
    bail!(NOT_YET)
}

/// Reads a project file for preview. `path` is project-relative and must stay
/// inside the project. `max_bytes` defaults to `PREVIEW_CAP_BYTES`.
pub fn read_base64(project: &Path, path: &str, max_bytes: Option<u64>) -> Result<FilePayload> {
    let _ = (project, path, max_bytes);
    bail!(NOT_YET)
}
