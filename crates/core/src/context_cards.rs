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
//! Wave 0 stub — task CC fills in the bodies.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Result, bail};
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

const NOT_YET: &str = "not implemented yet (Phase C, task CC)";

/// Every card, sorted by path. Hidden files and folders are skipped.
pub fn list_cards(project: &Path) -> Result<Vec<CardSummary>> {
    let _ = project;
    bail!(NOT_YET)
}

pub fn read_card(project: &Path, path: &str) -> Result<Card> {
    let _ = (project, path);
    bail!(NOT_YET)
}

/// Writes (creating folders as needed) atomically. Refuses paths outside
/// the context folder and non-`.md` paths.
pub fn write_card(project: &Path, path: &str, meta: &CardMeta, body: &str) -> Result<Card> {
    let _ = (project, path, meta, body);
    bail!(NOT_YET)
}

/// Changes only the `status` line of the header.
pub fn set_status(project: &Path, path: &str, status: &str) -> Result<Card> {
    let _ = (project, path, status);
    bail!(NOT_YET)
}

/// A board over cards of the given types (all types when empty).
pub fn board(project: &Path, types: &[CardType]) -> Result<Board> {
    let _ = (project, types);
    bail!(NOT_YET)
}

pub fn graph(project: &Path) -> Result<LinkGraph> {
    let _ = project;
    bail!(NOT_YET)
}
