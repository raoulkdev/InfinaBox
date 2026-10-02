//! "My own files": a folder the person downloaded (a Kenney, Quaternius or
//! OpenGameArt pack, say). Items are the asset files inside; the license
//! comes from the pack's own license file when there is one, otherwise it is
//! unknown and the import asks.
//!
//! `LibraryQuery.text` is the folder path. An optional name filter follows a
//! `|`: `/path/to/pack|tree` lists only files whose path inside the folder
//! contains "tree" (case-insensitive). `kind` filters by kind and `limit`
//! defaults to 200.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use anyhow::{Result, anyhow, bail};
use regex::Regex;

use super::{LibraryItem, LibraryProvider, LibraryProviderInfo, LibraryQuery};
use crate::assets::{AssetKind, LicenseInfo};

const DEFAULT_LIMIT: u32 = 200;
const MAX_DEPTH: usize = 6;
/// A license file bigger than this isn't read.
const LICENSE_MAX_BYTES: u64 = 1024 * 1024;

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
        let (folder_text, filter) = match query.text.rsplit_once('|') {
            Some((folder, filter)) => (folder, filter.trim().to_lowercase()),
            None => (query.text.as_str(), String::new()),
        };
        let folder = PathBuf::from(folder_text.trim());
        if folder_text.trim().is_empty() {
            bail!("Choose a folder to look in.");
        }
        let meta = fs::metadata(&folder)
            .map_err(|_| anyhow!("Couldn't find that folder. Check the path and try again."))?;
        if !meta.is_dir() {
            bail!("That's a file, not a folder. Choose the folder the pack is in.");
        }
        let root = folder.canonicalize().unwrap_or(folder);
        let source = format!(
            "My own files: {}",
            root.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| root.display().to_string())
        );
        let limit = query.limit.unwrap_or(DEFAULT_LIMIT).max(1) as usize;

        let mut walk = Walk {
            root: &root,
            source,
            filter,
            kind: query.kind,
            limit,
            items: Vec::new(),
            licenses: HashMap::new(),
        };
        walk.dir(&root, 0);
        Ok(walk.items)
    }

    fn fetch(&self, item: &LibraryItem, dest_dir: &Path) -> Result<Vec<PathBuf>> {
        let source = PathBuf::from(&item.id);
        let ok = source.is_absolute()
            && kind_of(&source).is_some()
            && !source.components().any(|c| matches!(c, Component::ParentDir))
            && !source
                .file_name()
                .is_some_and(|n| is_skipped(&n.to_string_lossy()))
            && is_regular_file(&source);
        if !ok {
            bail!("That file isn't in a folder InfinaBox listed, so it won't copy it.");
        }
        let name = source
            .file_name()
            .ok_or_else(|| anyhow!("That file has no name."))?;
        fs::create_dir_all(dest_dir).map_err(|e| anyhow!("Couldn't create the folder: {e}"))?;

        let is_gltf = source
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("gltf"));
        if !is_gltf {
            let target = copy_unique(&source, dest_dir, Path::new(name))?;
            return Ok(vec![target]);
        }

        // A .gltf points at a .bin and textures next to it: copy them with
        // the same layout, into one free folder so the links keep working.
        let base = source.parent().unwrap_or(Path::new("/"));
        let refs = gltf_references(&source, base);
        let mut group: Vec<(PathBuf, PathBuf)> = vec![(source.clone(), PathBuf::from(name))];
        group.extend(refs);
        let stem = source
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "model".into());
        let mut target_dir = dest_dir.to_path_buf();
        for n in 1..1000 {
            if group.iter().all(|(_, rel)| !target_dir.join(rel).exists()) {
                break;
            }
            target_dir = dest_dir.join(format!("{stem}-{}", n + 1));
        }
        let mut written = Vec::new();
        for (from, rel) in &group {
            match copy_exact(from, &target_dir.join(rel)) {
                Ok(p) => written.push(p),
                Err(e) => {
                    // Undo what this call copied; nothing pre-existing is touched.
                    for p in &written {
                        let _ = fs::remove_file(p);
                    }
                    return Err(e);
                }
            }
        }
        Ok(written)
    }
}

struct Walk<'a> {
    root: &'a Path,
    source: String,
    filter: String,
    kind: Option<AssetKind>,
    limit: usize,
    items: Vec<LibraryItem>,
    /// Directory -> the license found for files directly in it.
    licenses: HashMap<PathBuf, LicenseInfo>,
}

impl Walk<'_> {
    fn dir(&mut self, dir: &Path, depth: usize) {
        if self.items.len() >= self.limit {
            return;
        }
        let Ok(read) = fs::read_dir(dir) else { return };
        let mut entries: Vec<_> = read.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            if self.items.len() >= self.limit {
                return;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if is_skipped(&name) {
                continue;
            }
            let path = entry.path();
            // `symlink_metadata`, so links are never followed.
            let Ok(meta) = fs::symlink_metadata(&path) else { continue };
            if meta.is_dir() {
                if depth < MAX_DEPTH {
                    self.dir(&path, depth + 1);
                }
            } else if meta.is_file() {
                self.file(&path);
            }
        }
    }

    fn file(&mut self, path: &Path) {
        let Some(kind) = kind_of(path) else { return };
        if self.kind.is_some_and(|k| k != kind) {
            return;
        }
        if !self.filter.is_empty() {
            let rel = path.strip_prefix(self.root).unwrap_or(path);
            if !rel.to_string_lossy().to_lowercase().contains(&self.filter) {
                return;
            }
        }
        let dir = path.parent().unwrap_or(self.root).to_path_buf();
        let license = self.license_for(&dir);
        let title = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.items.push(LibraryItem {
            provider: "local_folder".into(),
            id: path.to_string_lossy().into_owned(),
            title,
            kind,
            license,
            thumbnail_url: None,
            page_url: None,
        });
    }

    fn license_for(&mut self, dir: &Path) -> LicenseInfo {
        if let Some(cached) = self.licenses.get(dir) {
            return cached.clone();
        }
        let mut found = LicenseInfo::default();
        // The file's folder and up to two parents, never above the chosen one.
        let mut current = Some(dir);
        for _ in 0..3 {
            let Some(d) = current else { break };
            if let Some(file) = find_license_file(d) {
                found = read_license(&file);
                break;
            }
            current = if d == self.root { None } else { d.parent() };
        }
        found.source = Some(self.source.clone());
        self.licenses.insert(dir.to_path_buf(), found.clone());
        found
    }
}

fn is_skipped(name: &str) -> bool {
    name.starts_with('.') || name == "__MACOSX"
}

fn is_regular_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.is_file())
}

/// The kind of asset a file is, by extension; `None` for anything else.
fn kind_of(path: &Path) -> Option<AssetKind> {
    let ext = path.extension()?.to_string_lossy().to_lowercase();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "svg" | "tga" | "exr" | "hdr" => {
            Some(AssetKind::Image)
        }
        "wav" | "ogg" | "mp3" | "flac" | "opus" => Some(AssetKind::Audio),
        "gltf" | "glb" | "obj" => Some(AssetKind::Model3d),
        "ttf" | "otf" | "woff" | "woff2" => Some(AssetKind::Font),
        _ => None,
    }
}

/// A license file directly in `dir` (LICENSE, License.txt, LICENCE.md,
/// COPYING, ... in any letter case), the first by name when several.
fn find_license_file(dir: &Path) -> Option<PathBuf> {
    let mut names: Vec<_> = fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let lower = name.to_lowercase();
            let (stem, ext) = match lower.rsplit_once('.') {
                Some((s, x)) => (s.to_string(), x.to_string()),
                None => (lower.clone(), String::new()),
            };
            let named = stem.starts_with("license")
                || stem.starts_with("licence")
                || stem.starts_with("copying");
            let texty = matches!(ext.as_str(), "" | "txt" | "md" | "rst");
            let small = e
                .metadata()
                .is_ok_and(|m| m.is_file() && m.len() <= LICENSE_MAX_BYTES);
            (named && texty && small).then_some(name)
        })
        .collect();
    names.sort();
    names.into_iter().next().map(|n| dir.join(n))
}

fn read_license(file: &Path) -> LicenseInfo {
    let bytes = fs::read(file).unwrap_or_default();
    let text = String::from_utf8_lossy(&bytes).into_owned();
    LicenseInfo {
        name: license_name(&text),
        source: None,
        author: license_author(&text),
        url: None,
        generated_by: None,
    }
}

/// A license's SPDX-style name from its text, or its first line when the
/// text isn't one InfinaBox knows.
fn license_name(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    let version = |re: &str| -> Option<String> {
        Regex::new(re)
            .ok()?
            .captures(&lower)
            .map(|c| c[1].to_string())
    };
    if Regex::new(r"\bcc[\s_-]*by[\s_-]*sa\b").unwrap().is_match(&lower)
        || lower.contains("attribution-sharealike")
        || lower.contains("attribution sharealike")
    {
        let v = version(r"(?:cc[\s_-]*by[\s_-]*sa|attribution[\s-]*sharealike)[\s_,:v-]*(?:version|license)?\s*(\d\.\d)")
            .unwrap_or_else(|| "4.0".into());
        return Some(format!("CC-BY-SA-{v}"));
    }
    if Regex::new(r"\bcc[\s_-]*by\b").unwrap().is_match(&lower)
        || lower.contains("creative commons attribution")
        || Regex::new(r"attribution\s*(?:license\s*)?\d\.\d").unwrap().is_match(&lower)
    {
        let v = version(r"(?:cc[\s_-]*by|creative commons attribution|attribution)[\s_,:v-]*(?:version|license)?\s*(\d\.\d)")
            .unwrap_or_else(|| "4.0".into());
        return Some(format!("CC-BY-{v}"));
    }
    if lower.contains("cc0") || lower.contains("creative commons zero") {
        return Some("CC0-1.0".into());
    }
    if lower.contains("mit license") {
        return Some("MIT".into());
    }
    if lower.contains("apache license") {
        return Some("Apache-2.0".into());
    }
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(|l| l.chars().take(80).collect::<String>().trim().to_string())
}

fn license_author(text: &str) -> Option<String> {
    let re = Regex::new(r"(?im)^\s*(?:created\s+by\s+|by\s+|author\s*:\s*)(.+?)\s*$").unwrap();
    let author = re.captures(text)?[1].trim().to_string();
    let author: String = author.chars().take(80).collect();
    (!author.is_empty()).then_some(author)
}

/// Files a .gltf refers to (buffers and images), as (source, path relative to
/// the model). Web links, data URIs, missing files and anything outside the
/// model's folder are left out.
fn gltf_references(gltf: &Path, base: &Path) -> Vec<(PathBuf, PathBuf)> {
    let Ok(text) = fs::read_to_string(gltf) else { return Vec::new() };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let mut out: Vec<(PathBuf, PathBuf)> = Vec::new();
    for key in ["buffers", "images"] {
        let Some(list) = json.get(key).and_then(|v| v.as_array()) else { continue };
        for entry in list {
            let Some(uri) = entry.get("uri").and_then(|v| v.as_str()) else { continue };
            if uri.contains("://") || uri.starts_with("data:") {
                continue;
            }
            let rel = PathBuf::from(percent_decode(uri));
            if rel.as_os_str().is_empty()
                || !rel.components().all(|c| matches!(c, Component::Normal(_)))
            {
                continue;
            }
            let from = base.join(&rel);
            if is_regular_file(&from) && !out.iter().any(|(_, r)| *r == rel) {
                out.push((from, rel));
            }
        }
    }
    out
}

/// Decodes `%20`-style escapes (glTF URIs are URI-encoded).
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(v) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Copies `from` to exactly `to` (creating parents), refusing to overwrite.
fn copy_exact(from: &Path, to: &Path) -> Result<PathBuf> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).map_err(|e| anyhow!("Couldn't create the folder: {e}"))?;
    }
    let mut src = fs::File::open(from).map_err(|e| anyhow!("Couldn't read that file: {e}"))?;
    let mut dst = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(to)
        .map_err(|e| anyhow!("Couldn't copy the file: {e}"))?;
    io::copy(&mut src, &mut dst).map_err(|e| anyhow!("Couldn't copy the file: {e}"))?;
    Ok(to.to_path_buf())
}

/// Copies `from` into `dir` under `name`, or `name-2.ext`, `name-3.ext`...
/// when that name is taken.
fn copy_unique(from: &Path, dir: &Path, name: &Path) -> Result<PathBuf> {
    let stem = name
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = name.extension().map(|e| e.to_string_lossy().into_owned());
    for n in 1..1000 {
        let file_name = match (n, &ext) {
            (1, _) => name.to_string_lossy().into_owned(),
            (n, Some(e)) => format!("{stem}-{n}.{e}"),
            (n, None) => format!("{stem}-{n}"),
        };
        let target = dir.join(file_name);
        if target.exists() {
            continue;
        }
        match copy_exact(from, &target) {
            Ok(p) => return Ok(p),
            // Lost a race for the name: try the next one.
            Err(_) if target.exists() => continue,
            Err(e) => return Err(e),
        }
    }
    bail!("Couldn't find a free file name for the copy.")
}
