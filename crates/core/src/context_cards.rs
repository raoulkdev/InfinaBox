//! Typed Context cards (spec §7.2): Markdown files with a small YAML
//! front-matter under `<project>/.ibproject/context/`, read and written by
//! both the person (through the Context section) and the AI (through the
//! InfinaBox MCP server).
//!
//! Front-matter is a deliberately small YAML subset: `key: value`,
//! `key: [a, b]`, `key:` followed by `- item` lines, and double-quoted
//! strings. Keys the app doesn't know are kept when a card is rewritten, and
//! the body is preserved byte for byte.
//!
//! Phase C contract (frozen, see `docs/superpowers/plans/2026-09-29-phase-c-make-the-whole-game.md`):
//! the types and signatures here are mirrored in `src/lib/studio-types.ts`.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::ffi::OsStr;
use std::fs;
use std::io::{Read as _, Write as _};
use std::path::{Component, Path, PathBuf};

use anyhow::{Context as _, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

/// What a card is about. Written to front-matter as `type: <kebab-case>`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum CardType {
    Concept,
    Mechanic,
    Character,
    Level,
    Story,
    Asset,
    StyleGuide,
    Task,
    Playtest,
    /// No `type`, or one this version doesn't know.
    Other,
}

/// The front-matter fields the app understands.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct CardMeta {
    #[serde(rename = "type")]
    pub card_type: Option<CardType>,
    pub title: Option<String>,
    /// "todo" / "doing" / "done" for tasks and bugs; free text elsewhere
    /// ("working", "draft", ...).
    pub status: Option<String>,
    /// Paths of other cards, relative to the context folder, `/`-separated.
    pub links: Vec<String>,
    /// Project files (scenes, scripts) that implement this card.
    pub implemented_in: Vec<String>,
    pub tags: Vec<String>,
    /// Every other key, as written (values are plain strings or lists of
    /// strings), kept across rewrites.
    pub extra: BTreeMap<String, Vec<String>>,
}

/// A whole card.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Card {
    /// Relative to the context folder, `/`-separated (`mechanics/jumping.md`).
    pub path: String,
    pub meta: CardMeta,
    /// Everything after the front-matter.
    pub body: String,
    /// Set when the header couldn't be read: the raw header text, so the UI
    /// can show it instead of losing it. `meta` is then empty.
    pub header_error: Option<String>,
}

/// What lists, boards and maps show.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CardSummary {
    pub path: String,
    pub card_type: CardType,
    /// The `title`, else the first `# heading`, else the file name.
    pub title: String,
    pub status: Option<String>,
    /// Existing cards this one links to.
    pub links: Vec<String>,
    /// Link targets that don't exist.
    pub broken_links: Vec<String>,
    /// Cards that link here.
    pub backlinks: Vec<String>,
    /// The page's icon (an emoji), from the `icon` header key.
    pub icon: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct BoardColumn {
    pub status: String,
    pub cards: Vec<CardSummary>,
}

/// Columns in the order todo, doing, done, then any other status found;
/// cards without a status are in `todo`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Board {
    pub columns: Vec<BoardColumn>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LinkEdge {
    pub from: String,
    pub to: String,
    pub broken: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LinkGraph {
    pub nodes: Vec<CardSummary>,
    pub edges: Vec<LinkEdge>,
}

/// Where Context lives inside a project (spec §9).
const CONTEXT_DIR: &[&str] = &[".ibproject", "context"];
const CARD_EXTENSION: &str = "md";
const FENCE: &str = "---";
const KNOWN_KEYS: [&str; 6] = ["type", "title", "status", "links", "implemented_in", "tags"];

impl CardType {
    /// The `type:` value this is written as.
    pub fn as_str(self) -> &'static str {
        match self {
            CardType::Concept => "concept",
            CardType::Mechanic => "mechanic",
            CardType::Character => "character",
            CardType::Level => "level",
            CardType::Story => "story",
            CardType::Asset => "asset",
            CardType::StyleGuide => "style-guide",
            CardType::Task => "task",
            CardType::Playtest => "playtest",
            CardType::Other => "other",
        }
    }

    /// Unknown strings are `Other`.
    fn from_str_lenient(s: &str) -> CardType {
        match s.trim().to_ascii_lowercase().as_str() {
            "concept" => CardType::Concept,
            "mechanic" => CardType::Mechanic,
            "character" => CardType::Character,
            "level" => CardType::Level,
            "story" => CardType::Story,
            "asset" => CardType::Asset,
            "style-guide" => CardType::StyleGuide,
            "task" => CardType::Task,
            "playtest" => CardType::Playtest,
            _ => CardType::Other,
        }
    }
}

// ---------------------------------------------------------------------------
// Front-matter: splitting, parsing, writing
// ---------------------------------------------------------------------------

/// Where the header sits in a card's text.
enum Split {
    /// No `---` first line.
    None,
    /// `---` opened but never closed; everything after the opener.
    Unclosed(String),
    Closed {
        /// Byte range of the header text (between the two fences).
        header: std::ops::Range<usize>,
        /// Byte offset where the body starts (after the closing fence line).
        body_start: usize,
    },
}

fn is_fence(line: &str) -> bool {
    line.trim_end() == FENCE
}

fn split_header(text: &str) -> Split {
    let first_end = text.find('\n').map(|i| i + 1).unwrap_or(text.len());
    if !is_fence(&text[..first_end]) {
        return Split::None;
    }
    let mut pos = first_end;
    while pos < text.len() {
        let end = text[pos..]
            .find('\n')
            .map(|i| pos + i + 1)
            .unwrap_or(text.len());
        if is_fence(&text[pos..end]) {
            return Split::Closed {
                header: first_end..pos,
                body_start: end,
            };
        }
        pos = end;
    }
    Split::Unclosed(text[first_end..].to_string())
}

/// A parsed front-matter value.
enum Value {
    Scalar(String),
    List(Vec<String>),
    /// `key:` with nothing after it.
    Empty,
}

/// Parses one scalar: a double-quoted string (`\"`, `\\`, `\n`, `\t`, `\r`
/// escapes) or a bare value (which may contain colons).
fn parse_scalar(s: &str) -> Result<String, String> {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix('"') {
        let (value, after) = parse_quoted(rest)?;
        if !after.trim().is_empty() {
            return Err(format!("unexpected text after the closing quote in `{s}`"));
        }
        Ok(value)
    } else {
        Ok(s.to_string())
    }
}

/// Reads a quoted string whose opening quote is already consumed; returns
/// the value and the text after the closing quote.
fn parse_quoted(rest: &str) -> Result<(String, &str), String> {
    let mut out = String::new();
    let mut chars = rest.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Ok((out, &rest[i + 1..])),
            '\\' => match chars.next() {
                Some((_, '"')) => out.push('"'),
                Some((_, '\\')) => out.push('\\'),
                Some((_, 'n')) => out.push('\n'),
                Some((_, 't')) => out.push('\t'),
                Some((_, 'r')) => out.push('\r'),
                Some((_, other)) => {
                    out.push('\\');
                    out.push(other);
                }
                None => break,
            },
            c => out.push(c),
        }
    }
    Err(format!(
        "a quoted value is missing its closing quote: \"{rest}"
    ))
}

/// `[a, "b, c", d]` -> items. Commas inside quotes don't split.
fn parse_flow_list(s: &str) -> Result<Vec<String>, String> {
    let inner = s
        .trim()
        .strip_prefix('[')
        .and_then(|r| r.strip_suffix(']'))
        .ok_or_else(|| format!("a list that starts with [ must end with ]: `{s}`"))?;
    let mut items = Vec::new();
    let mut rest = inner.trim_start();
    while !rest.is_empty() {
        let (item, after) = if let Some(quoted) = rest.strip_prefix('"') {
            let (v, after) = parse_quoted(quoted)?;
            let after = after.trim_start();
            if !(after.is_empty() || after.starts_with(',')) {
                return Err(format!(
                    "unexpected text after a quoted list item: `{after}`"
                ));
            }
            (Some(v), after)
        } else {
            let end = rest.find(',').unwrap_or(rest.len());
            let bare = rest[..end].trim();
            ((!bare.is_empty()).then(|| bare.to_string()), &rest[end..])
        };
        if let Some(item) = item {
            items.push(item);
        }
        rest = after.strip_prefix(',').unwrap_or(after).trim_start();
    }
    Ok(items)
}

fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Parses the text between the fences. Blank lines and `#` comment lines are
/// skipped (comments are not kept when the card is rewritten).
fn parse_meta(header: &str) -> Result<CardMeta, String> {
    let lines: Vec<&str> = header.lines().collect();
    let mut meta = CardMeta::default();
    let mut seen = HashSet::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        i += 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if line.starts_with([' ', '\t']) || trimmed.starts_with('-') {
            return Err(format!("line doesn't belong to any key: `{trimmed}`"));
        }
        let Some((key, rest)) = line.split_once(':') else {
            return Err(format!("expected `key: value`, found `{trimmed}`"));
        };
        if !valid_key(key) {
            return Err(format!("`{key}` isn't a valid key"));
        }
        if !(rest.is_empty() || rest.starts_with([' ', '\t'])) {
            return Err(format!("expected a space after `{key}:`"));
        }
        if !seen.insert(key.to_string()) {
            return Err(format!("`{key}` appears twice"));
        }
        let rest = rest.trim();
        let value = if rest.is_empty() {
            let mut items = Vec::new();
            while i < lines.len() {
                let t = lines[i].trim();
                let item = if t == "-" {
                    ""
                } else if let Some(item) = t.strip_prefix("- ") {
                    item
                } else {
                    break;
                };
                items.push(parse_scalar(item)?);
                i += 1;
            }
            if items.is_empty() {
                Value::Empty
            } else {
                Value::List(items)
            }
        } else if rest.starts_with('[') {
            Value::List(parse_flow_list(rest)?)
        } else {
            Value::Scalar(parse_scalar(rest)?)
        };
        assign(&mut meta, key, value);
    }
    Ok(meta)
}

fn assign(meta: &mut CardMeta, key: &str, value: Value) {
    let first = |v: &Value| match v {
        Value::Scalar(s) => Some(s.clone()),
        Value::List(l) => l.first().cloned(),
        Value::Empty => None,
    };
    let list = |v: Value| match v {
        // A bare `key:` is `Empty`, so an empty scalar here was quoted.
        Value::Scalar(s) => vec![s],
        Value::List(l) => l,
        Value::Empty => Vec::new(),
    };
    match key {
        "type" => meta.card_type = first(&value).map(|s| CardType::from_str_lenient(&s)),
        "title" => meta.title = first(&value),
        "status" => meta.status = first(&value),
        "links" => meta.links = list(value),
        "implemented_in" => meta.implemented_in = list(value),
        "tags" => meta.tags = list(value),
        other => {
            meta.extra.insert(other.to_string(), list(value));
        }
    }
}

/// Whether a value has to be written in double quotes to read back the same.
fn needs_quotes(s: &str, in_list: bool) -> bool {
    s.is_empty()
        || s != s.trim()
        || s.contains(": ")
        || s.ends_with(':')
        || s.contains(['#', '"', '\'', '\n', '\r', '\t'])
        || s.starts_with(['[', '{'])
        || (in_list && s.contains([',', '[', ']']))
}

fn scalar_text(s: &str, in_list: bool) -> String {
    if !needs_quotes(s, in_list) {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn list_text(items: &[String]) -> String {
    let inner: Vec<String> = items.iter().map(|i| scalar_text(i, true)).collect();
    format!("[{}]", inner.join(", "))
}

/// The header lines (without fences) for `meta`, in canonical order.
///
/// `CardMeta` doesn't remember whether an extra key was a scalar or a list in
/// the source, so a one-item extra is written as a scalar.
fn header_text(meta: &CardMeta) -> Result<String> {
    let mut out = String::new();
    if let Some(t) = meta.card_type {
        out.push_str(&format!("type: {}\n", t.as_str()));
    }
    if let Some(t) = &meta.title {
        out.push_str(&format!("title: {}\n", scalar_text(t, false)));
    }
    if let Some(s) = &meta.status {
        out.push_str(&format!("status: {}\n", scalar_text(s, false)));
    }
    for (key, items) in [
        ("links", &meta.links),
        ("implemented_in", &meta.implemented_in),
        ("tags", &meta.tags),
    ] {
        if !items.is_empty() {
            out.push_str(&format!("{key}: {}\n", list_text(items)));
        }
    }
    for (key, items) in &meta.extra {
        if !valid_key(key) {
            bail!("`{key}` isn't a valid front-matter key");
        }
        if KNOWN_KEYS.contains(&key.as_str()) {
            bail!("`{key}` is a known field and can't also be an extra key");
        }
        match items.as_slice() {
            [] => out.push_str(&format!("{key}:\n")),
            [one] => out.push_str(&format!("{key}: {}\n", scalar_text(one, false))),
            many => out.push_str(&format!("{key}: {}\n", list_text(many))),
        }
    }
    Ok(out)
}

/// The full text of a card: header (when there is anything to say) + body.
fn card_text(meta: &CardMeta, body: &str) -> Result<String> {
    let header = header_text(meta)?;
    if header.is_empty() {
        // No header. A body that itself opens with `---` would be read as
        // one, so it gets an explicit empty header.
        let first_end = body.find('\n').map(|i| i + 1).unwrap_or(body.len());
        if is_fence(&body[..first_end]) {
            return Ok(format!("{FENCE}\n{FENCE}\n{body}"));
        }
        return Ok(body.to_string());
    }
    Ok(format!("{FENCE}\n{header}{FENCE}\n{body}"))
}

/// Reads a card's text as it would be read from disk. Never fails: a header
/// that can't be read becomes `header_error`.
pub fn parse_card(path: &str, text: &str) -> Card {
    match split_header(text) {
        Split::None => Card {
            path: path.to_string(),
            meta: CardMeta::default(),
            body: text.to_string(),
            header_error: None,
        },
        Split::Unclosed(raw) => Card {
            path: path.to_string(),
            meta: CardMeta::default(),
            body: String::new(),
            header_error: Some(raw),
        },
        Split::Closed { header, body_start } => {
            let raw = &text[header];
            match parse_meta(raw) {
                Ok(meta) => Card {
                    path: path.to_string(),
                    meta,
                    body: text[body_start..].to_string(),
                    header_error: None,
                },
                Err(_) => Card {
                    path: path.to_string(),
                    meta: CardMeta::default(),
                    body: text[body_start..].to_string(),
                    header_error: Some(raw.to_string()),
                },
            }
        }
    }
}

/// Why a card's header can't be read, in plain words (for callers that want
/// to refuse it). `None` when it reads fine or there is no header.
pub fn header_problem(text: &str) -> Option<String> {
    match split_header(text) {
        Split::None => None,
        Split::Unclosed(_) => Some(
            "the front-matter starts with --- but never ends: add a closing --- line".to_string(),
        ),
        Split::Closed { header, .. } => parse_meta(&text[header]).err(),
    }
}

// ---------------------------------------------------------------------------
// Summaries, links, board, graph
// ---------------------------------------------------------------------------

fn first_heading(body: &str) -> Option<String> {
    let mut in_fence = false;
    for line in body.lines() {
        let t = line.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            in_fence = !in_fence;
        } else if !in_fence {
            if let Some(h) = line.strip_prefix("# ") {
                let h = h.trim();
                if !h.is_empty() {
                    return Some(h.to_string());
                }
            }
        }
    }
    None
}

/// The file name without its folder and `.md`.
pub fn file_stem(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.strip_suffix(".md").unwrap_or(name).to_string()
}

/// The title a card shows: `title`, else the first `# heading`, else its file name.
pub fn display_title(card: &Card) -> String {
    card.meta
        .title
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .or_else(|| first_heading(&card.body))
        .unwrap_or_else(|| file_stem(&card.path))
}

/// Lexically normalises `/`-separated parts; `None` if `..` climbs out.
fn normalize(parts: impl Iterator<Item = String>) -> Option<String> {
    let mut out: Vec<String> = Vec::new();
    for part in parts {
        match part.as_str() {
            "" | "." => {}
            ".." => {
                out.pop()?;
            }
            _ => out.push(part),
        }
    }
    (!out.is_empty()).then(|| out.join("/"))
}

/// The existing card a link points at, if any. Links are relative to the
/// context folder; one that doesn't match there is also tried relative to
/// the linking card's own folder.
fn resolve_link(from: &str, raw: &str, existing: &HashSet<String>) -> Option<String> {
    let target = raw.split('#').next().unwrap_or("").trim();
    if target.is_empty() {
        return None;
    }
    let target_parts = || target.split('/').map(str::to_string);
    let from_dir = from.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let from_parts = from_dir.split('/').map(str::to_string);
    [
        normalize(target_parts()),
        normalize(from_parts.chain(target_parts())),
    ]
    .into_iter()
    .flatten()
    .find(|c| existing.contains(c))
}

fn summarize(cards: &[Card]) -> (Vec<CardSummary>, Vec<LinkEdge>) {
    let existing: HashSet<String> = cards.iter().map(|c| c.path.clone()).collect();
    let mut backlinks: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut summaries = Vec::new();
    let mut edges = Vec::new();
    for card in cards {
        let mut links: Vec<String> = Vec::new();
        let mut broken: Vec<String> = Vec::new();
        for raw in &card.meta.links {
            match resolve_link(&card.path, raw, &existing) {
                Some(target) => {
                    if !links.contains(&target) {
                        links.push(target.clone());
                        edges.push(LinkEdge {
                            from: card.path.clone(),
                            to: target.clone(),
                            broken: false,
                        });
                    }
                    if target != card.path {
                        backlinks
                            .entry(target)
                            .or_default()
                            .insert(card.path.clone());
                    }
                }
                None => {
                    if !broken.contains(raw) {
                        broken.push(raw.clone());
                        edges.push(LinkEdge {
                            from: card.path.clone(),
                            to: raw.clone(),
                            broken: true,
                        });
                    }
                }
            }
        }
        summaries.push(CardSummary {
            path: card.path.clone(),
            card_type: card.meta.card_type.unwrap_or(CardType::Other),
            title: display_title(card),
            status: card.meta.status.clone(),
            links,
            broken_links: broken,
            backlinks: Vec::new(),
            icon: card.meta.extra.get("icon").and_then(|v| v.first()).cloned().filter(|i| !i.is_empty()),
        });
    }
    for s in &mut summaries {
        if let Some(from) = backlinks.remove(&s.path) {
            s.backlinks = from.into_iter().collect();
        }
    }
    (summaries, edges)
}

// ---------------------------------------------------------------------------
// Disk access (same rules as the MCP server's context.rs)
// ---------------------------------------------------------------------------

fn context_root(project: &Path) -> PathBuf {
    let mut root = project.to_path_buf();
    for part in CONTEXT_DIR {
        root.push(part);
    }
    root
}

fn canonical_root(project: &Path) -> Result<Option<PathBuf>> {
    let root = context_root(project);
    match root.canonicalize() {
        Ok(p) => Ok(Some(p)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("couldn't open {}", root.display())),
    }
}

/// Validates a card path lexically: relative, no `..`, `.md`, no hidden or
/// unportable components. Returns the normalised path.
fn checked_card_path(rel: &str) -> Result<PathBuf> {
    let trimmed = rel.trim();
    if trimmed.is_empty() {
        bail!("path is empty");
    }
    let mut out = PathBuf::new();
    for comp in Path::new(trimmed).components() {
        match comp {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                bail!("refusing {rel}: paths must stay inside the context folder")
            }
        }
    }
    if out.as_os_str().is_empty() {
        bail!("refusing {rel}: not a file path");
    }
    for comp in out.components() {
        let part = comp
            .as_os_str()
            .to_str()
            .ok_or_else(|| anyhow!("refusing {rel}: not valid UTF-8"))?;
        if part.starts_with('.') {
            bail!("refusing {rel}: names starting with '.' are hidden and not allowed");
        }
        if part.contains('\\') || part.contains(':') {
            bail!("refusing {rel}: use '/' between folders, and no ':' in names");
        }
    }
    if out.extension().and_then(|e| e.to_str()) != Some(CARD_EXTENSION) {
        bail!("context cards are markdown files; use a path ending in .{CARD_EXTENSION}");
    }
    Ok(out)
}

fn to_slash(p: &Path) -> String {
    p.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn split_file(rel_path: &Path) -> Result<(Vec<&OsStr>, &OsStr)> {
    let mut parts: Vec<&OsStr> = rel_path.components().map(|c| c.as_os_str()).collect();
    let file = parts.pop().ok_or_else(|| anyhow!("not a file path"))?;
    Ok((parts, file))
}

/// Walks `dirs` down from the canonical root; every level must be a real
/// folder, never a symlink. With `create`, missing levels are made one at a
/// time so nothing is ever created outside the context folder.
fn resolve_dir(root: &Path, dirs: &[&OsStr], create: bool, rel: &str) -> Result<PathBuf> {
    let mut cur = root.to_path_buf();
    for part in dirs {
        cur.push(part);
        match fs::symlink_metadata(&cur) {
            Ok(meta) if meta.is_dir() => {}
            Ok(meta) if meta.file_type().is_symlink() => {
                bail!("refusing {rel}: a folder on the way is a symlink")
            }
            Ok(_) => bail!("refusing {rel}: a folder on the way is a file"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && create => {
                fs::create_dir(&cur)
                    .with_context(|| format!("couldn't create a folder for {rel}"))?;
                if !fs::symlink_metadata(&cur)?.is_dir() {
                    bail!("refusing {rel}: a folder on the way changed while being created");
                }
            }
            Err(e) => return Err(e).with_context(|| format!("no context card at {rel}")),
        }
    }
    let real = cur.canonicalize()?;
    if !real.starts_with(root) {
        bail!("refusing {rel}: it resolves outside the context folder");
    }
    Ok(real)
}

#[cfg(unix)]
fn same_file(a: &fs::Metadata, b: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    a.dev() == b.dev() && a.ino() == b.ino()
}

#[cfg(not(unix))]
fn same_file(_a: &fs::Metadata, b: &fs::Metadata) -> bool {
    b.is_file()
}

/// The exact text of a card file, refusing symlinks and paths that leave the
/// context folder. Returns the normalised path too.
fn read_text(project: &Path, path: &str) -> Result<(String, String)> {
    let rel_path = checked_card_path(path)?;
    let root = canonical_root(project)?
        .ok_or_else(|| anyhow!("this project has no Context folder yet"))?;
    let (dirs, file_name) = split_file(&rel_path)?;
    let parent = resolve_dir(&root, &dirs, false, path)?;
    let file_path = parent.join(file_name);
    let before =
        fs::symlink_metadata(&file_path).with_context(|| format!("no context card at {path}"))?;
    if before.file_type().is_symlink() {
        bail!("refusing {path}: it is a symlink");
    }
    if !before.is_file() {
        bail!("refusing {path}: not a file");
    }
    let mut file = fs::File::open(&file_path).with_context(|| format!("couldn't open {path}"))?;
    if !same_file(&before, &file.metadata()?) {
        bail!("refusing {path}: it changed while being opened");
    }
    let mut text = String::new();
    file.read_to_string(&mut text)
        .with_context(|| format!("couldn't read {path} as text"))?;
    Ok((to_slash(&rel_path), text))
}

/// Temp file beside the target + rename, creating folders as needed.
fn write_text(project: &Path, path: &str, text: &str) -> Result<String> {
    let rel_path = checked_card_path(path)?;
    let context = context_root(project);
    fs::create_dir_all(&context)
        .with_context(|| format!("couldn't create {}", context.display()))?;
    let root = context.canonicalize()?;
    let (dirs, file_name) = split_file(&rel_path)?;
    let parent = resolve_dir(&root, &dirs, true, path)?;
    let target = parent.join(file_name);
    if let Ok(meta) = fs::symlink_metadata(&target) {
        if meta.file_type().is_symlink() {
            bail!("refusing {path}: it is a symlink");
        }
        if meta.is_dir() {
            bail!("refusing {path}: it is a folder");
        }
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp = parent.join(format!(
        ".{}.{}-{nanos}.tmp",
        file_name.to_string_lossy(),
        std::process::id()
    ));
    let written = (|| -> Result<()> {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, &target)?;
        Ok(())
    })();
    if let Err(e) = written {
        let _ = fs::remove_file(&tmp);
        return Err(e.context(format!("couldn't write {path}")));
    }
    Ok(to_slash(&rel_path))
}

fn collect_paths(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        // `file_type` doesn't follow symlinks, so neither a symlinked folder
        // nor a symlinked card is walked.
        let ft = entry.file_type()?;
        let path = entry.path();
        if ft.is_dir() {
            collect_paths(root, &path, out)?;
        } else if ft.is_file() && path.extension().and_then(|e| e.to_str()) == Some(CARD_EXTENSION)
        {
            out.push(to_slash(path.strip_prefix(root).unwrap_or(&path)));
        }
    }
    Ok(())
}

/// Every card, sorted by path. A card that can't be read as text is left out.
fn load_cards(project: &Path) -> Result<Vec<Card>> {
    let Some(root) = canonical_root(project)? else {
        return Ok(Vec::new());
    };
    let mut paths = Vec::new();
    collect_paths(&root, &root, &mut paths)?;
    paths.sort();
    Ok(paths
        .into_iter()
        .filter_map(|p| {
            let (path, text) = read_text(project, &p).ok()?;
            Some(parse_card(&path, &text))
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Folders, moving and deleting (the notes tree)
// ---------------------------------------------------------------------------

/// Validates a path to a file or folder inside the context folder (no
/// extension rule). Same restrictions as `checked_card_path`.
fn checked_entry_path(rel: &str) -> Result<PathBuf> {
    let trimmed = rel.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        bail!("path is empty");
    }
    let mut out = PathBuf::new();
    for comp in Path::new(trimmed).components() {
        match comp {
            Component::Normal(part) => out.push(part),
            Component::CurDir => {}
            _ => bail!("refusing {rel}: paths must stay inside the context folder"),
        }
    }
    for comp in out.components() {
        let part = comp
            .as_os_str()
            .to_str()
            .ok_or_else(|| anyhow!("refusing {rel}: not valid UTF-8"))?;
        if part.starts_with('.') || part.contains('\\') || part.contains(':') {
            bail!("refusing {rel}: names can't start with '.' or contain ':' or '\\'");
        }
    }
    if out.as_os_str().is_empty() {
        bail!("refusing {rel}: not a path");
    }
    Ok(out)
}

/// The existing entry for `rel` under the canonical root: never a symlink,
/// always inside the root.
fn existing_entry(root: &Path, rel: &Path) -> Result<PathBuf> {
    let full = root.join(rel);
    let meta = fs::symlink_metadata(&full).with_context(|| format!("no such note or folder: {}", to_slash(rel)))?;
    if meta.file_type().is_symlink() {
        bail!("refusing {}: it's a symlink", to_slash(rel));
    }
    let real = full.canonicalize()?;
    if !real.starts_with(root) {
        bail!("refusing {}: it resolves outside the context folder", to_slash(rel));
    }
    Ok(real)
}

/// Every folder under the context folder (empty ones too), `/`-separated and
/// sorted. Hidden folders are skipped.
pub fn list_folders(project: &Path) -> Result<Vec<String>> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            if entry.file_name().to_string_lossy().starts_with('.') || !entry.file_type()?.is_dir() {
                continue;
            }
            let path = entry.path();
            out.push(to_slash(path.strip_prefix(root).unwrap_or(&path)));
            walk(root, &path, out)?;
        }
        Ok(())
    }
    let Some(root) = canonical_root(project)? else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    walk(&root, &root, &mut out)?;
    out.sort();
    Ok(out)
}

/// Makes a folder (and any missing parents) inside the context folder.
pub fn create_folder(project: &Path, path: &str) -> Result<()> {
    let rel = checked_entry_path(path)?;
    let root = canonical_root(project)?.ok_or_else(|| anyhow!("this project has no Context folder yet"))?;
    let parts: Vec<&OsStr> = rel.components().map(|c| c.as_os_str()).collect();
    resolve_dir(&root, &parts, true, path)?;
    Ok(())
}

/// Moves or renames a note (`.md` file) or a folder. Refuses to overwrite,
/// and to move a folder into itself. Links in other notes' headers that
/// pointed at the moved note (or at notes inside the moved folder) are
/// updated to the new place. Returns the new normalised path.
pub fn move_entry(project: &Path, from: &str, to: &str) -> Result<String> {
    let from_rel = checked_entry_path(from)?;
    let to_rel = checked_entry_path(to)?;
    let root = canonical_root(project)?.ok_or_else(|| anyhow!("this project has no Context folder yet"))?;
    let source = existing_entry(&root, &from_rel)?;
    let is_dir = fs::metadata(&source)?.is_dir();
    if !is_dir && (from_rel.extension().and_then(|e| e.to_str()) != Some(CARD_EXTENSION) || to_rel.extension().and_then(|e| e.to_str()) != Some(CARD_EXTENSION)) {
        bail!("notes are markdown files; keep the .{CARD_EXTENSION} ending");
    }
    if to_rel.starts_with(&from_rel) && to_rel != from_rel {
        bail!("a folder can't be moved into itself");
    }
    if to_rel == from_rel {
        return Ok(to_slash(&to_rel));
    }
    let to_parts: Vec<&OsStr> = to_rel.components().map(|c| c.as_os_str()).collect();
    let (dirs, name) = to_parts.split_at(to_parts.len() - 1);
    let parent = resolve_dir(&root, dirs, true, to)?;
    let dest = parent.join(name[0]);
    if fs::symlink_metadata(&dest).is_ok() {
        bail!("there's already something called {} there", name[0].to_string_lossy());
    }

    // Which notes' paths change, for fixing links afterwards.
    let renames: Vec<(String, String)> = if is_dir {
        let mut inside = Vec::new();
        collect_paths(&root, &source, &mut inside)?;
        inside
            .into_iter()
            .map(|p| {
                let old = to_slash(Path::new(&p));
                let tail = old.strip_prefix(&format!("{}/", to_slash(&from_rel))).unwrap_or(&old).to_string();
                (old, format!("{}/{}", to_slash(&to_rel), tail))
            })
            .collect()
    } else {
        vec![(to_slash(&from_rel), to_slash(&to_rel))]
    };

    fs::rename(&source, &dest).with_context(|| format!("couldn't move {from}"))?;
    if is_dir {
        follow_folder_icons(project, &root, &to_slash(&from_rel), Some(&to_slash(&to_rel)))?;
    }

    // Fix header links in every note (including moved ones, whose relative
    // targets may name each other by old path).
    let map: std::collections::HashMap<String, String> = renames.into_iter().collect();
    for card in load_cards(project)? {
        if card.header_error.is_some() || !card.meta.links.iter().any(|l| map.contains_key(l)) {
            continue;
        }
        let mut meta = card.meta.clone();
        for link in &mut meta.links {
            if let Some(new) = map.get(link) {
                *link = new.clone();
            }
        }
        write_card(project, &card.path, &meta, &card.body)?;
    }
    Ok(to_slash(&to_rel))
}

const FOLDER_ICONS_FILE: &str = ".icons.json";

/// Folder icons, `folder path -> icon`. Kept in a hidden file inside the
/// context folder; a missing or damaged file is no icons.
pub fn folder_icons(project: &Path) -> Result<std::collections::BTreeMap<String, String>> {
    let Some(root) = canonical_root(project)? else {
        return Ok(Default::default());
    };
    Ok(fs::read_to_string(root.join(FOLDER_ICONS_FILE))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default())
}

fn save_folder_icons(root: &Path, icons: &std::collections::BTreeMap<String, String>) -> Result<()> {
    let path = root.join(FOLDER_ICONS_FILE);
    if icons.is_empty() {
        let _ = fs::remove_file(&path);
        return Ok(());
    }
    let tmp = root.join(format!("{FOLDER_ICONS_FILE}.tmp"));
    fs::write(&tmp, serde_json::to_string_pretty(icons)? + "\n")?;
    fs::rename(&tmp, &path).context("couldn't save folder icons")
}

/// Sets (or, with `None`, removes) a folder's icon. An icon is a short piece
/// of text, normally one emoji.
pub fn set_folder_icon(project: &Path, folder: &str, icon: Option<&str>) -> Result<()> {
    let rel = to_slash(&checked_entry_path(folder)?);
    let root = canonical_root(project)?.ok_or_else(|| anyhow!("this project has no Context folder yet"))?;
    if !existing_entry(&root, Path::new(&rel))?.is_dir() {
        bail!("{folder} isn't a folder");
    }
    let mut icons = folder_icons(project)?;
    match icon.map(str::trim).filter(|i| !i.is_empty()) {
        Some(icon) => {
            if icon.chars().count() > 16 || icon.contains(['\n', '\r']) {
                bail!("an icon is a single emoji");
            }
            icons.insert(rel, icon.to_string());
        }
        None => {
            icons.remove(&rel);
        }
    }
    save_folder_icons(&root, &icons)
}

/// Moves icons with a folder that was renamed or moved (`from` -> `to`), or
/// drops them when `to` is `None` (the folder was deleted).
fn follow_folder_icons(project: &Path, root: &Path, from: &str, to: Option<&str>) -> Result<()> {
    let icons = folder_icons(project)?;
    let prefix = format!("{from}/");
    let mut next = std::collections::BTreeMap::new();
    let mut changed = false;
    for (path, icon) in icons {
        if path == from || path.starts_with(&prefix) {
            changed = true;
            if let Some(to) = to {
                next.insert(format!("{to}{}", &path[from.len()..]), icon);
            }
        } else {
            next.insert(path, icon);
        }
    }
    if changed {
        save_folder_icons(root, &next)?;
    }
    Ok(())
}

/// Deletes a note or a folder with everything in it. The context folder
/// itself can't be deleted.
pub fn delete_entry(project: &Path, path: &str) -> Result<()> {
    let rel = checked_entry_path(path)?;
    let root = canonical_root(project)?.ok_or_else(|| anyhow!("this project has no Context folder yet"))?;
    let target = existing_entry(&root, &rel)?;
    if target == root {
        bail!("the Context folder itself can't be deleted");
    }
    if fs::metadata(&target)?.is_dir() {
        follow_folder_icons(project, &root, &to_slash(&rel), None)?;
        fs::remove_dir_all(&target)
    } else {
        fs::remove_file(&target)
    }
    .with_context(|| format!("couldn't delete {path}"))
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Every card, sorted by path. Hidden files and folders are skipped.
pub fn list_cards(project: &Path) -> Result<Vec<CardSummary>> {
    Ok(summarize(&load_cards(project)?).0)
}

pub fn read_card(project: &Path, path: &str) -> Result<Card> {
    let (path, text) = read_text(project, path)?;
    Ok(parse_card(&path, &text))
}

/// Writes (creating folders as needed) atomically. Refuses paths outside
/// the context folder and non-`.md` paths.
pub fn write_card(project: &Path, path: &str, meta: &CardMeta, body: &str) -> Result<Card> {
    let text = card_text(meta, body)?;
    let written = write_text(project, path, &text)?;
    read_card(project, &written)
}

/// Changes only the `status` line of the header.
pub fn set_status(project: &Path, path: &str, status: &str) -> Result<Card> {
    let status = status.trim();
    if status.is_empty() {
        bail!("a status can't be empty");
    }
    if status.contains(['\n', '\r']) {
        bail!("a status is a single line");
    }
    let (path, text) = read_text(project, path)?;
    let new_line = format!("status: {}", scalar_text(status, false));

    let updated = match split_header(&text) {
        Split::None => format!("{FENCE}\n{new_line}\n{FENCE}\n{text}"),
        Split::Unclosed(_) => bail!(
            "{path} has front-matter that can't be read (it never closes), so its status was \
             left alone; fix the header first"
        ),
        Split::Closed { header, .. } => {
            if let Err(why) = parse_meta(&text[header.clone()]) {
                bail!(
                    "{path} has front-matter that can't be read ({why}), so its status was left \
                     alone; fix the header first"
                );
            }
            let eol = if text[header.clone()].contains("\r\n") {
                "\r\n"
            } else {
                "\n"
            };
            let lines: Vec<&str> = text[header.clone()].split_inclusive('\n').collect();
            let key_of = |l: &str| -> Option<String> {
                if l.starts_with([' ', '\t']) {
                    return None;
                }
                l.split_once(':').map(|(k, _)| k.to_string())
            };
            let mut out = String::new();
            let mut i = 0;
            let mut done = false;
            while i < lines.len() {
                let line = lines[i];
                i += 1;
                if !done && key_of(line).as_deref() == Some("status") {
                    let ending = &line[line.trim_end_matches(['\r', '\n']).len()..];
                    out.push_str(&new_line);
                    out.push_str(if ending.is_empty() { eol } else { ending });
                    // A block-style value's `- item` lines belong to it.
                    while i < lines.len() {
                        let t = lines[i].trim();
                        if t == "-" || t.starts_with("- ") {
                            i += 1;
                        } else {
                            break;
                        }
                    }
                    done = true;
                    continue;
                }
                out.push_str(line);
            }
            if !done {
                // Canonical spot: after `title`, else after `type`, else first.
                let pos = |key: &str| lines.iter().position(|l| key_of(l).as_deref() == Some(key));
                let after = pos("title").or(pos("type")).map(|p| p + 1).unwrap_or(0);
                out = String::new();
                for (n, line) in lines.iter().enumerate() {
                    if n == after {
                        out.push_str(&format!("{new_line}{eol}"));
                    }
                    out.push_str(line);
                }
                if after >= lines.len() {
                    out.push_str(&format!("{new_line}{eol}"));
                }
            }
            format!(
                "{}{}{}",
                &text[..header.start],
                out,
                &text[header.end..]
            )
        }
    };
    write_text(project, &path, &updated)?;
    read_card(project, &path)
}

fn column_key(status: Option<&str>) -> String {
    match status.map(str::trim).filter(|s| !s.is_empty()) {
        None => "todo".to_string(),
        Some(s) => {
            let lower = s.to_lowercase();
            if ["todo", "doing", "done"].contains(&lower.as_str()) {
                lower
            } else {
                s.to_string()
            }
        }
    }
}

/// A board over cards of the given types (all types when empty).
pub fn board(project: &Path, types: &[CardType]) -> Result<Board> {
    let cards = list_cards(project)?;
    let mut columns: BTreeMap<String, Vec<CardSummary>> = BTreeMap::new();
    for name in ["todo", "doing", "done"] {
        columns.insert(name.to_string(), Vec::new());
    }
    for card in cards {
        if !types.is_empty() && !types.contains(&card.card_type) {
            continue;
        }
        columns
            .entry(column_key(card.status.as_deref()))
            .or_default()
            .push(card);
    }
    let mut ordered = Vec::new();
    for name in ["todo", "doing", "done"] {
        if let Some(cards) = columns.remove(name) {
            ordered.push(BoardColumn {
                status: name.to_string(),
                cards,
            });
        }
    }
    // What's left is in alphabetical order already (BTreeMap).
    ordered.extend(
        columns
            .into_iter()
            .map(|(status, cards)| BoardColumn { status, cards }),
    );
    Ok(Board { columns: ordered })
}

pub fn graph(project: &Path) -> Result<LinkGraph> {
    let (nodes, edges) = summarize(&load_cards(project)?);
    Ok(LinkGraph { nodes, edges })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn put(project: &Path, rel: &str, text: &str) {
        let p = context_root(project).join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }

    fn get(project: &Path, rel: &str) -> String {
        fs::read_to_string(context_root(project).join(rel)).unwrap()
    }

    fn s(v: &str) -> String {
        v.to_string()
    }

    fn round_trip(meta: CardMeta, body: &str) {
        let text = card_text(&meta, body).unwrap();
        let card = parse_card("x.md", &text);
        assert_eq!(card.header_error, None, "{text:?}");
        assert_eq!(card.meta, meta, "{text:?}");
        assert_eq!(card.body, body, "{text:?}");
    }

    #[test]
    fn parses_every_accepted_form() {
        let text = "---\n\
type: mechanic\n\
title: \"Say \\\"hi\\\" \\\\ there: ok\"\n\
status: working: mostly\n\
links: [a.md, \"b, c.md\"]\n\
implemented_in:\n\
  - scripts/player.gd\n\
  - \"scenes/x: y.tscn\"\n\
tags: []\n\
owner: Sam\n\
mood:\n\
  - calm\n\
  - tense\n\
empty:\n\
---\n\
\n# Body\n";
        let card = parse_card("m/x.md", text);
        assert_eq!(card.header_error, None);
        let m = &card.meta;
        assert_eq!(m.card_type, Some(CardType::Mechanic));
        assert_eq!(m.title.as_deref(), Some("Say \"hi\" \\ there: ok"));
        assert_eq!(m.status.as_deref(), Some("working: mostly"));
        assert_eq!(m.links, vec!["a.md", "b, c.md"]);
        assert_eq!(m.implemented_in, vec!["scripts/player.gd", "scenes/x: y.tscn"]);
        assert!(m.tags.is_empty());
        assert_eq!(m.extra["owner"], vec!["Sam"]);
        assert_eq!(m.extra["mood"], vec!["calm", "tense"]);
        assert!(m.extra["empty"].is_empty());
        assert_eq!(card.body, "\n# Body\n");
    }

    #[test]
    fn unknown_type_is_other() {
        let card = parse_card("x.md", "---\ntype: banana\n---\n");
        assert_eq!(card.meta.card_type, Some(CardType::Other));
    }

    #[test]
    fn writer_emits_canonical_order_and_quotes_only_when_needed() {
        let mut meta = CardMeta {
            card_type: Some(CardType::Task),
            title: Some(s("Fix: the jump")),
            status: Some(s("doing")),
            links: vec![s("concept.md"), s("a, b.md")],
            implemented_in: vec![s("scripts/player.gd")],
            tags: vec![s("bug")],
            extra: BTreeMap::new(),
        };
        meta.extra.insert(s("zeta"), vec![s("z")]);
        meta.extra.insert(s("alpha"), vec![s("x"), s("y")]);
        meta.extra.insert(s("note"), vec![s("  spaced")]);
        meta.extra.insert(s("hash"), vec![s("a # b")]);
        meta.extra.insert(s("url"), vec![s("http://x.y")]);
        let text = card_text(&meta, "body\n").unwrap();
        assert_eq!(
            text,
            "---\ntype: task\ntitle: \"Fix: the jump\"\nstatus: doing\n\
links: [concept.md, \"a, b.md\"]\nimplemented_in: [scripts/player.gd]\ntags: [bug]\n\
alpha: [x, y]\nhash: \"a # b\"\nnote: \"  spaced\"\nurl: http://x.y\nzeta: z\n---\nbody\n"
        );
        assert!(text.lines().all(|l| l == l.trim_end()));
        round_trip(meta, "body\n");
    }

    #[test]
    fn round_trips_awkward_values() {
        for value in [
            "plain",
            "",
            " lead",
            "trail ",
            "a: b",
            "ends:",
            "x#y",
            "[not a list]",
            "{brace}",
            "say \"hi\"",
            "it's",
            "back\\slash",
            "two\nlines",
            "tab\there",
            "http://a.b:80/c",
            "a, b",
        ] {
            let meta = CardMeta {
                title: Some(value.to_string()),
                status: Some(value.to_string()),
                links: vec![value.to_string(), s("second.md")],
                tags: vec![value.to_string()],
                extra: BTreeMap::from([(s("k"), vec![value.to_string()])]),
                ..CardMeta::default()
            };
            round_trip(meta, "\n# Body\n\ntext");
        }
        // A one-item extra list is written as a scalar (CardMeta can't tell).
        round_trip(
            CardMeta {
                extra: BTreeMap::from([(s("many"), vec![s("a"), s("b, c"), s("")])]),
                ..CardMeta::default()
            },
            "",
        );
    }

    #[test]
    fn body_is_kept_byte_for_byte_with_or_without_a_header() {
        for body in [
            "",
            "no trailing newline",
            "\n\n  indented\r\nCRLF line\r\n",
            "---\nlooks like a header\n---\n",
            "---",
            "# Title\n---\nrule above\n",
        ] {
            round_trip(CardMeta::default(), body);
            round_trip(
                CardMeta {
                    title: Some(s("T")),
                    ..CardMeta::default()
                },
                body,
            );
        }
        let plain = parse_card("x.md", "# Just text\n");
        assert_eq!(plain.meta, CardMeta::default());
        assert_eq!(plain.body, "# Just text\n");
    }

    #[test]
    fn unreadable_headers_become_header_error() {
        let unclosed = "---\ntype: mechanic\ntitle: Oops\n\n# Body\n";
        let card = parse_card("x.md", unclosed);
        assert_eq!(
            card.header_error.as_deref(),
            Some("type: mechanic\ntitle: Oops\n\n# Body\n")
        );
        assert_eq!(card.meta, CardMeta::default());

        let garbage = "---\ntype: mechanic\nthis is not a pair\n---\nbody\n";
        let card = parse_card("x.md", garbage);
        assert_eq!(
            card.header_error.as_deref(),
            Some("type: mechanic\nthis is not a pair\n")
        );
        assert_eq!(card.body, "body\n");

        for bad in [
            "---\nkey:value\n---\n",
            "---\na: 1\na: 2\n---\n",
            "---\nlinks: [a\n---\n",
            "---\nt: \"open\n---\n",
            "---\n  - stray\n---\n",
            "---\nbad key: 1\n---\n",
        ] {
            assert!(parse_card("x.md", bad).header_error.is_some(), "{bad:?}");
            assert!(header_problem(bad).is_some(), "{bad:?}");
        }
        assert!(header_problem("---\ntype: task\n---\n").is_none());
        assert!(header_problem("# no header\n").is_none());
    }

    #[test]
    fn set_status_refuses_a_broken_header_and_leaves_it_alone() {
        let p = project();
        let broken = "---\ntype: task\nnot a pair\n---\nbody\n";
        put(p.path(), "t.md", broken);
        let err = set_status(p.path(), "t.md", "done").unwrap_err();
        assert!(format!("{err}").contains("can't be read"), "{err}");
        assert_eq!(get(p.path(), "t.md"), broken);
        put(p.path(), "u.md", "---\ntype: task\n");
        assert!(set_status(p.path(), "u.md", "done").is_err());
        assert_eq!(get(p.path(), "u.md"), "---\ntype: task\n");
    }

    #[test]
    fn set_status_changes_only_the_status_line() {
        let p = project();
        let original = "---\ntype: task\ntitle: Jump\nstatus: todo\nowner:   Sam  \n\
links: [a.md]\n---\n\n# Jump\r\nkeep   this  \n";
        put(p.path(), "t.md", original);
        let card = set_status(p.path(), "t.md", "done").unwrap();
        assert_eq!(card.meta.status.as_deref(), Some("done"));
        assert_eq!(get(p.path(), "t.md"), original.replace("status: todo", "status: done"));

        // Added after the title when absent.
        put(p.path(), "a.md", "---\ntype: task\ntitle: A\nlinks: [b.md]\n---\nbody");
        set_status(p.path(), "a.md", "doing").unwrap();
        assert_eq!(
            get(p.path(), "a.md"),
            "---\ntype: task\ntitle: A\nstatus: doing\nlinks: [b.md]\n---\nbody"
        );
        // ...after the type when there is no title...
        put(p.path(), "b.md", "---\ntype: task\ntags: [x]\n---\n");
        set_status(p.path(), "b.md", "doing").unwrap();
        assert_eq!(get(p.path(), "b.md"), "---\ntype: task\nstatus: doing\ntags: [x]\n---\n");
        // ...at the top when there is neither, in an empty header too.
        put(p.path(), "c.md", "---\ntags: [x]\n---\n");
        set_status(p.path(), "c.md", "doing").unwrap();
        assert_eq!(get(p.path(), "c.md"), "---\nstatus: doing\ntags: [x]\n---\n");
        put(p.path(), "d.md", "---\n---\nbody\n");
        set_status(p.path(), "d.md", "a: b").unwrap();
        assert_eq!(get(p.path(), "d.md"), "---\nstatus: \"a: b\"\n---\nbody\n");
        // A card with no front-matter gets one; its text is untouched.
        put(p.path(), "e.md", "# E\ntext\n");
        set_status(p.path(), "e.md", "done").unwrap();
        assert_eq!(get(p.path(), "e.md"), "---\nstatus: done\n---\n# E\ntext\n");
        assert!(set_status(p.path(), "e.md", "  ").is_err());
        assert!(set_status(p.path(), "e.md", "a\nb").is_err());
        assert!(set_status(p.path(), "../x.md", "done").is_err());
    }

    #[test]
    fn write_card_round_trips_and_only_changes_what_was_edited() {
        let p = project();
        let original = "---\ntype: mechanic\ntitle: Jumping\nstatus: working\n\
implemented_in: [scripts/player.gd, scenes/player.tscn]\n---\n\n# Jumping\n\nSpace jumps.\n";
        put(p.path(), "mechanics/jumping.md", original);
        let card = read_card(p.path(), "mechanics/jumping.md").unwrap();
        // Rewriting unchanged is a no-op on disk.
        write_card(p.path(), &card.path, &card.meta, &card.body).unwrap();
        assert_eq!(get(p.path(), "mechanics/jumping.md"), original);
        // Editing one field changes just that line.
        let mut meta = card.meta.clone();
        meta.title = Some(s("Jump and hop"));
        let written = write_card(p.path(), &card.path, &meta, &card.body).unwrap();
        assert_eq!(written.meta.title.as_deref(), Some("Jump and hop"));
        assert_eq!(
            get(p.path(), "mechanics/jumping.md"),
            original.replace("title: Jumping", "title: Jump and hop")
        );
    }

    #[test]
    fn folders_move_delete_and_links_follow_moves() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path();
        let meta = |links: &[&str]| CardMeta { links: links.iter().map(|s| s.to_string()).collect(), ..Default::default() };
        write_card(p, "world/city.md", &meta(&[]), "City\n").unwrap();
        write_card(p, "story.md", &meta(&["world/city.md"]), "Plot\n").unwrap();
        create_folder(p, "empty/inner").unwrap();
        assert_eq!(list_folders(p).unwrap(), ["empty", "empty/inner", "world"]);

        // A note moves; links to it follow.
        assert_eq!(move_entry(p, "world/city.md", "places/capital.md").unwrap(), "places/capital.md");
        assert_eq!(read_card(p, "story.md").unwrap().meta.links, ["places/capital.md"]);
        // A folder moves with its notes; links follow.
        move_entry(p, "places", "world/places").unwrap();
        assert_eq!(read_card(p, "story.md").unwrap().meta.links, ["world/places/capital.md"]);
        assert!(read_card(p, "world/places/capital.md").is_ok());

        // Refusals.
        assert!(move_entry(p, "world", "world/places/deeper").is_err());
        assert!(move_entry(p, "story.md", "world/places/capital.md").is_err());
        assert!(move_entry(p, "story.md", "story.txt").is_err());
        assert!(move_entry(p, "../x.md", "y.md").is_err());
        assert!(delete_entry(p, "..").is_err());

        // Icons: on a page (in its header) and on a folder (they follow moves).
        let mut m = meta(&[]);
        m.extra.insert("icon".into(), vec!["🗺️".into()]);
        write_card(p, "map.md", &m, "x\n").unwrap();
        assert_eq!(list_cards(p).unwrap().iter().find(|c| c.path == "map.md").unwrap().icon.as_deref(), Some("🗺️"));
        assert_eq!(read_card(p, "map.md").unwrap().meta.extra["icon"], ["🗺️"]);
        set_folder_icon(p, "world/places", Some("🏰")).unwrap();
        move_entry(p, "world/places", "world/cities").unwrap();
        assert_eq!(folder_icons(p).unwrap()["world/cities"], "🏰");
        assert!(set_folder_icon(p, "story.md", Some("x")).is_err());
        assert!(set_folder_icon(p, "world", Some("way too long to be an icon")).is_err());
        set_folder_icon(p, "world/cities", None).unwrap();
        assert!(folder_icons(p).unwrap().is_empty());
        set_folder_icon(p, "world", Some("🌍")).unwrap();

        delete_entry(p, "world").unwrap();
        assert!(folder_icons(p).unwrap().is_empty());
        assert!(read_card(p, "world/places/capital.md").is_err());
        assert_eq!(list_cards(p).unwrap().len(), 2);
    }

    #[test]
    fn write_card_is_atomic_creates_folders_and_refuses_bad_paths() {
        let p = project();
        let meta = CardMeta {
            card_type: Some(CardType::Level),
            ..CardMeta::default()
        };
        let card = write_card(p.path(), "levels/one/start.md", &meta, "hi").unwrap();
        assert_eq!(card.path, "levels/one/start.md");
        assert_eq!(card.meta.card_type, Some(CardType::Level));
        assert_eq!(card.body, "hi");
        // No temp files left behind.
        let names: Vec<_> = fs::read_dir(context_root(p.path()).join("levels/one"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["start.md"]);
        // `./` is normalised.
        assert_eq!(
            write_card(p.path(), "./x.md", &meta, "").unwrap().path,
            "x.md"
        );

        let secret = p.path().join("secret.md");
        fs::write(&secret, "outside").unwrap();
        for bad in [
            "script.gd",
            "noext",
            "../../secret.md",
            "a/../../../secret.md",
            "/etc/x.md",
            ".hidden.md",
            ".dir/x.md",
            "a\\b.md",
            "c:d.md",
            "",
            ".",
        ] {
            assert!(
                write_card(p.path(), bad, &meta, "x").is_err(),
                "should refuse {bad:?}"
            );
            assert!(read_card(p.path(), bad).is_err(), "should refuse {bad:?}");
        }
        assert_eq!(fs::read_to_string(&secret).unwrap(), "outside");

        // Extra keys can't collide with known fields.
        let bad_meta = CardMeta {
            extra: BTreeMap::from([(s("title"), vec![s("x")])]),
            ..CardMeta::default()
        };
        assert!(write_card(p.path(), "y.md", &bad_meta, "").is_err());
        assert!(!context_root(p.path()).join("y.md").exists());
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlink_escapes() {
        use std::os::unix::fs::symlink;
        let p = project();
        let outside = project();
        fs::write(outside.path().join("secret.md"), "---\ntype: task\n---\nx").unwrap();
        put(p.path(), "ok.md", "# ok");
        let root = context_root(p.path());
        symlink(outside.path().join("secret.md"), root.join("link.md")).unwrap();
        symlink(outside.path(), root.join("linkdir")).unwrap();

        let meta = CardMeta::default();
        assert!(read_card(p.path(), "link.md").is_err());
        assert!(read_card(p.path(), "linkdir/secret.md").is_err());
        assert!(write_card(p.path(), "link.md", &meta, "pwned").is_err());
        assert!(write_card(p.path(), "linkdir/new.md", &meta, "pwned").is_err());
        assert!(write_card(p.path(), "linkdir/a/b.md", &meta, "pwned").is_err());
        assert!(set_status(p.path(), "link.md", "done").is_err());
        assert!(!outside.path().join("new.md").exists());
        assert!(!outside.path().join("a").exists());
        assert_eq!(
            fs::read_to_string(outside.path().join("secret.md")).unwrap(),
            "---\ntype: task\n---\nx"
        );
        let listed: Vec<_> = list_cards(p.path())
            .unwrap()
            .into_iter()
            .map(|c| c.path)
            .collect();
        assert_eq!(listed, vec!["ok.md"]);
    }

    #[test]
    fn list_cards_summarises_links_backlinks_and_skips_hidden() {
        let p = project();
        assert!(list_cards(p.path()).unwrap().is_empty());
        put(
            p.path(),
            "concept.md",
            "---\ntype: concept\ntitle: The Game\nstatus: draft\n---\nbody",
        );
        put(
            p.path(),
            "mechanics/jump.md",
            "---\ntype: mechanic\nstatus: working\nlinks: [concept.md#pitch, ./mechanics/run.md, \
../concept.md, missing.md, ghost/none.md#x, concept.md]\n---\n# Jump it\n",
        );
        put(
            p.path(),
            "mechanics/run.md",
            "---\ntype: mechanic\nlinks: [run.md, mechanics/jump.md]\n---\n",
        );
        put(p.path(), "notes/plain.md", "no header, no heading");
        put(p.path(), "notes/headed.md", "intro\n\n```\n# not this\n```\n# Real Heading\n");
        put(p.path(), ".hidden/x.md", "x");
        put(p.path(), "notes/.secret.md", "x");
        put(p.path(), "notes/readme.txt", "x");

        let cards = list_cards(p.path()).unwrap();
        let paths: Vec<_> = cards.iter().map(|c| c.path.as_str()).collect();
        assert_eq!(
            paths,
            vec![
                "concept.md",
                "mechanics/jump.md",
                "mechanics/run.md",
                "notes/headed.md",
                "notes/plain.md"
            ]
        );
        let by = |path: &str| cards.iter().find(|c| c.path == path).unwrap();

        let jump = by("mechanics/jump.md");
        assert_eq!(jump.title, "Jump it");
        assert_eq!(jump.card_type, CardType::Mechanic);
        assert_eq!(jump.status.as_deref(), Some("working"));
        assert_eq!(jump.links, vec!["concept.md", "mechanics/run.md"]);
        assert_eq!(jump.broken_links, vec!["missing.md", "ghost/none.md#x"]);
        assert_eq!(jump.backlinks, vec!["mechanics/run.md"]);

        let concept = by("concept.md");
        assert_eq!(concept.title, "The Game");
        assert_eq!(concept.backlinks, vec!["mechanics/jump.md"]);

        let run = by("mechanics/run.md");
        assert_eq!(run.title, "run");
        assert_eq!(run.card_type, CardType::Mechanic);
        // A link to itself is a link but not a backlink.
        assert_eq!(run.links, vec!["mechanics/run.md", "mechanics/jump.md"]);
        assert_eq!(run.backlinks, vec!["mechanics/jump.md"]);

        assert_eq!(by("notes/headed.md").title, "Real Heading");
        let plain = by("notes/plain.md");
        assert_eq!(plain.title, "plain");
        assert_eq!(plain.card_type, CardType::Other);
    }

    #[test]
    fn broken_header_cards_still_list() {
        let p = project();
        put(p.path(), "bad.md", "---\ntype: task\ngarbage line\n---\n# Bad one\n");
        put(p.path(), "worse.md", "---\ntype: task\n");
        let cards = list_cards(p.path()).unwrap();
        assert_eq!(cards[0].title, "Bad one");
        assert_eq!(cards[0].card_type, CardType::Other);
        assert_eq!(cards[1].title, "worse");
        assert_eq!(read_card(p.path(), "worse.md").unwrap().header_error.as_deref(), Some("type: task\n"));
    }

    fn task(project: &Path, path: &str, status: Option<&str>, ty: CardType) {
        let meta = CardMeta {
            card_type: Some(ty),
            title: Some(path.to_string()),
            status: status.map(str::to_string),
            ..CardMeta::default()
        };
        write_card(project, path, &meta, "").unwrap();
    }

    #[test]
    fn board_orders_columns_and_filters_by_type() {
        let p = project();
        let empty = board(p.path(), &[]).unwrap();
        let names: Vec<_> = empty.columns.iter().map(|c| c.status.as_str()).collect();
        assert_eq!(names, vec!["todo", "doing", "done"]);
        assert!(empty.columns.iter().all(|c| c.cards.is_empty()));

        task(p.path(), "a.md", None, CardType::Task);
        task(p.path(), "b.md", Some("done"), CardType::Task);
        task(p.path(), "c.md", Some("blocked"), CardType::Task);
        task(p.path(), "d.md", Some("Doing"), CardType::Playtest);
        task(p.path(), "e.md", Some("archived"), CardType::Task);
        task(p.path(), "f.md", Some("working"), CardType::Mechanic);

        let all = board(p.path(), &[]).unwrap();
        let cols: Vec<(&str, Vec<&str>)> = all
            .columns
            .iter()
            .map(|c| (c.status.as_str(), c.cards.iter().map(|x| x.path.as_str()).collect()))
            .collect();
        assert_eq!(
            cols,
            vec![
                ("todo", vec!["a.md"]),
                ("doing", vec!["d.md"]),
                ("done", vec!["b.md"]),
                ("archived", vec!["e.md"]),
                ("blocked", vec!["c.md"]),
                ("working", vec!["f.md"]),
            ]
        );

        let tasks = board(p.path(), &[CardType::Task]).unwrap();
        let cols: Vec<_> = tasks.columns.iter().map(|c| (c.status.as_str(), c.cards.len())).collect();
        assert_eq!(
            cols,
            vec![("todo", 1), ("doing", 0), ("done", 1), ("archived", 1), ("blocked", 1)]
        );
    }

    #[test]
    fn graph_has_one_edge_per_link_including_broken_ones() {
        let p = project();
        put(p.path(), "a.md", "---\nlinks: [b.md, ./nope.md, b.md#x]\n---\n");
        put(p.path(), "b.md", "---\nlinks: [a.md]\n---\n");
        let g = graph(p.path()).unwrap();
        assert_eq!(g.nodes.len(), 2);
        assert_eq!(
            g.edges,
            vec![
                LinkEdge { from: s("a.md"), to: s("b.md"), broken: false },
                LinkEdge { from: s("a.md"), to: s("./nope.md"), broken: true },
                LinkEdge { from: s("b.md"), to: s("a.md"), broken: false },
            ]
        );
        assert!(graph(project().path()).unwrap().edges.is_empty());
    }

    #[test]
    fn every_template_has_readable_cards_without_broken_links() {
        let templates = crate::scaffold::list_templates();
        assert!(!templates.is_empty());
        for info in templates {
            let parent = project();
            let game = crate::scaffold::create_project_from_template(
                parent.path(),
                "Card Check",
                &info.id,
            )
            .unwrap_or_else(|e| panic!("{}: {e:#}", info.id));
            let mut count = 0;
            for summary in list_cards(&game).unwrap() {
                count += 1;
                let card = read_card(&game, &summary.path).unwrap();
                assert_eq!(
                    card.header_error, None,
                    "{}: {} has an unreadable header",
                    info.id, summary.path
                );
                assert!(
                    summary.broken_links.is_empty(),
                    "{}: {} has broken links {:?}",
                    info.id,
                    summary.path,
                    summary.broken_links
                );
                if summary.path.starts_with("mechanics/") {
                    assert_eq!(
                        card.meta.card_type,
                        Some(CardType::Mechanic),
                        "{}: {} needs `type: mechanic`",
                        info.id,
                        summary.path
                    );
                    assert!(
                        card.meta.status.is_some(),
                        "{}: {} needs a status",
                        info.id,
                        summary.path
                    );
                }
            }
            assert!(count > 0, "{} has no Context cards", info.id);
        }
    }
}
