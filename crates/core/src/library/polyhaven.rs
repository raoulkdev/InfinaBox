//! Poly Haven (polyhaven.com): CC0 HDRIs, textures and 3D models through its
//! public API (`INFINABOX_POLYHAVEN_BASE` overrides `https://api.polyhaven.com`
//! for tests).
//!
//! Written from the documented public API; not yet checked against the live
//! service (the tests run against a local fake with this shape):
//!
//! - `GET {base}/assets?t=<type>` (`hdris`, `textures`, `models`, or `all`)
//!   returns a JSON object keyed by asset id. Each value has `name`, `type`
//!   (0 hdri, 1 texture, 2 model), `authors` (an object whose keys are author
//!   names), `categories`, `tags` (arrays of strings) and `thumbnail_url`.
//! - `GET {base}/files/{id}` returns nested JSON of downloadable files.
//!   Models: `gltf` -> resolution (`1k`, `2k`, ...) -> `gltf` ->
//!   `{url, size, md5, include: {relative path: {url, size, md5}}}`.
//!   Textures: map name (`Diffuse`, `nor_gl`, ...) -> resolution -> `png`/`jpg`
//!   -> `{url, size, md5}`. HDRIs: `hdri` -> resolution -> `hdr`/`exr` ->
//!   `{url, size, md5}`.
//! - Everything on the site is CC0, so every item gets that license.
//!
//! Downloads are checked against the API's `size` when it gives one. The API
//! also gives an `md5`, but no md5 implementation is available to this crate,
//! so it is not checked.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use reqwest::blocking::Client;
use reqwest::redirect::Policy;
use serde_json::Value;

use super::{LibraryItem, LibraryProvider, LibraryProviderInfo, LibraryQuery};
use crate::assets::{AssetKind, LicenseInfo};

pub const DEFAULT_BASE: &str = "https://api.polyhaven.com";
pub const BASE_ENV: &str = "INFINABOX_POLYHAVEN_BASE";

const DEFAULT_LIMIT: u32 = 24;
const MAX_LIMIT: u32 = 60;

/// Time and size limits for every request. Tests shrink them.
#[derive(Clone, Copy)]
pub(crate) struct Limits {
    pub connect_timeout: Duration,
    pub total_timeout: Duration,
    pub json_cap: u64,
    pub file_cap: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(10),
            total_timeout: Duration::from_secs(120),
            json_cap: 8 * 1024 * 1024,
            file_cap: 100 * 1024 * 1024,
        }
    }
}

pub struct PolyHaven {
    pub base: String,
}

impl PolyHaven {
    pub fn from_env() -> Self {
        Self {
            base: std::env::var(BASE_ENV).unwrap_or_else(|_| DEFAULT_BASE.to_string()),
        }
    }

    pub(crate) fn search_with(
        &self,
        query: &LibraryQuery,
        limits: Limits,
    ) -> Result<Vec<LibraryItem>> {
        let types: &[&str] = match query.kind {
            Some(AssetKind::Model3d) => &["models"],
            Some(AssetKind::Image) => &["textures"],
            None => &["models", "textures"],
            // Poly Haven has no sounds or fonts.
            Some(_) => return Ok(Vec::new()),
        };
        let client = build_client(limits)?;
        let base = self.base()?;
        let needle = query.text.trim().to_lowercase();
        let mut found: Vec<LibraryItem> = Vec::new();
        for t in types {
            let body = get_capped(&client, &format!("{base}/assets?t={t}"), limits.json_cap)?;
            let list: Value = serde_json::from_slice(&body)
                .map_err(|_| anyhow!("Poly Haven sent back something InfinaBox couldn't read."))?;
            let Some(map) = list.as_object() else {
                bail!("Poly Haven sent back something InfinaBox couldn't read.");
            };
            for (id, entry) in map {
                if let Some(item) = to_item(id, entry, &needle) {
                    found.push(item);
                }
            }
        }
        found.sort_by(|a, b| {
            a.title
                .to_lowercase()
                .cmp(&b.title.to_lowercase())
                .then_with(|| a.id.cmp(&b.id))
        });
        let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT) as usize;
        found.truncate(limit);
        Ok(found)
    }

    pub(crate) fn fetch_with(
        &self,
        item: &LibraryItem,
        dest_dir: &Path,
        limits: Limits,
    ) -> Result<Vec<PathBuf>> {
        let id = &item.id;
        if id.is_empty()
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
            || id.starts_with('.')
        {
            bail!("That isn't a Poly Haven item.");
        }
        let client = build_client(limits)?;
        let base = self.base()?;
        let body = get_capped(&client, &format!("{base}/files/{id}"), limits.json_cap)?;
        let files: Value = serde_json::from_slice(&body)
            .map_err(|_| anyhow!("Poly Haven sent back something InfinaBox couldn't read."))?;
        let plan = plan_downloads(&files)?;

        fs::create_dir_all(dest_dir)
            .map_err(|e| anyhow!("Couldn't create the folder for the download: {e}"))?;
        let folder = create_unique_dir(dest_dir, id)?;
        let mut written = Vec::new();
        for file in &plan {
            let target = folder.join(&file.rel);
            let result = target
                .parent()
                .map_or(Ok(()), fs::create_dir_all)
                .map_err(|e| anyhow!("Couldn't save the download: {e}"))
                .and_then(|_| download_to(&client, &file.url, &target, file.size, limits.file_cap));
            if let Err(e) = result {
                // Only ever remove the folder this call just made.
                let _ = fs::remove_dir_all(&folder);
                return Err(e);
            }
            written.push(target);
        }
        Ok(written)
    }

    fn base(&self) -> Result<String> {
        let base = self.base.trim().trim_end_matches('/');
        let url = reqwest::Url::parse(base)
            .map_err(|_| anyhow!("The Poly Haven address isn't a web address."))?;
        if !matches!(url.scheme(), "http" | "https") {
            bail!("The Poly Haven address isn't a web address.");
        }
        Ok(base.to_string())
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
        self.search_with(query, Limits::default())
    }

    fn fetch(&self, item: &LibraryItem, dest_dir: &Path) -> Result<Vec<PathBuf>> {
        self.fetch_with(item, dest_dir, Limits::default())
    }
}

/// One entry of the search list, when it matches `needle` (lowercased).
fn to_item(id: &str, entry: &Value, needle: &str) -> Option<LibraryItem> {
    let name = entry.get("name").and_then(Value::as_str).unwrap_or(id);
    let strings = |key: &str| -> Vec<String> {
        entry
            .get(key)
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
            .unwrap_or_default()
    };
    if !needle.is_empty() {
        let hit = name.to_lowercase().contains(needle)
            || strings("categories")
                .iter()
                .chain(strings("tags").iter())
                .any(|s| s.to_lowercase().contains(needle));
        if !hit {
            return None;
        }
    }
    let kind = match entry.get("type").and_then(Value::as_u64) {
        Some(2) => AssetKind::Model3d,
        _ => AssetKind::Image,
    };
    let authors: Vec<&str> = entry
        .get("authors")
        .and_then(Value::as_object)
        .map(|o| o.keys().map(String::as_str).collect())
        .unwrap_or_default();
    let page = format!("https://polyhaven.com/a/{id}");
    Some(LibraryItem {
        provider: "polyhaven".into(),
        id: id.to_string(),
        title: name.to_string(),
        kind,
        license: LicenseInfo {
            name: Some("CC0-1.0".into()),
            source: Some("Poly Haven".into()),
            author: (!authors.is_empty()).then(|| authors.join(", ")),
            url: Some(page.clone()),
            generated_by: None,
        },
        thumbnail_url: entry
            .get("thumbnail_url")
            .and_then(Value::as_str)
            .map(str::to_string),
        page_url: Some(page),
    })
}

struct Planned {
    /// Path under the item's folder, always plain relative components.
    rel: PathBuf,
    url: String,
    size: Option<u64>,
}

/// Decides which files to download from a `/files/{id}` answer.
fn plan_downloads(files: &Value) -> Result<Vec<Planned>> {
    let unsupported = || anyhow!("Poly Haven doesn't offer files InfinaBox can use for this item.");
    if let Some(gltf) = files.get("gltf") {
        let entry = pick_resolution(gltf)
            .and_then(|res| res.get("gltf"))
            .ok_or_else(unsupported)?;
        let mut plan = vec![planned_main(entry)?];
        if let Some(include) = entry.get("include").and_then(Value::as_object) {
            for (rel, sub) in include {
                let rel = safe_relative(rel)?;
                plan.push(Planned {
                    rel,
                    url: file_url(sub)?,
                    size: sub.get("size").and_then(Value::as_u64),
                });
            }
        }
        return Ok(plan);
    }
    if let Some(hdri) = files.get("hdri") {
        let res = pick_resolution(hdri).ok_or_else(unsupported)?;
        let entry = res.get("hdr").or_else(|| res.get("exr")).ok_or_else(unsupported)?;
        return Ok(vec![planned_main(entry)?]);
    }
    // A texture: keyed by map name.
    let map = files.as_object().ok_or_else(unsupported)?;
    let find = |names: &[&str]| -> Option<&Value> {
        names.iter().find_map(|n| {
            map.iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(n))
                .map(|(_, v)| v)
        })
    };
    let color = find(&["Diffuse", "diff", "Color", "Albedo", "col"])
        .ok_or_else(|| anyhow!("Poly Haven doesn't offer a color map for this texture."))?;
    let mut plan = vec![planned_texture_map(color)?];
    if let Some(normal) = find(&["nor_gl", "Normal", "nor_dx"]) {
        if let Ok(p) = planned_texture_map(normal) {
            if p.rel != plan[0].rel {
                plan.push(p);
            }
        }
    }
    Ok(plan)
}

fn planned_texture_map(map: &Value) -> Result<Planned> {
    let res = pick_resolution(map)
        .ok_or_else(|| anyhow!("Poly Haven doesn't offer files InfinaBox can use for this item."))?;
    let entry = res
        .get("png")
        .or_else(|| res.get("jpg"))
        .ok_or_else(|| anyhow!("Poly Haven doesn't offer files InfinaBox can use for this item."))?;
    planned_main(entry)
}

fn planned_main(entry: &Value) -> Result<Planned> {
    let url = file_url(entry)?;
    let parsed = reqwest::Url::parse(&url).map_err(|_| anyhow!("Poly Haven sent a bad link."))?;
    let name = parsed
        .path_segments()
        .and_then(|mut s| s.next_back())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("Poly Haven sent a bad link."))?;
    Ok(Planned {
        rel: safe_relative(name)?,
        url,
        size: entry.get("size").and_then(Value::as_u64),
    })
}

/// The `url` of a file entry, which must be http or https.
fn file_url(entry: &Value) -> Result<String> {
    let url = entry
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("Poly Haven sent a file without a link."))?;
    let parsed = reqwest::Url::parse(url).map_err(|_| anyhow!("Poly Haven sent a bad link."))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        bail!("Poly Haven sent a link InfinaBox won't open.");
    }
    Ok(url.to_string())
}

/// A relative path made only of plain names (no `..`, no drive or root).
fn safe_relative(rel: &str) -> Result<PathBuf> {
    let path = PathBuf::from(rel);
    let ok = !rel.is_empty()
        && !rel.contains('\\')
        && path.components().all(|c| matches!(c, Component::Normal(_)));
    if !ok {
        bail!("Poly Haven sent a file name InfinaBox won't use.");
    }
    Ok(path)
}

/// The value for `1k` when present, else the smallest resolution (`2k` < `4k`).
fn pick_resolution(by_res: &Value) -> Option<&Value> {
    let map = by_res.as_object()?;
    if let Some(v) = map.get("1k") {
        return Some(v);
    }
    map.iter()
        .filter_map(|(k, v)| {
            let n: u32 = k.trim_end_matches(['k', 'K']).parse().ok()?;
            Some((n, v))
        })
        .min_by_key(|(n, _)| *n)
        .map(|(_, v)| v)
}

/// Creates `parent/<name>`, or `parent/<name>-2`, `-3`... if that exists.
fn create_unique_dir(parent: &Path, name: &str) -> Result<PathBuf> {
    for n in 1..1000 {
        let candidate = if n == 1 {
            parent.join(name)
        } else {
            parent.join(format!("{name}-{n}"))
        };
        match fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => bail!("Couldn't create the folder for the download: {e}"),
        }
    }
    bail!("Couldn't find a free folder name for the download.")
}

/// A client with the timeouts, User-Agent and redirect rule: a redirect is
/// followed only when it stays on the same host and port.
fn build_client(limits: Limits) -> Result<Client> {
    let policy = Policy::custom(|attempt| {
        let same = attempt.previous().first().is_some_and(|first| {
            first.host_str() == attempt.url().host_str()
                && first.port_or_known_default() == attempt.url().port_or_known_default()
        });
        if same && attempt.previous().len() < 5 {
            attempt.follow()
        } else {
            attempt.stop()
        }
    });
    Client::builder()
        .connect_timeout(limits.connect_timeout)
        .timeout(limits.total_timeout)
        .user_agent("InfinaBox")
        .redirect(policy)
        .build()
        .map_err(|_| anyhow!("Couldn't set up the connection to Poly Haven."))
}

fn net_error(e: &reqwest::Error) -> anyhow::Error {
    if e.is_timeout() {
        anyhow!("Poly Haven took too long to answer. Try again in a moment.")
    } else {
        anyhow!("Couldn't reach Poly Haven. Check your internet connection.")
    }
}

fn read_error(e: &io::Error) -> anyhow::Error {
    if e.kind() == io::ErrorKind::TimedOut {
        anyhow!("Poly Haven took too long to answer. Try again in a moment.")
    } else {
        anyhow!("The download from Poly Haven was cut off. Try again.")
    }
}

/// GET that turns every failure into a plain sentence.
fn get(client: &Client, url: &str, cap: u64) -> Result<reqwest::blocking::Response> {
    let resp = client.get(url).send().map_err(|e| net_error(&e))?;
    let status = resp.status();
    if status.is_redirection() {
        bail!("Poly Haven tried to send InfinaBox somewhere else, so it stopped.");
    }
    if !status.is_success() {
        if status.as_u16() == 404 {
            bail!("Poly Haven doesn't have that item (status 404).");
        }
        bail!(
            "Poly Haven didn't understand the request (status {}).",
            status.as_u16()
        );
    }
    if resp.content_length().is_some_and(|len| len > cap) {
        bail!("Poly Haven sent back more than expected, so InfinaBox stopped.");
    }
    Ok(resp)
}

fn get_capped(client: &Client, url: &str, cap: u64) -> Result<Vec<u8>> {
    let resp = get(client, url, cap)?;
    let mut body = Vec::new();
    resp.take(cap + 1)
        .read_to_end(&mut body)
        .map_err(|e| read_error(&e))?;
    if body.len() as u64 > cap {
        bail!("Poly Haven sent back more than expected, so InfinaBox stopped.");
    }
    Ok(body)
}

/// Streams a file to `target` (never overwriting), stopping at `cap` bytes
/// and checking the size the API promised.
fn download_to(
    client: &Client,
    url: &str,
    target: &Path,
    expected_size: Option<u64>,
    cap: u64,
) -> Result<()> {
    let resp = get(client, url, cap)?;
    let mut out = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .map_err(|e| anyhow!("Couldn't save the download: {e}"))?;
    let mut limited = resp.take(cap + 1);
    let copied = io::copy(&mut limited, &mut out).map_err(|e| read_error(&e))?;
    out.flush()
        .map_err(|e| anyhow!("Couldn't save the download: {e}"))?;
    if copied > cap {
        bail!("Poly Haven sent back more than expected, so InfinaBox stopped.");
    }
    if let Some(size) = expected_size {
        if size != copied {
            bail!("A file from Poly Haven arrived incomplete. Try again.");
        }
    }
    Ok(())
}
