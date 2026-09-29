//! A project's assets (spec §7.3): what's in the game's folders, where each
//! file came from and under what license, whether anything is unused or
//! missing, and the credits.
//!
//! A file's license lives in its **Asset card** (`.ibproject/context/assets/<slug>.md`,
//! front-matter `type: asset`, `file`, `license`, `source`, `author`, `url`,
//! `generated_by`), so it is part of the one model of the game.
//!
//! Phase C contract (frozen).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

use anyhow::{Context, Result, bail};
use base64::Engine;
use regex::Regex;
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

/// Images over this many pixels on a side are flagged by `health`.
const MAX_IMAGE_SIDE: u32 = 4096;
/// Any asset file over this many bytes is flagged by `health`.
const MAX_ASSET_BYTES: u64 = 20 * 1024 * 1024;
/// Project text files bigger than this are not searched for references.
const MAX_TEXT_BYTES: u64 = 2 * 1024 * 1024;
/// Project files whose text can mention `res://` paths.
const TEXT_EXTENSIONS: &[&str] = &["tscn", "tres", "gd", "godot", "cfg", "gdshader"];
/// Where Asset cards live, relative to the project.
const CARDS_DIR: &str = ".ibproject/context/assets";

static RES_REF: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"res://([^"'\r\n]+)"#).expect("valid regex"));

fn extension(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// The kind of asset a file name is, or `None` when it isn't an asset at all
/// (scenes, scripts, `.import` files and everything else Godot keeps).
fn kind_of(path: &str) -> Option<AssetKind> {
    Some(match extension(path).as_str() {
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "svg" | "bmp" => AssetKind::Image,
        "ogg" | "wav" | "mp3" | "flac" | "opus" => AssetKind::Audio,
        "glb" | "gltf" | "obj" => AssetKind::Model3d,
        "ttf" | "otf" | "woff" | "woff2" => AssetKind::Font,
        _ => return None,
    })
}

fn folder_for(kind: AssetKind) -> &'static str {
    match kind {
        AssetKind::Image => "images",
        AssetKind::Audio => "audio",
        AssetKind::Model3d => "models",
        AssetKind::Font => "fonts",
        AssetKind::Other => "misc",
    }
}

// ---------- Asset cards ----------

/// One parsed Asset card.
struct Card {
    /// Path relative to `.ibproject/context/`.
    card: String,
    title: Option<String>,
    file: String,
    license: LicenseInfo,
}

/// Splits a Markdown file's `---` front-matter into `key: value` pairs.
fn parse_front_matter(text: &str) -> Vec<(String, String)> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.lines();
    if lines.next().map(str::trim_end) != Some("---") {
        return Vec::new();
    }
    let mut out = Vec::new();
    for line in lines {
        if line.trim_end() == "---" {
            return out;
        }
        if let Some((k, v)) = line.split_once(':') {
            if k.is_empty() || k.starts_with(char::is_whitespace) {
                continue;
            }
            out.push((k.trim().to_string(), unquote(v.trim())));
        }
    }
    // No closing fence: not front-matter.
    Vec::new()
}

fn unquote(v: &str) -> String {
    if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
        let mut out = String::new();
        let mut chars = v[1..v.len() - 1].chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('n') => out.push('\n'),
                    Some(o) => out.push(o),
                    None => out.push('\\'),
                }
            } else {
                out.push(c);
            }
        }
        out
    } else if v.len() >= 2 && v.starts_with('\'') && v.ends_with('\'') {
        v[1..v.len() - 1].replace("''", "'")
    } else {
        v.to_string()
    }
}

/// A front-matter value, double-quoted when a plain one could be misread.
fn yaml_value(v: &str) -> String {
    let v = v.replace(['\r', '\n'], " ");
    let needs_quotes = v.is_empty()
        || v.contains([':', '#', '"', '\\'])
        || v.starts_with(|c: char| c.is_whitespace() || "[]{}&*!|>'%@`-?".contains(c))
        || v.ends_with(char::is_whitespace);
    if needs_quotes {
        format!("\"{}\"", v.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        v
    }
}

fn normalize_rel(p: &str) -> String {
    let p = p.trim().replace('\\', "/");
    p.strip_prefix("./").unwrap_or(&p).to_string()
}

fn non_empty(s: Option<&String>) -> Option<String> {
    s.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Every Asset card (`type: asset`) in the project, ordered by card path.
fn read_cards(project: &Path) -> Vec<Card> {
    let dir = project.join(CARDS_DIR);
    let Ok(entries) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.ends_with(".md") && !n.starts_with('.'))
        .collect();
    names.sort();
    let mut cards = Vec::new();
    for name in names {
        let Ok(text) = fs::read_to_string(dir.join(&name)) else {
            continue;
        };
        let fm: HashMap<String, String> = parse_front_matter(&text).into_iter().collect();
        if fm.get("type").map(|t| t.trim()) != Some("asset") {
            continue;
        }
        let Some(file) = non_empty(fm.get("file")) else {
            continue;
        };
        cards.push(Card {
            card: format!("assets/{name}"),
            title: non_empty(fm.get("title")),
            file: normalize_rel(&file),
            license: LicenseInfo {
                name: non_empty(fm.get("license")),
                source: non_empty(fm.get("source")),
                author: non_empty(fm.get("author")),
                url: non_empty(fm.get("url")),
                generated_by: non_empty(fm.get("generated_by")),
            },
        });
    }
    cards
}

// ---------- Scanning ----------

/// Everything one walk of the project finds.
struct Survey {
    assets: Vec<AssetInfo>,
    /// `(project file, res:// target)` for every reference in project text.
    refs: Vec<(String, String)>,
}

/// Lists files below `dir` (recursively), skipping hidden folders and files,
/// the top-level `addons/`, and every symlink. Paths are project-relative.
fn walk(dir: &Path, rel: &str, out: &mut Vec<(PathBuf, String)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_symlink() {
            continue;
        }
        let child_rel = if rel.is_empty() {
            name.clone()
        } else {
            format!("{rel}/{name}")
        };
        if ft.is_dir() {
            if rel.is_empty() && name == "addons" {
                continue;
            }
            walk(&entry.path(), &child_rel, out);
        } else if ft.is_file() {
            out.push((entry.path(), child_rel));
        }
    }
}

/// The `res://` targets mentioned in a project text file. Whole-line comments
/// (the trivially detectable ones) are ignored.
fn refs_in(rel: &str, text: &str) -> BTreeSet<String> {
    let comment = match extension(rel).as_str() {
        "gd" | "gdshader" => Some('#'),
        "cfg" | "godot" => Some(';'),
        _ => None,
    };
    let mut out = BTreeSet::new();
    for line in text.lines() {
        if comment.is_some_and(|c| line.trim_start().starts_with(c)) {
            continue;
        }
        for cap in RES_REF.captures_iter(line) {
            let target = cap[1].trim_end();
            if !target.is_empty() {
                out.insert(target.to_string());
            }
        }
    }
    out
}

fn survey(project: &Path) -> Result<Survey> {
    if !project.is_dir() {
        bail!("{} isn't a folder", project.display());
    }
    let mut files = Vec::new();
    walk(project, "", &mut files);
    files.sort_by(|a, b| a.1.cmp(&b.1));

    let cards = read_cards(project);
    let mut by_file: HashMap<&str, &Card> = HashMap::new();
    for c in &cards {
        by_file.entry(c.file.as_str()).or_insert(c);
    }

    let mut assets = Vec::new();
    let mut refs = Vec::new();
    let mut used: HashMap<String, Vec<String>> = HashMap::new();
    for (abs, rel) in &files {
        if let Some(kind) = kind_of(rel) {
            let size_bytes = fs::metadata(abs).map(|m| m.len()).unwrap_or(0);
            let (width, height) = if kind == AssetKind::Image && extension(rel) != "svg" {
                match image::image_dimensions(abs) {
                    Ok((w, h)) => (Some(w), Some(h)),
                    Err(_) => (None, None),
                }
            } else {
                (None, None)
            };
            let card = by_file.get(rel.as_str());
            assets.push(AssetInfo {
                path: rel.clone(),
                kind,
                size_bytes,
                width,
                height,
                license: card.map(|c| c.license.clone()),
                card: card.map(|c| c.card.clone()),
                used_by: Vec::new(),
            });
        } else if TEXT_EXTENSIONS.contains(&extension(rel).as_str()) {
            let small = fs::metadata(abs)
                .map(|m| m.len() <= MAX_TEXT_BYTES)
                .unwrap_or(false);
            if !small {
                continue;
            }
            let Ok(bytes) = fs::read(abs) else { continue };
            let Ok(text) = String::from_utf8(bytes) else {
                continue;
            };
            for target in refs_in(rel, &text) {
                used.entry(target.clone()).or_default().push(rel.clone());
                refs.push((rel.clone(), target));
            }
        }
    }
    for a in &mut assets {
        if let Some(by) = used.remove(&a.path) {
            a.used_by = by;
        }
    }
    Ok(Survey { assets, refs })
}

/// Every asset file under the project (skipping `.git`, `.godot`, `.ibproject`,
/// `addons/infinabox`), sorted by path.
pub fn scan(project: &Path) -> Result<Vec<AssetInfo>> {
    Ok(survey(project)?.assets)
}

// ---------- Import ----------

/// Files and folders one import call has written, so a failure can undo them.
#[derive(Default)]
struct Written {
    files: Vec<PathBuf>,
    dirs: Vec<PathBuf>,
}

impl Written {
    fn undo(&self) {
        for f in self.files.iter().rev() {
            let _ = fs::remove_file(f);
        }
        for d in self.dirs.iter().rev() {
            let _ = fs::remove_dir(d);
        }
    }

    /// Creates `dir` and any missing parents, remembering the new ones.
    fn ensure_dir(&mut self, dir: &Path) -> Result<()> {
        let mut missing = Vec::new();
        let mut cur = dir;
        while !cur.exists() {
            missing.push(cur.to_path_buf());
            match cur.parent() {
                Some(p) => cur = p,
                None => break,
            }
        }
        for d in missing.into_iter().rev() {
            fs::create_dir(&d).with_context(|| format!("couldn't create {}", d.display()))?;
            self.dirs.push(d);
        }
        Ok(())
    }
}

/// A file name with only letters, digits and `-_. ` left.
fn sanitize_name(name: &str) -> String {
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s, e),
        _ => (name, ""),
    };
    let keep = |s: &str, allow_dot_space: bool| -> String {
        s.chars()
            .filter(|c| {
                c.is_alphanumeric()
                    || matches!(c, '-' | '_')
                    || (allow_dot_space && matches!(c, '.' | ' '))
            })
            .collect()
    };
    let stem = keep(stem, true);
    let stem = stem.trim_matches(|c: char| c == '.' || c == ' ');
    let stem = if stem.is_empty() { "asset" } else { stem };
    let ext = keep(ext, false);
    if ext.is_empty() {
        stem.to_string()
    } else {
        format!("{stem}.{ext}")
    }
}

/// `name`, or `stem 2.ext`, `stem 3.ext`, … until nothing is in the way.
fn unique_name(dir: &Path, name: &str) -> String {
    let taken = |n: &str| dir.join(n).symlink_metadata().is_ok();
    if !taken(name) {
        return name.to_string();
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) => (s, format!(".{e}")),
        None => (name, String::new()),
    };
    let mut n = 2;
    loop {
        let candidate = format!("{stem} {n}{ext}");
        if !taken(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// Checks a caller-given folder inside `assets/` and returns its parts.
fn validate_subdir(sub: &str) -> Result<Vec<String>> {
    let bad = || anyhow::anyhow!("\"{sub}\" isn't a folder name I can put assets in");
    let normalized = sub.replace('\\', "/");
    if normalized.starts_with('/') {
        return Err(bad());
    }
    let mut parts = Vec::new();
    for part in normalized.split('/') {
        if part.is_empty() {
            continue;
        }
        let ok = !part.starts_with('.')
            && part
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ' '));
        if !ok {
            return Err(bad());
        }
        parts.push(part.to_string());
    }
    if parts.is_empty() {
        return Err(bad());
    }
    Ok(parts)
}

/// Copies a file next to its final name first, then renames it into place.
fn atomic_copy(src: &Path, dest: &Path, written: &mut Written) -> Result<()> {
    let dir = dest.parent().context("no destination folder")?;
    let tmp = dir.join(format!(".ibtmp-{}", uuid::Uuid::new_v4()));
    if let Err(e) = fs::copy(src, &tmp) {
        let _ = fs::remove_file(&tmp);
        return Err(e).with_context(|| format!("couldn't copy {}", src.display()));
    }
    if let Err(e) = fs::rename(&tmp, dest) {
        let _ = fs::remove_file(&tmp);
        return Err(e).with_context(|| format!("couldn't save {}", dest.display()));
    }
    written.files.push(dest.to_path_buf());
    Ok(())
}

fn atomic_write(dest: &Path, contents: &str, written: &mut Written) -> Result<()> {
    let dir = dest.parent().context("no destination folder")?;
    let tmp = dir.join(format!(".ibtmp-{}", uuid::Uuid::new_v4()));
    if let Err(e) = fs::write(&tmp, contents) {
        let _ = fs::remove_file(&tmp);
        return Err(e).with_context(|| format!("couldn't write {}", dest.display()));
    }
    if let Err(e) = fs::rename(&tmp, dest) {
        let _ = fs::remove_file(&tmp);
        return Err(e).with_context(|| format!("couldn't save {}", dest.display()));
    }
    written.files.push(dest.to_path_buf());
    Ok(())
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let hex = |c: u8| (c as char).to_digit(16);
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let (Some(hi), Some(lo)) = (hex(b[i + 1]), hex(b[i + 2]))
        {
            out.push((hi * 16 + lo) as u8);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The files a `.gltf` points at (`buffers[].uri`, `images[].uri`), as
/// relative paths. `data:` URIs are embedded and skipped.
fn gltf_siblings(gltf: &Path) -> Result<Vec<PathBuf>> {
    let text = fs::read_to_string(gltf).context("couldn't read the model file")?;
    let json: serde_json::Value =
        serde_json::from_str(&text).context("that .gltf file isn't valid")?;
    let mut out = Vec::new();
    for key in ["buffers", "images"] {
        for item in json
            .get(key)
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
        {
            let Some(uri) = item.get("uri").and_then(|u| u.as_str()) else {
                continue;
            };
            if uri.starts_with("data:") {
                continue;
            }
            let rel = PathBuf::from(percent_decode(uri));
            if rel.as_os_str().is_empty()
                || !rel.components().all(|c| matches!(c, Component::Normal(_)))
            {
                bail!("the model refers to \"{uri}\", which is outside its own folder");
            }
            if !out.contains(&rel) {
                out.push(rel);
            }
        }
    }
    Ok(out)
}

fn slugify(title: &str) -> String {
    let mut slug = String::new();
    for c in title.chars() {
        if c.is_alphanumeric() {
            slug.extend(c.to_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-').to_string();
    if slug.is_empty() {
        "asset".into()
    } else {
        slug
    }
}

fn card_text(title: &str, rel: &str, source_name: &str, license: &LicenseInfo) -> String {
    let mut s = String::from("---\ntype: asset\n");
    s.push_str(&format!("title: {}\n", yaml_value(title)));
    s.push_str("status: working\n");
    s.push_str(&format!("file: {}\n", yaml_value(rel)));
    for (key, value) in [
        ("license", &license.name),
        ("source", &license.source),
        ("author", &license.author),
        ("url", &license.url),
        ("generated_by", &license.generated_by),
    ] {
        if let Some(v) = value.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
            s.push_str(&format!("{key}: {}\n", yaml_value(v)));
        }
    }
    s.push_str("---\n\n");
    let origin = match (&license.generated_by, &license.source) {
        (Some(g), _) if !g.trim().is_empty() => format!("generated with {}", g.trim()),
        (_, Some(src)) if !src.trim().is_empty() => format!("from {}", src.trim()),
        _ => format!("from the file {source_name}"),
    };
    s.push_str(&format!("`{rel}`, {origin}.\n"));
    s
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
    let mut written = Written::default();
    match import_inner(project, source, dest_subdir, title, license, &mut written) {
        Ok(info) => Ok(info),
        Err(e) => {
            written.undo();
            Err(e)
        }
    }
}

fn import_inner(
    project: &Path,
    source: &Path,
    dest_subdir: Option<&str>,
    title: &str,
    license: &LicenseInfo,
    written: &mut Written,
) -> Result<AssetInfo> {
    let project_canon = project
        .canonicalize()
        .with_context(|| format!("{} isn't a project folder", project.display()))?;
    let meta =
        fs::metadata(source).with_context(|| format!("couldn't find {}", source.display()))?;
    if !meta.is_file() {
        bail!("{} isn't a file", source.display());
    }
    let file_name = source
        .file_name()
        .and_then(|n| n.to_str())
        .context("that file's name can't be read")?;
    let Some(kind) = kind_of(file_name) else {
        bail!(
            "{file_name} isn't an image, sound, 3D model or font, so it can't be imported as an asset"
        );
    };
    let title = title.trim();
    let title = if title.is_empty() { file_name } else { title };

    // Destination folder.
    let mut dest_dir = project.join("assets");
    match dest_subdir.map(str::trim).filter(|s| !s.is_empty()) {
        Some(sub) => {
            for part in validate_subdir(sub)? {
                dest_dir.push(part);
            }
        }
        None => dest_dir.push(folder_for(kind)),
    }
    written.ensure_dir(&dest_dir)?;
    if !dest_dir.canonicalize()?.starts_with(&project_canon) {
        bail!("that folder isn't inside the project");
    }

    // The file itself.
    let name = unique_name(&dest_dir, &sanitize_name(file_name));
    let dest = dest_dir.join(&name);
    atomic_copy(source, &dest, written)?;

    // A .gltf needs its .bin and textures next to it.
    if extension(file_name) == "gltf" {
        let src_dir = match source.parent() {
            Some(p) if !p.as_os_str().is_empty() => p.canonicalize()?,
            _ => Path::new(".").canonicalize()?,
        };
        for rel in gltf_siblings(source)? {
            let from = src_dir.join(&rel);
            let inside = from
                .canonicalize()
                .map(|c| c.starts_with(&src_dir) && c.is_file())
                .unwrap_or(false);
            if !inside {
                bail!("the model needs {}, which isn't next to it", rel.display());
            }
            let to = dest_dir.join(&rel);
            if to.symlink_metadata().is_ok() {
                // Already there (e.g. an earlier import of the same model):
                // fine if it's the same file, otherwise it can't be renamed
                // because the model points at it by name.
                if fs::read(&from)? == fs::read(&to)? {
                    continue;
                }
                bail!(
                    "there's already a different {} in that folder",
                    rel.display()
                );
            }
            if let Some(parent) = to.parent() {
                written.ensure_dir(parent)?;
            }
            atomic_copy(&from, &to, written)?;
        }
    }

    // The Asset card.
    let rel = dest
        .strip_prefix(project)
        .context("destination outside the project")?
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/");
    let cards_dir = project.join(CARDS_DIR);
    written.ensure_dir(&cards_dir)?;
    if !cards_dir.canonicalize()?.starts_with(&project_canon) {
        bail!("the project's Context folder isn't inside the project");
    }
    let base = slugify(title);
    let mut slug = base.clone();
    let mut n = 2;
    while cards_dir
        .join(format!("{slug}.md"))
        .symlink_metadata()
        .is_ok()
    {
        slug = format!("{base}-{n}");
        n += 1;
    }
    atomic_write(
        &cards_dir.join(format!("{slug}.md")),
        &card_text(title, &rel, file_name, license),
        written,
    )?;

    scan(project)?
        .into_iter()
        .find(|a| a.path == rel)
        .context("the imported file didn't show up as an asset")
}

// ---------- Health, credits, preview ----------

pub fn health(project: &Path) -> Result<HealthReport> {
    let s = survey(project)?;
    let mut report = HealthReport::default();
    for a in &s.assets {
        if a.used_by.is_empty() {
            report.unused.push(a.path.clone());
        }
        let too_big_side = a.width.is_some_and(|w| w > MAX_IMAGE_SIDE)
            || a.height.is_some_and(|h| h > MAX_IMAGE_SIDE);
        if too_big_side || a.size_bytes > MAX_ASSET_BYTES {
            report.oversized.push(Oversized {
                path: a.path.clone(),
                width: a.width,
                height: a.height,
                size_bytes: a.size_bytes,
            });
        }
        if a.license.as_ref().and_then(|l| l.name.as_ref()).is_none() {
            report.unlicensed.push(a.path.clone());
        }
    }
    let mut missing = BTreeSet::new();
    for (from, to) in &s.refs {
        if to.contains(['*', '%', '{']) {
            continue;
        }
        if !project.join(to).exists() {
            missing.insert((from.clone(), to.clone()));
        }
    }
    report.missing_refs = missing
        .into_iter()
        .map(|(from, to)| MissingRef { from, to })
        .collect();
    Ok(report)
}

/// Markdown credits grouped by license, from the Asset cards.
pub fn credits(project: &Path) -> Result<String> {
    if !project.is_dir() {
        bail!("{} isn't a folder", project.display());
    }
    const UNKNOWN: &str = "License unknown";
    // Only cards whose file is still in the project.
    let cards: Vec<Card> = read_cards(project)
        .into_iter()
        .filter(|c| project.join(&c.file).is_file())
        .collect();
    let mut out = String::from("# Credits\n");
    if cards.is_empty() {
        out.push_str("\nNo assets yet.\n");
        return Ok(out);
    }
    let mut by_license: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut generated: Vec<String> = Vec::new();
    for c in &cards {
        let title = c.title.clone().unwrap_or_else(|| {
            Path::new(&c.file)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| c.file.clone())
        });
        let l = &c.license;
        if let Some(by) = &l.generated_by {
            let mut line = format!("- {title} — {by}");
            if let Some(name) = &l.name {
                line.push_str(&format!(" ({name})"));
            }
            generated.push(line);
            continue;
        }
        let author = l.author.as_deref().unwrap_or("unknown author");
        let mut line = format!("- {title} — {author}");
        let origin: Vec<&str> = [l.source.as_deref(), l.url.as_deref()]
            .into_iter()
            .flatten()
            .collect();
        if !origin.is_empty() {
            line.push_str(&format!(" ({})", origin.join(", ")));
        }
        by_license
            .entry(l.name.clone().unwrap_or_else(|| UNKNOWN.to_string()))
            .or_default()
            .push(line);
    }
    // Named licenses first (sorted), then unknown, then generated.
    let unknown = by_license.remove(UNKNOWN);
    let mut sections: Vec<(String, Vec<String>)> = by_license.into_iter().collect();
    if let Some(lines) = unknown {
        sections.push((UNKNOWN.to_string(), lines));
    }
    if !generated.is_empty() {
        sections.push(("Generated with AI".to_string(), generated));
    }
    for (heading, mut lines) in sections {
        lines.sort();
        out.push_str(&format!("\n## {heading}\n\n"));
        for line in lines {
            out.push_str(&line);
            out.push('\n');
        }
    }
    Ok(out)
}

fn mime_for(path: &str) -> &'static str {
    match extension(path).as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "ogg" | "opus" => "audio/ogg",
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "glb" => "model/gltf-binary",
        "gltf" => "model/gltf+json",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}

/// Reads a project file for preview. `path` is project-relative and must stay
/// inside the project. `max_bytes` defaults to `PREVIEW_CAP_BYTES`.
pub fn read_base64(project: &Path, path: &str, max_bytes: Option<u64>) -> Result<FilePayload> {
    let project_canon = project
        .canonicalize()
        .with_context(|| format!("{} isn't a project folder", project.display()))?;
    let rel = path.replace('\\', "/");
    let canon = project_canon
        .join(&rel)
        .canonicalize()
        .with_context(|| format!("couldn't find {path}"))?;
    if !canon.starts_with(&project_canon) {
        bail!("{path} isn't inside the project");
    }
    let meta = fs::metadata(&canon)?;
    if !meta.is_file() {
        bail!("{path} isn't a file");
    }
    let size = meta.len();
    let mime = mime_for(&rel).to_string();
    if size > max_bytes.unwrap_or(PREVIEW_CAP_BYTES) {
        return Ok(FilePayload {
            mime,
            base64: String::new(),
            truncated: true,
            size,
        });
    }
    let bytes = fs::read(&canon)?;
    Ok(FilePayload {
        mime,
        base64: base64::engine::general_purpose::STANDARD.encode(bytes),
        truncated: false,
        size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scaffold;
    use tempfile::TempDir;

    fn png(path: &Path, w: u32, h: u32) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        image::RgbaImage::from_pixel(w, h, image::Rgba([200, 30, 30, 255]))
            .save(path)
            .unwrap();
    }

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn plain_project() -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("project.godot"), "config_version=5\n");
        dir
    }

    fn template_project(dir: &TempDir) -> PathBuf {
        scaffold::create_project_from_template(dir.path(), "Game", "platformer-2d").unwrap()
    }

    fn cc0() -> LicenseInfo {
        LicenseInfo {
            name: Some("CC0-1.0".into()),
            source: Some("Poly Haven".into()),
            author: Some("Jane".into()),
            url: Some("https://example.com/a".into()),
            generated_by: None,
        }
    }

    #[test]
    fn kinds_by_extension_case_insensitive() {
        assert_eq!(kind_of("a/b.PNG"), Some(AssetKind::Image));
        assert_eq!(kind_of("x.jpeg"), Some(AssetKind::Image));
        assert_eq!(kind_of("x.OGG"), Some(AssetKind::Audio));
        assert_eq!(kind_of("x.glb"), Some(AssetKind::Model3d));
        assert_eq!(kind_of("x.woff2"), Some(AssetKind::Font));
        for not in [
            "a.png.import",
            "a.uid",
            "a.tscn",
            "a.tres",
            "a.gd",
            "a.txt",
            "png",
        ] {
            assert_eq!(kind_of(not), None, "{not}");
        }
    }

    #[test]
    fn scan_finds_assets_with_dimensions_and_skips_the_rest() {
        let dir = plain_project();
        let p = dir.path();
        png(&p.join("assets/images/hero.png"), 32, 16);
        write(&p.join("assets/images/hero.png.import"), "[remap]\n");
        write(
            &p.join("assets/logo.svg"),
            "<svg xmlns='http://www.w3.org/2000/svg'/>",
        );
        write(&p.join("assets/audio/jump.wav"), "RIFF");
        png(&p.join(".godot/imported/x.png"), 4, 4);
        png(&p.join("addons/some/icon.png"), 4, 4);
        png(&p.join(".hidden/icon.png"), 4, 4);
        write(&p.join(".ibproject/context/assets/x.png"), "not an asset");
        write(&p.join("main.tscn"), "[gd_scene]\n");

        let assets = scan(p).unwrap();
        let paths: Vec<_> = assets.iter().map(|a| a.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "assets/audio/jump.wav",
                "assets/images/hero.png",
                "assets/logo.svg"
            ]
        );
        let hero = &assets[1];
        assert_eq!(hero.kind, AssetKind::Image);
        assert_eq!((hero.width, hero.height), (Some(32), Some(16)));
        assert!(hero.size_bytes > 0);
        assert_eq!(assets[2].width, None);
        assert_eq!(assets[0].kind, AssetKind::Audio);
    }

    #[test]
    fn scan_does_not_follow_symlinks_out_of_the_project() {
        let dir = plain_project();
        let outside = tempfile::tempdir().unwrap();
        png(&outside.path().join("secret.png"), 4, 4);
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(outside.path(), dir.path().join("linked")).unwrap();
            std::os::unix::fs::symlink(
                outside.path().join("secret.png"),
                dir.path().join("secret.png"),
            )
            .unwrap();
        }
        assert!(scan(dir.path()).unwrap().is_empty());
    }

    #[test]
    fn scan_works_on_a_template_project() {
        let dir = tempfile::tempdir().unwrap();
        let project = template_project(&dir);
        let assets = scan(&project).unwrap();
        // Nothing under addons/infinabox is ever reported.
        assert!(assets.iter().all(|a| !a.path.starts_with("addons/")));
        let report = health(&project).unwrap();
        // The shipped template refers only to files that exist.
        assert!(report.missing_refs.is_empty(), "{:?}", report.missing_refs);
    }

    #[test]
    fn used_by_comes_from_res_references() {
        let dir = plain_project();
        let p = dir.path();
        png(&p.join("assets/images/hero.png"), 8, 8);
        png(&p.join("assets/images/tree.png"), 8, 8);
        write(
            &p.join("levels/one.tscn"),
            "[ext_resource type=\"Texture2D\" path=\"res://assets/images/hero.png\" id=\"1\"]\n",
        );
        write(
            &p.join("player.gd"),
            "extends Node\n# res://assets/images/tree.png is not used yet\nvar t = preload(\"res://assets/images/hero.png\")\n",
        );
        let assets = scan(p).unwrap();
        assert_eq!(assets[0].path, "assets/images/hero.png");
        assert_eq!(assets[0].used_by, ["levels/one.tscn", "player.gd"]);
        assert!(assets[1].used_by.is_empty());
        let report = health(p).unwrap();
        assert_eq!(report.unused, ["assets/images/tree.png"]);
    }

    #[test]
    fn cards_supply_license_and_card_path() {
        let dir = plain_project();
        let p = dir.path();
        png(&p.join("assets/images/hero.png"), 8, 8);
        write(
            &p.join(".ibproject/context/assets/hero.md"),
            "---\ntype: asset\ntitle: Hero\nfile: assets/images/hero.png\nlicense: CC-BY-4.0\nauthor: \"Ann: the artist\"\n---\nBody\n",
        );
        write(
            &p.join(".ibproject/context/assets/other.md"),
            "---\ntype: mechanic\nfile: assets/images/hero.png\nlicense: WRONG\n---\n",
        );
        let a = &scan(p).unwrap()[0];
        let l = a.license.as_ref().unwrap();
        assert_eq!(l.name.as_deref(), Some("CC-BY-4.0"));
        assert_eq!(l.author.as_deref(), Some("Ann: the artist"));
        assert_eq!(l.source, None);
        assert_eq!(a.card.as_deref(), Some("assets/hero.md"));
    }

    #[test]
    fn import_copies_into_kind_folder_and_writes_card() {
        let dir = plain_project();
        let src_dir = tempfile::tempdir().unwrap();
        let src = src_dir.path().join("My Hero (final)!.png");
        png(&src, 10, 20);

        let info = import_file(dir.path(), &src, None, "Hero: the sprite", &cc0()).unwrap();
        assert_eq!(info.path, "assets/images/My Hero final.png");
        assert_eq!(info.kind, AssetKind::Image);
        assert_eq!((info.width, info.height), (Some(10), Some(20)));
        assert_eq!(info.card.as_deref(), Some("assets/hero-the-sprite.md"));
        assert_eq!(
            info.license.as_ref().unwrap().name.as_deref(),
            Some("CC0-1.0")
        );
        assert!(src.exists(), "the source is left alone");

        let card = fs::read_to_string(
            dir.path()
                .join(".ibproject/context/assets/hero-the-sprite.md"),
        )
        .unwrap();
        assert!(
            card.starts_with("---\ntype: asset\ntitle: \"Hero: the sprite\"\nstatus: working\n")
        );
        assert!(card.contains("file: assets/images/My Hero final.png\n"));
        assert!(card.contains("license: CC0-1.0\n"));
        assert!(card.contains("url: \"https://example.com/a\"\n"));
        assert!(!card.contains("generated_by"));
        // No leftovers.
        let leftovers: Vec<_> = fs::read_dir(dir.path().join("assets/images"))
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(leftovers.len(), 1);
    }

    #[test]
    fn import_folders_by_kind_and_subdir() {
        let dir = plain_project();
        let s = tempfile::tempdir().unwrap();
        for (name, folder) in [("a.ogg", "audio"), ("b.glb", "models"), ("c.ttf", "fonts")] {
            let f = s.path().join(name);
            fs::write(&f, b"data").unwrap();
            let info = import_file(dir.path(), &f, None, name, &LicenseInfo::default()).unwrap();
            assert_eq!(info.path, format!("assets/{folder}/{name}"));
        }
        let f = s.path().join("d.png");
        png(&f, 2, 2);
        let info = import_file(
            dir.path(),
            &f,
            Some("characters/hero"),
            "D",
            &LicenseInfo::default(),
        )
        .unwrap();
        assert_eq!(info.path, "assets/characters/hero/d.png");
    }

    #[test]
    fn import_never_overwrites_and_numbers_clashes() {
        let dir = plain_project();
        let s = tempfile::tempdir().unwrap();
        let f = s.path().join("hero.png");
        png(&f, 2, 2);
        let a = import_file(dir.path(), &f, None, "Hero", &cc0()).unwrap();
        let b = import_file(dir.path(), &f, None, "Hero", &cc0()).unwrap();
        let c = import_file(dir.path(), &f, None, "Hero", &cc0()).unwrap();
        assert_eq!(a.path, "assets/images/hero.png");
        assert_eq!(b.path, "assets/images/hero 2.png");
        assert_eq!(c.path, "assets/images/hero 3.png");
        assert_eq!(b.card.as_deref(), Some("assets/hero-2.md"));
        assert_eq!(c.card.as_deref(), Some("assets/hero-3.md"));
        assert_eq!(scan(dir.path()).unwrap().len(), 3);
    }

    #[test]
    fn import_name_with_nothing_left_becomes_asset() {
        let dir = plain_project();
        let s = tempfile::tempdir().unwrap();
        let f = s.path().join("日本!.png");
        png(&f, 2, 2);
        // Letters of any script are kept; symbols are not.
        let info = import_file(dir.path(), &f, None, "", &LicenseInfo::default()).unwrap();
        assert_eq!(info.path, "assets/images/日本.png");
        assert_eq!(sanitize_name("!!!.png"), "asset.png");
        assert_eq!(sanitize_name("..."), "asset");
        assert_eq!(sanitize_name("a b-c_d.e.PNG"), "a b-c_d.e.PNG");
    }

    #[test]
    fn import_rejects_bad_sources_and_folders_without_writing() {
        let dir = plain_project();
        let s = tempfile::tempdir().unwrap();
        let f = s.path().join("a.png");
        png(&f, 2, 2);
        let d = LicenseInfo::default();
        assert!(import_file(dir.path(), s.path(), None, "x", &d).is_err());
        assert!(import_file(dir.path(), &s.path().join("nope.png"), None, "x", &d).is_err());
        let txt = s.path().join("notes.txt");
        fs::write(&txt, "x").unwrap();
        assert!(import_file(dir.path(), &txt, None, "x", &d).is_err());
        for bad in [
            "../x",
            "a/../../x",
            "/abs",
            ".hidden",
            "a/.git",
            "///",
            "a:b",
        ] {
            assert!(
                import_file(dir.path(), &f, Some(bad), "x", &d).is_err(),
                "{bad:?}"
            );
        }
        assert!(!dir.path().join("assets").exists());
        assert!(!dir.path().join(".ibproject").exists());
        assert!(!dir.path().join("x").exists());
    }

    #[cfg(unix)]
    #[test]
    fn import_refuses_to_write_through_a_symlinked_assets_folder() {
        let dir = plain_project();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("assets")).unwrap();
        let s = tempfile::tempdir().unwrap();
        let f = s.path().join("a.png");
        png(&f, 2, 2);
        let err = import_file(dir.path(), &f, None, "A", &LicenseInfo::default());
        assert!(err.is_err());
        assert_eq!(
            fs::read_dir(outside.path()).unwrap().count(),
            0,
            "nothing escaped"
        );
    }

    #[cfg(unix)]
    #[test]
    fn import_accepts_a_symlink_to_a_file_but_not_to_a_folder() {
        let dir = plain_project();
        let s = tempfile::tempdir().unwrap();
        let f = s.path().join("real.png");
        png(&f, 2, 2);
        let link = s.path().join("link.png");
        std::os::unix::fs::symlink(&f, &link).unwrap();
        assert!(import_file(dir.path(), &link, None, "L", &LicenseInfo::default()).is_ok());
        let folder = s.path().join("folder.png");
        fs::create_dir(&folder).unwrap();
        let dlink = s.path().join("dlink.png");
        std::os::unix::fs::symlink(&folder, &dlink).unwrap();
        assert!(import_file(dir.path(), &dlink, None, "L", &LicenseInfo::default()).is_err());
    }

    fn write_gltf(dir: &Path, name: &str, extra_image: bool) {
        let images = if extra_image {
            r#","images":[{"uri":"textures/wood%20grain.png"},{"uri":"data:image/png;base64,AAAA"}]"#
        } else {
            ""
        };
        write(
            &dir.join(name),
            &format!(
                r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"uri":"model.bin","byteLength":4}},{{"uri":"data:application/octet-stream;base64,AAAA","byteLength":3}}]{images}}}"#
            ),
        );
    }

    #[test]
    fn import_gltf_brings_its_bin_and_textures() {
        let dir = plain_project();
        let s = tempfile::tempdir().unwrap();
        write_gltf(s.path(), "tree.gltf", true);
        fs::write(s.path().join("model.bin"), b"\0\0\0\0").unwrap();
        png(&s.path().join("textures/wood grain.png"), 4, 4);

        let info = import_file(
            dir.path(),
            &s.path().join("tree.gltf"),
            None,
            "Tree",
            &cc0(),
        )
        .unwrap();
        assert_eq!(info.path, "assets/models/tree.gltf");
        assert_eq!(info.kind, AssetKind::Model3d);
        assert!(dir.path().join("assets/models/model.bin").is_file());
        assert!(
            dir.path()
                .join("assets/models/textures/wood grain.png")
                .is_file()
        );
        // A second import of the same model reuses identical siblings.
        let again = import_file(
            dir.path(),
            &s.path().join("tree.gltf"),
            None,
            "Tree",
            &cc0(),
        )
        .unwrap();
        assert_eq!(again.path, "assets/models/tree 2.gltf");
    }

    #[test]
    fn import_gltf_with_missing_or_escaping_sibling_rolls_back() {
        let dir = plain_project();
        let s = tempfile::tempdir().unwrap();
        write_gltf(s.path(), "tree.gltf", false);
        // model.bin is missing.
        let err = import_file(
            dir.path(),
            &s.path().join("tree.gltf"),
            None,
            "Tree",
            &cc0(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("model.bin"), "{err}");
        assert!(
            !dir.path().join("assets").exists(),
            "assets/ was rolled back"
        );
        assert!(!dir.path().join(".ibproject").exists());

        write(
            &s.path().join("evil.gltf"),
            r#"{"buffers":[{"uri":"../secret.bin"}]}"#,
        );
        fs::write(s.path().join("../secret.bin"), b"x").ok();
        assert!(import_file(dir.path(), &s.path().join("evil.gltf"), None, "E", &cc0()).is_err());
        assert!(!dir.path().join("assets").exists());
    }

    #[test]
    fn import_rollback_keeps_what_was_already_there() {
        let dir = plain_project();
        let p = dir.path();
        png(&p.join("assets/images/existing.png"), 2, 2);
        let s = tempfile::tempdir().unwrap();
        write_gltf(s.path(), "tree.gltf", false); // missing bin
        assert!(import_file(p, &s.path().join("tree.gltf"), None, "T", &cc0()).is_err());
        assert!(p.join("assets/images/existing.png").is_file());
        assert!(!p.join("assets/models").exists());
    }

    #[test]
    fn health_reports_missing_oversized_and_unlicensed() {
        let dir = plain_project();
        let p = dir.path();
        png(&p.join("assets/images/big.png"), 4200, 8);
        png(&p.join("assets/images/ok.png"), 8, 8);
        write(
            &p.join(".ibproject/context/assets/ok.md"),
            "---\ntype: asset\ntitle: Ok\nfile: assets/images/ok.png\nlicense: CC0-1.0\n---\n",
        );
        write(
            &p.join(".ibproject/context/assets/nolicense.md"),
            "---\ntype: asset\ntitle: Big\nfile: assets/images/big.png\n---\n",
        );
        write(
            &p.join("main.tscn"),
            "[ext_resource path=\"res://assets/images/ok.png\"]\n[ext_resource path=\"res://assets/images/gone.png\"]\n[ext_resource path=\"res://icon.svg\"]\n",
        );
        write(
            &p.join("script.gd"),
            "# res://commented/out.png\nvar a = load(\"res://levels/level_%d.tscn\" % 1)\nvar b = load(\"res://sfx/*.wav\")\nvar c = \"res://addons/infinabox/runtime.gd\"\nvar d = \"res://assets/images/gone.png\"\n",
        );
        write(&p.join("addons/infinabox/runtime.gd"), "extends Node\n");
        write(&p.join("icon.svg"), "<svg/>");

        let r = health(p).unwrap();
        assert_eq!(
            r.missing_refs,
            [
                MissingRef {
                    from: "main.tscn".into(),
                    to: "assets/images/gone.png".into()
                },
                MissingRef {
                    from: "script.gd".into(),
                    to: "assets/images/gone.png".into()
                },
            ]
        );
        assert_eq!(r.oversized.len(), 1);
        assert_eq!(r.oversized[0].path, "assets/images/big.png");
        assert_eq!(r.oversized[0].width, Some(4200));
        assert_eq!(r.unlicensed, ["assets/images/big.png", "icon.svg"]);
        assert_eq!(r.unused, ["assets/images/big.png"]);
    }

    #[test]
    fn health_flags_files_over_20_mb() {
        let dir = plain_project();
        let f = dir.path().join("assets/audio/long.wav");
        fs::create_dir_all(f.parent().unwrap()).unwrap();
        let file = fs::File::create(&f).unwrap();
        file.set_len(MAX_ASSET_BYTES + 1).unwrap();
        let r = health(dir.path()).unwrap();
        assert_eq!(r.oversized.len(), 1);
        assert_eq!(r.oversized[0].size_bytes, MAX_ASSET_BYTES + 1);
    }

    #[test]
    fn credits_group_by_license() {
        let dir = plain_project();
        let p = dir.path();
        let s = tempfile::tempdir().unwrap();
        for n in ["a.png", "b.png", "c.png", "d.png", "e.png"] {
            png(&s.path().join(n), 2, 2);
        }
        let with = |license: Option<&str>,
                    author: Option<&str>,
                    source: Option<&str>,
                    url: Option<&str>,
                    gen_by: Option<&str>| LicenseInfo {
            name: license.map(Into::into),
            author: author.map(Into::into),
            source: source.map(Into::into),
            url: url.map(Into::into),
            generated_by: gen_by.map(Into::into),
        };
        import_file(
            p,
            &s.path().join("a.png"),
            None,
            "Tree",
            &with(
                Some("CC0-1.0"),
                Some("Jane"),
                Some("Poly Haven"),
                Some("https://ph.example/tree"),
                None,
            ),
        )
        .unwrap();
        import_file(
            p,
            &s.path().join("b.png"),
            None,
            "Rock",
            &with(Some("CC-BY-4.0"), Some("Bob"), Some("Kenney"), None, None),
        )
        .unwrap();
        import_file(
            p,
            &s.path().join("c.png"),
            None,
            "Mystery",
            &with(None, None, None, None, None),
        )
        .unwrap();
        import_file(
            p,
            &s.path().join("d.png"),
            None,
            "Cloud",
            &with(
                Some("Generated"),
                None,
                Some("Generated"),
                None,
                Some("cloudflare / flux"),
            ),
        )
        .unwrap();
        import_file(
            p,
            &s.path().join("e.png"),
            None,
            "Bush",
            &with(Some("CC0-1.0"), None, None, None, None),
        )
        .unwrap();
        // A card whose file is gone is not credited.
        write(
            &p.join(".ibproject/context/assets/ghost.md"),
            "---\ntype: asset\ntitle: Ghost\nfile: assets/images/ghost.png\nlicense: MIT\n---\n",
        );

        let c = credits(p).unwrap();
        let expected = "# Credits\n\
\n## CC-BY-4.0\n\n- Rock — Bob (Kenney)\n\
\n## CC0-1.0\n\n- Bush — unknown author\n- Tree — Jane (Poly Haven, https://ph.example/tree)\n\
\n## License unknown\n\n- Mystery — unknown author\n\
\n## Generated with AI\n\n- Cloud — cloudflare / flux (Generated)\n";
        assert_eq!(c, expected);
    }

    #[test]
    fn credits_of_an_empty_project() {
        let dir = plain_project();
        assert_eq!(
            credits(dir.path()).unwrap(),
            "# Credits\n\nNo assets yet.\n"
        );
        let t = tempfile::tempdir().unwrap();
        let project = template_project(&t);
        assert!(credits(&project).unwrap().contains("No assets yet."));
    }

    #[test]
    fn read_base64_reads_and_caps() {
        let dir = plain_project();
        let p = dir.path();
        png(&p.join("assets/images/a.png"), 4, 4);
        let bytes = fs::read(p.join("assets/images/a.png")).unwrap();
        let r = read_base64(p, "assets/images/a.png", None).unwrap();
        assert_eq!(r.mime, "image/png");
        assert!(!r.truncated);
        assert_eq!(r.size, bytes.len() as u64);
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(&r.base64)
                .unwrap(),
            bytes
        );
        let capped = read_base64(p, "assets/images/a.png", Some(10)).unwrap();
        assert!(capped.truncated);
        assert!(capped.base64.is_empty());
        assert_eq!(capped.size, bytes.len() as u64);
        assert_eq!(mime_for("x.jpg"), "image/jpeg");
        assert_eq!(mime_for("x.mp3"), "audio/mpeg");
        assert_eq!(mime_for("x.glb"), "model/gltf-binary");
        assert_eq!(mime_for("x.dat"), "application/octet-stream");
    }

    #[test]
    fn read_base64_stays_inside_the_project() {
        let dir = plain_project();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret.txt"), "s").unwrap();
        let rel_out = format!(
            "../{}/secret.txt",
            outside.path().file_name().unwrap().to_string_lossy()
        );
        // Sibling temp dirs share a parent only sometimes; cover both ways.
        assert!(read_base64(dir.path(), &rel_out, None).is_err());
        assert!(read_base64(dir.path(), "../../etc/passwd", None).is_err());
        assert!(read_base64(dir.path(), "/etc/passwd", None).is_err());
        assert!(read_base64(dir.path(), "missing.png", None).is_err());
        assert!(read_base64(dir.path(), ".", None).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                outside.path().join("secret.txt"),
                dir.path().join("link.png"),
            )
            .unwrap();
            assert!(read_base64(dir.path(), "link.png", None).is_err());
        }
    }

    #[test]
    fn front_matter_round_trips_tricky_values() {
        for v in [
            "plain",
            "a: b",
            "x # y",
            "say \"hi\"",
            "back\\slash",
            "- dash",
            "",
        ] {
            let text = format!("---\nk: {}\n---\n", yaml_value(v));
            assert_eq!(
                parse_front_matter(&text),
                [("k".to_string(), v.to_string())],
                "{v:?}"
            );
        }
        assert!(parse_front_matter("no front matter").is_empty());
        assert!(parse_front_matter("---\nk: v\n").is_empty());
    }
}
