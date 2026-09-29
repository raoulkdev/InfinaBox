//! The Producer's journey (spec §6.3): Idea → Prototype → Vertical Slice →
//! Alpha → Beta → Launch, each with a checklist. An item is checked only by
//! a real signal in the project (cards, files, snapshots, license records) or
//! by the person; `evidence` says what was actually found.
//!
//! Manual ticks and the last successful game run live in
//! `.ibproject/journey.json`.
//!
//! Phase C contract (frozen types and signatures).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use git2::Repository;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Idea,
    Prototype,
    VerticalSlice,
    Alpha,
    Beta,
    Launch,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Signal {
    /// Checked by the app from something real in the project.
    Auto,
    /// Only the person can say.
    Manual,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Criterion {
    /// Stable id (`prototype.core_loop_cards`).
    pub id: String,
    pub title: String,
    pub signal: Signal,
    pub done: bool,
    /// What was found, in plain words ("3 mechanic cards"), or why it isn't done.
    pub evidence: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct StageStatus {
    pub stage: Stage,
    pub title: String,
    pub criteria: Vec<Criterion>,
    pub complete: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct NextStep {
    pub criterion_id: String,
    pub title: String,
    /// A ready-to-send message for the Producer role ("Help me finish: …").
    pub ask: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Journey {
    /// The first stage that isn't complete (Launch when everything is).
    pub current: Stage,
    pub stages: Vec<StageStatus>,
    pub next_step: Option<NextStep>,
}

// --- The journey, as data ---------------------------------------------------

/// One criterion as written down: its stable id, what the person sees, and
/// whether the app can check it from the project.
struct Def {
    id: &'static str,
    title: &'static str,
    signal: Signal,
}

const fn auto(id: &'static str, title: &'static str) -> Def {
    Def {
        id,
        title,
        signal: Signal::Auto,
    }
}

const fn manual(id: &'static str, title: &'static str) -> Def {
    Def {
        id,
        title,
        signal: Signal::Manual,
    }
}

struct StageDef {
    stage: Stage,
    title: &'static str,
    criteria: &'static [Def],
}

const STAGES: [StageDef; 6] = [
    StageDef {
        stage: Stage::Idea,
        title: "Idea",
        criteria: &[
            auto("idea.concept_written", "Write down what your game is about"),
            auto(
                "idea.style_guide",
                "Describe how your game should look and feel",
            ),
            auto("idea.first_milestone", "Plan your first milestone"),
        ],
    },
    StageDef {
        stage: Stage::Prototype,
        title: "Prototype",
        criteria: &[
            auto("prototype.playable", "Get the game to start"),
            auto("prototype.runs_clean", "Run the game with no errors"),
            auto(
                "prototype.core_mechanic_card",
                "Get your core mechanic working",
            ),
            auto("prototype.snapshots", "Save a few versions of your game"),
            manual(
                "prototype.playtested_by_you",
                "Play it yourself from start to finish",
            ),
        ],
    },
    StageDef {
        stage: Stage::VerticalSlice,
        title: "Vertical slice",
        criteria: &[
            auto("slice.level_card", "Describe one polished level"),
            auto("slice.character_card", "Describe your main character"),
            auto(
                "slice.assets_licensed",
                "Know where every art and sound file came from",
            ),
            manual(
                "slice.start_to_end",
                "You can play from the start screen to an ending",
            ),
        ],
    },
    StageDef {
        stage: Stage::Alpha,
        title: "Alpha",
        criteria: &[
            auto("alpha.all_mechanics_working", "Get all your mechanics working"),
            auto("alpha.no_open_bugs", "Fix the bugs you know about"),
            manual("alpha.sound", "Add sound and music"),
            manual("alpha.friends_played", "Let a few friends play it"),
        ],
    },
    StageDef {
        stage: Stage::Beta,
        title: "Beta",
        criteria: &[
            auto(
                "beta.playtest_notes",
                "Write up what you learned from playtests",
            ),
            manual("beta.feedback_addressed", "Act on the feedback you got"),
            manual("beta.credits", "Check your credits are complete"),
            manual("beta.stable", "The game runs steadily, with no crashes"),
        ],
    },
    StageDef {
        stage: Stage::Launch,
        title: "Launch",
        criteria: &[
            manual("launch.store_page", "Set up your store page"),
            manual("launch.build_tested", "Test the finished build"),
            manual("launch.trailer_or_screens", "Make a trailer or screenshots"),
            manual(
                "launch.price_and_platforms",
                "Decide your price and platforms",
            ),
        ],
    },
];

/// How many saved versions, mechanic cards and playtest cards a game needs
/// before the matching criterion counts.
const SNAPSHOTS_WANTED: usize = 3;
const PLAYTESTS_WANTED: usize = 3;
const MECHANICS_WANTED: usize = 3;

// --- journey.json -------------------------------------------------------------

#[derive(Serialize, Deserialize, Default, Debug)]
struct JourneyFile {
    #[serde(default)]
    manual: BTreeMap<String, bool>,
    #[serde(default)]
    last_boot: Option<Boot>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
struct Boot {
    ok: bool,
    /// Unix seconds.
    at: u64,
}

fn journey_path(project: &Path) -> PathBuf {
    project.join(".ibproject/journey.json")
}

/// A missing file is an empty journey; a file that can't be read is an
/// error (never silently reset: the person's ticks are in there).
fn read_file(project: &Path) -> Result<JourneyFile> {
    let path = journey_path(project);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(JourneyFile::default()),
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    serde_json::from_str(&text).with_context(|| {
        format!(
            "The journey file ({}) is damaged and can't be read. Fix or delete it to start the checklist again.",
            path.display()
        )
    })
}

/// Temp file + rename, so a crash never leaves half a file.
fn write_file(project: &Path, file: &JourneyFile) -> Result<()> {
    let path = journey_path(project);
    let dir = path.parent().expect("journey.json has a parent");
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let tmp = dir.join("journey.json.tmp");
    let text = serde_json::to_string_pretty(file).context("writing the journey")?;
    fs::write(&tmp, text + "\n").with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, &path).with_context(|| {
        let _ = fs::remove_file(&tmp);
        format!("saving {}", path.display())
    })
}

// --- Reading the project ------------------------------------------------------

/// The few card fields the journey needs. Missing or odd front-matter just
/// leaves them empty.
#[derive(Default)]
struct CardInfo {
    kind: String,
    title: String,
    status: String,
    tags: Vec<String>,
    file: String,
    license: String,
    /// The text after the front-matter.
    body: String,
    path: String,
}

fn unquote(value: &str) -> String {
    let v = value.trim();
    if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
        if let Ok(s) = serde_json::from_str::<String>(v) {
            return s;
        }
    }
    if v.len() >= 2 && v.starts_with('\'') && v.ends_with('\'') {
        return v[1..v.len() - 1].to_string();
    }
    v.to_string()
}

fn parse_card(path: String, text: &str) -> CardInfo {
    let text = text.replace("\r\n", "\n");
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let mut card = CardInfo {
        path,
        ..Default::default()
    };
    let Some(rest) = text.strip_prefix("---\n") else {
        card.body = text.to_string();
        return card;
    };
    let (head, body) = match rest.split_once("\n---") {
        Some((head, after)) => (head, after.split_once('\n').map_or("", |(_, b)| b)),
        None => {
            // Never closed: not front-matter.
            card.body = text.to_string();
            return card;
        }
    };
    card.body = body.to_string();
    let mut list_key: Option<String> = None;
    for line in head.lines() {
        if let Some(item) = line.trim_start().strip_prefix("- ") {
            if list_key.as_deref() == Some("tags") {
                card.tags.push(unquote(item).to_lowercase());
            }
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_lowercase();
        let value = value.trim();
        list_key = value.is_empty().then(|| key.clone());
        match key.as_str() {
            "type" => card.kind = unquote(value).to_lowercase(),
            "title" => card.title = unquote(value),
            "status" => card.status = unquote(value).to_lowercase(),
            "file" => card.file = unquote(value),
            "license" => card.license = unquote(value),
            "tags" => {
                if let Some(inner) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
                    card.tags.extend(
                        inner
                            .split(',')
                            .map(|t| unquote(t).to_lowercase())
                            .filter(|t| !t.is_empty()),
                    );
                }
            }
            _ => {}
        }
    }
    if card.title.is_empty() {
        card.title = card
            .body
            .lines()
            .find_map(|l| l.strip_prefix("# "))
            .unwrap_or_default()
            .trim()
            .to_string();
    }
    card
}

fn read_cards(project: &Path) -> Vec<CardInfo> {
    let root = project.join(".ibproject/context");
    let mut cards = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "md") {
                if let Ok(text) = fs::read_to_string(&path) {
                    let rel = path.strip_prefix(&root).unwrap_or(&path);
                    let rel = rel.to_string_lossy().replace('\\', "/");
                    cards.push(parse_card(rel, &text));
                }
            }
        }
    }
    cards.sort_by(|a, b| a.path.cmp(&b.path));
    cards
}

/// The files the game keeps under `assets/` (not Godot's own `.import` and
/// `.uid` bookkeeping, and not hidden files), project-relative.
fn asset_files(project: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![project.join("assets")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if name.starts_with('.') {
                continue;
            }
            if kind.is_dir() {
                stack.push(path);
            } else if !name.ends_with(".import") && !name.ends_with(".uid") {
                if let Ok(rel) = path.strip_prefix(project) {
                    out.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
    }
    out.sort();
    out
}

fn normalize_asset_path(path: &str) -> String {
    let p = path.trim();
    let p = p.strip_prefix("res://").unwrap_or(p);
    p.strip_prefix("./").unwrap_or(p).to_string()
}

/// How many commits carry the `InfinaBox-Snapshot` trailer; 0 without a
/// repository or any commit.
fn count_snapshots(project: &Path) -> usize {
    let Ok(repo) = Repository::open(project) else {
        return 0;
    };
    let Ok(mut walk) = repo.revwalk() else {
        return 0;
    };
    if walk.push_head().is_err() {
        return 0;
    }
    walk.flatten()
        .filter_map(|id| repo.find_commit(id).ok())
        .filter(|c| {
            c.message().is_ok_and(|m| {
                m.lines()
                    .any(|l| l.trim_start().starts_with("InfinaBox-Snapshot:"))
            })
        })
        .count()
}

/// The `run/main_scene` of `project.godot` (project-relative), and whether
/// that file exists.
fn main_scene(project: &Path) -> (Option<String>, bool) {
    let Ok(text) = fs::read_to_string(project.join("project.godot")) else {
        return (None, false);
    };
    let scene = text.lines().find_map(|l| {
        let (key, value) = l.split_once('=')?;
        (key.trim() == "run/main_scene").then(|| unquote(value))
    });
    match scene {
        Some(s) if !s.is_empty() => {
            let rel = normalize_asset_path(&s);
            let exists = project.join(&rel).is_file();
            (Some(rel), exists)
        }
        _ => (None, false),
    }
}

/// Whether a concept card has really been written: a "Pitch" section with
/// text of its own, and the template's "Starting point" note gone. (The
/// interview's own card has "Starting point" as a section heading, which is
/// fine; the template's is a `> **Starting point.**` note.)
fn concept_is_written(body: &str) -> bool {
    if body
        .lines()
        .any(|l| l.trim_start().starts_with('>') && l.contains("Starting point"))
    {
        return false;
    }
    let mut in_pitch = false;
    let mut text = String::new();
    for line in body.lines() {
        if let Some(heading) = line.trim_start().strip_prefix('#') {
            let heading = heading.trim_start_matches('#').trim();
            if in_pitch {
                break;
            }
            in_pitch = heading.eq_ignore_ascii_case("pitch");
        } else if in_pitch {
            text.push_str(line);
            text.push('\n');
        }
    }
    let text = text.trim();
    !text.is_empty() && text != "What is the game, in one or two sentences?"
}

/// Real description text: something other than headings and quoted notes.
fn has_own_text(body: &str) -> bool {
    body.lines().map(str::trim).any(|l| {
        !l.is_empty() && !l.starts_with('#') && !l.starts_with('>') && !l.starts_with("---")
    })
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn count_cards(n: usize, one: &str, many: &str) -> (bool, String) {
    if n > 0 {
        (true, plural(n, one, many))
    } else {
        (false, format!("No {many} yet."))
    }
}

/// What was found in the project.
struct Facts {
    cards: Vec<CardInfo>,
    snapshots: usize,
    main_scene: (Option<String>, bool),
    assets: Vec<String>,
    boot: Option<Boot>,
}

impl Facts {
    fn of_type<'a>(&'a self, kind: &'a str) -> impl Iterator<Item = &'a CardInfo> {
        self.cards.iter().filter(move |c| c.kind == kind)
    }

    /// Whether some Asset card names this file and gives a license.
    fn is_licensed(&self, asset: &str) -> bool {
        self.of_type("asset")
            .any(|c| !c.license.trim().is_empty() && normalize_asset_path(&c.file) == asset)
    }

    /// Done or not, and the plain-words evidence, for an automatic criterion.
    fn check(&self, id: &str) -> (bool, String) {
        match id {
            "idea.concept_written" => match self.cards.iter().find(|c| c.kind == "concept") {
                None => (false, "There's no concept card yet.".into()),
                Some(c) if concept_is_written(&c.body) => {
                    (true, "The concept card has a pitch.".into())
                }
                Some(_) => (
                    false,
                    "The concept card is still the starting template.".into(),
                ),
            },
            "idea.style_guide" => match self.of_type("style-guide").next() {
                None => (false, "There's no style guide card yet.".into()),
                Some(c) if has_own_text(&c.body) => {
                    (true, "The style guide card has a description.".into())
                }
                Some(_) => (false, "The style guide card is empty.".into()),
            },
            "idea.first_milestone" => {
                count_cards(self.of_type("task").count(), "task card", "task cards")
            }
            "prototype.playable" => match &self.main_scene {
                (Some(scene), true) => (true, format!("The game starts from {scene}.")),
                (Some(scene), false) => (
                    false,
                    format!("The game's start scene, {scene}, is missing."),
                ),
                (None, _) => (false, "The game has no start scene set.".into()),
            },
            "prototype.runs_clean" => match self.boot {
                Some(Boot { ok: true, .. }) => (true, "The game's last run had no errors.".into()),
                Some(Boot { ok: false, .. }) => (false, "The game's last run had errors.".into()),
                None => (false, "The game hasn't been run yet.".into()),
            },
            "prototype.core_mechanic_card" => {
                let n = self
                    .of_type("mechanic")
                    .filter(|c| c.status == "working")
                    .count();
                if n > 0 {
                    (
                        true,
                        format!("{} marked working", plural(n, "mechanic card", "mechanic cards")),
                    )
                } else {
                    (false, "No mechanic card is marked working yet.".into())
                }
            }
            "prototype.snapshots" => {
                let n = self.snapshots;
                let found = plural(n, "saved version", "saved versions");
                if n >= SNAPSHOTS_WANTED {
                    (true, found)
                } else {
                    (false, format!("{found} so far (aim for {SNAPSHOTS_WANTED})"))
                }
            }
            "slice.level_card" => {
                count_cards(self.of_type("level").count(), "level card", "level cards")
            }
            "slice.character_card" => count_cards(
                self.of_type("character").count(),
                "character card",
                "character cards",
            ),
            "slice.assets_licensed" => {
                let total = self.assets.len();
                if total == 0 {
                    return (false, "No assets yet.".into());
                }
                let licensed = self.assets.iter().filter(|a| self.is_licensed(a)).count();
                (
                    licensed == total,
                    format!("{licensed} of {total} assets have a license"),
                )
            }
            "alpha.all_mechanics_working" => {
                let mechanics: Vec<_> = self.of_type("mechanic").collect();
                let n = mechanics.len();
                let bad = mechanics
                    .iter()
                    .filter(|c| c.status == "broken" || c.status == "planned")
                    .count();
                if n < MECHANICS_WANTED {
                    (
                        false,
                        format!(
                            "Only {} so far (aim for {MECHANICS_WANTED})",
                            plural(n, "mechanic card", "mechanic cards")
                        ),
                    )
                } else if bad > 0 {
                    (
                        false,
                        format!("{bad} of {n} mechanic cards are broken or still planned"),
                    )
                } else {
                    (true, format!("{n} mechanic cards, none broken or planned"))
                }
            }
            "alpha.no_open_bugs" => {
                let open = self
                    .of_type("task")
                    .filter(|c| {
                        (c.title.to_lowercase().starts_with("bug")
                            || c.tags.iter().any(|t| t == "bug"))
                            && c.status != "done"
                    })
                    .count();
                if open == 0 {
                    (true, "No open bugs recorded.".into())
                } else {
                    (false, plural(open, "open bug", "open bugs"))
                }
            }
            "beta.playtest_notes" => {
                let n = self.of_type("playtest").count();
                let found = plural(n, "playtest card", "playtest cards");
                if n >= PLAYTESTS_WANTED {
                    (true, found)
                } else {
                    (false, format!("{found} so far (aim for {PLAYTESTS_WANTED})"))
                }
            }
            _ => (false, "Not checked by the app.".into()),
        }
    }
}

pub fn compute(project: &Path) -> Result<Journey> {
    let file = read_file(project)?;
    let facts = Facts {
        cards: read_cards(project),
        snapshots: count_snapshots(project),
        main_scene: main_scene(project),
        assets: asset_files(project),
        boot: file.last_boot,
    };

    let stages: Vec<StageStatus> = STAGES
        .iter()
        .map(|def| {
            let criteria: Vec<Criterion> = def
                .criteria
                .iter()
                .map(|c| {
                    let (done, evidence) = match c.signal {
                        Signal::Auto => facts.check(c.id),
                        Signal::Manual => {
                            if file.manual.get(c.id).copied().unwrap_or(false) {
                                (true, "You ticked this off.".to_string())
                            } else {
                                (false, "Only you can say. Tick it when it's true.".to_string())
                            }
                        }
                    };
                    Criterion {
                        id: c.id.into(),
                        title: c.title.into(),
                        signal: c.signal,
                        done,
                        evidence,
                    }
                })
                .collect();
            StageStatus {
                stage: def.stage,
                title: def.title.into(),
                complete: criteria.iter().all(|c| c.done),
                criteria,
            }
        })
        .collect();

    let current_stage = stages.iter().find(|s| !s.complete);
    let current = current_stage.map_or(Stage::Launch, |s| s.stage);
    // Prefer something the app can check (and so help with) over a tick.
    let next_step = current_stage.and_then(|s| {
        let open = || s.criteria.iter().filter(|c| !c.done);
        open()
            .find(|c| c.signal == Signal::Auto)
            .or_else(|| open().next())
            .map(|c| NextStep {
                criterion_id: c.id.clone(),
                title: c.title.clone(),
                ask: format!(
                    "Help me with the next step on my game's journey: {}. What should I do first? \
                     Here's what my project shows: {}",
                    c.title, c.evidence
                ),
            })
    });

    Ok(Journey {
        current,
        stages,
        next_step,
    })
}

/// Ticks or unticks a manual criterion. Refuses an unknown id or an
/// automatic criterion.
pub fn set_manual(project: &Path, id: &str, done: bool) -> Result<()> {
    let def = STAGES.iter().flat_map(|s| s.criteria).find(|c| c.id == id);
    match def {
        None => bail!("There's no step called “{id}” on the journey."),
        Some(c) if c.signal == Signal::Auto => bail!(
            "“{}” is checked by InfinaBox from your project, so it can't be ticked by hand.",
            c.title
        ),
        Some(_) => {}
    }
    let mut file = read_file(project)?;
    if done {
        file.manual.insert(id.to_string(), true);
    } else {
        file.manual.remove(id);
    }
    write_file(project, &file)
}

/// Records whether the game last ran without errors (called by the app
/// after a run); feeds the "runs without errors" criterion.
pub fn record_boot(project: &Path, ok: bool) -> Result<()> {
    let mut file = read_file(project)?;
    let at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    file.last_boot = Some(Boot { ok, at });
    write_file(project, &file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{onboarding, scaffold, snapshot};

    fn project(template: &str) -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let p = scaffold::create_project_from_template(tmp.path(), "Game", template).unwrap();
        (tmp, p)
    }

    fn criterion<'a>(j: &'a Journey, id: &str) -> &'a Criterion {
        j.stages
            .iter()
            .flat_map(|s| &s.criteria)
            .find(|c| c.id == id)
            .unwrap_or_else(|| panic!("no criterion {id}"))
    }

    fn write(project: &Path, rel: &str, text: &str) {
        let path = project.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn card(project: &Path, rel: &str, kind: &str, title: &str, status: &str) {
        write(
            project,
            &format!(".ibproject/context/{rel}"),
            &format!("---\ntype: {kind}\ntitle: {title}\nstatus: {status}\n---\n\n# {title}\n"),
        );
    }

    #[test]
    fn journey_has_six_stages_with_unique_ids() {
        let (_t, p) = project("blank-2d");
        let j = compute(&p).unwrap();
        assert_eq!(j.stages.len(), 6);
        assert!(
            j.stages
                .iter()
                .all(|s| (3..=6).contains(&s.criteria.len()))
        );
        let mut ids: Vec<_> = j
            .stages
            .iter()
            .flat_map(|s| &s.criteria)
            .map(|c| c.id.clone())
            .collect();
        let n = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), n);
    }

    #[test]
    fn fresh_platformer_project_is_honest() {
        let (_t, p) = project("platformer-2d");
        let j = compute(&p).unwrap();
        assert_eq!(j.current, Stage::Idea);
        // The template's concept is a placeholder, and there are no other cards.
        assert!(!criterion(&j, "idea.concept_written").done);
        assert!(!criterion(&j, "idea.style_guide").done);
        assert!(!criterion(&j, "idea.first_milestone").done);
        let playable = criterion(&j, "prototype.playable");
        assert!(playable.done);
        assert!(playable.evidence.contains("scenes/main.tscn"));
        let clean = criterion(&j, "prototype.runs_clean");
        assert!(!clean.done);
        assert_eq!(clean.evidence, "The game hasn't been run yet.");
        // The template's mechanic cards are marked working.
        let core = criterion(&j, "prototype.core_mechanic_card");
        assert!(core.done, "{}", core.evidence);
        assert!(core.evidence.starts_with("5 mechanic cards"));
        // One snapshot, "New project".
        let snaps = criterion(&j, "prototype.snapshots");
        assert!(!snaps.done);
        assert!(
            snaps.evidence.starts_with("1 saved version so far"),
            "{}",
            snaps.evidence
        );
        assert_eq!(criterion(&j, "slice.assets_licensed").evidence, "No assets yet.");
        assert!(!j.stages[0].complete);
        let next = j.next_step.unwrap();
        assert_eq!(next.criterion_id, "idea.concept_written");
        assert!(
            next.ask
                .starts_with("Help me with the next step on my game's journey:")
        );
        assert!(next.ask.contains(&next.title));
        assert!(next.ask.contains("still the starting template"));
    }

    #[test]
    fn concept_placeholder_versus_interview_concept() {
        let (_t, blank) = project("blank-2d");
        let j = compute(&blank).unwrap();
        assert!(!criterion(&j, "idea.concept_written").done);

        let tmp = tempfile::tempdir().unwrap();
        let answers = onboarding::InterviewAnswers {
            idea: "A frog that collects moonlight".into(),
            genre: "blank-2d".into(),
            name: "Frog".into(),
            feel: vec!["cozy".into()],
            look: "soft pixel art".into(),
            ..Default::default()
        };
        let made =
            onboarding::create_from_interview(tmp.path(), &answers, "blank-2d", "codex").unwrap();
        let j = compute(Path::new(&made.path)).unwrap();
        assert!(criterion(&j, "idea.concept_written").done);
        assert!(criterion(&j, "idea.style_guide").done);
        assert!(criterion(&j, "idea.first_milestone").done);
        assert_eq!(j.current, Stage::Prototype);
        assert_eq!(j.next_step.unwrap().criterion_id, "prototype.runs_clean");

        // The same interview without an idea leaves no pitch to count.
        let tmp2 = tempfile::tempdir().unwrap();
        let bare = onboarding::InterviewAnswers {
            genre: "blank-2d".into(),
            name: "Bare".into(),
            ..Default::default()
        };
        let made =
            onboarding::create_from_interview(tmp2.path(), &bare, "blank-2d", "codex").unwrap();
        let j = compute(Path::new(&made.path)).unwrap();
        assert!(!criterion(&j, "idea.concept_written").done);
        assert!(!criterion(&j, "idea.style_guide").done);
    }

    #[test]
    fn concept_detection_details() {
        assert!(!concept_is_written(
            "# G\n\n## Pitch\n\nWhat is the game, in one or two sentences?\n"
        ));
        assert!(!concept_is_written("# G\n\n## Pitch\n\n## Pillars\n\nx\n"));
        assert!(!concept_is_written(
            "# G\n\n> **Starting point.** blah\n\n## Pitch\n\nA real game.\n"
        ));
        assert!(concept_is_written(
            "# G\n\n## Pitch\n\nA real game.\n\n## Pillars\n\nWhat?\n"
        ));
        assert!(!concept_is_written("# G\n\nA real game.\n"));
    }

    #[test]
    fn cards_bugs_and_mechanics_are_read_from_the_project() {
        let (_t, p) = project("blank-2d");
        card(&p, "mechanics/a.md", "mechanic", "A", "working");
        card(&p, "mechanics/b.md", "mechanic", "B", "working");
        card(&p, "mechanics/c.md", "mechanic", "C", "Planned");
        card(&p, "levels/one.md", "level", "One", "draft");
        card(&p, "characters/hero.md", "character", "Hero", "draft");
        card(&p, "tasks/x.md", "task", "Bug: falls through floor", "todo");
        write(
            &p,
            ".ibproject/context/tasks/y.md",
            "---\ntype: task\ntitle: Odd one\nstatus: doing\ntags: [Bug, physics]\n---\n",
        );
        write(
            &p,
            ".ibproject/context/tasks/z.md",
            "---\ntype: task\ntitle: Bug: old\nstatus: done\n---\n",
        );
        write(&p, ".ibproject/context/notes.md", "no front matter at all\n");
        write(&p, ".ibproject/context/broken.md", "---\ntype: [oops\n");

        let j = compute(&p).unwrap();
        let all = criterion(&j, "alpha.all_mechanics_working");
        assert!(!all.done);
        assert_eq!(
            all.evidence,
            "1 of 3 mechanic cards are broken or still planned"
        );
        let bugs = criterion(&j, "alpha.no_open_bugs");
        assert!(!bugs.done);
        assert_eq!(bugs.evidence, "2 open bugs");
        assert!(criterion(&j, "slice.level_card").done);
        assert!(criterion(&j, "slice.character_card").done);
        assert!(criterion(&j, "prototype.core_mechanic_card").done);

        card(&p, "mechanics/c.md", "mechanic", "C", "working");
        card(&p, "tasks/x.md", "task", "Bug: falls through floor", "done");
        write(
            &p,
            ".ibproject/context/tasks/y.md",
            "---\ntype: task\ntitle: Odd one\nstatus: done\ntags:\n  - bug\n---\n",
        );
        let j = compute(&p).unwrap();
        assert!(criterion(&j, "alpha.all_mechanics_working").done);
        assert!(criterion(&j, "alpha.no_open_bugs").done);
    }

    #[test]
    fn playtest_cards_need_three() {
        let (_t, p) = project("blank-2d");
        card(&p, "playtests/1.md", "playtest", "One", "done");
        card(&p, "playtests/2.md", "playtest", "Two", "done");
        assert!(!criterion(&compute(&p).unwrap(), "beta.playtest_notes").done);
        card(&p, "playtests/3.md", "playtest", "Three", "done");
        let j = compute(&p).unwrap();
        assert!(criterion(&j, "beta.playtest_notes").done);
        assert_eq!(criterion(&j, "beta.playtest_notes").evidence, "3 playtest cards");
    }

    #[test]
    fn snapshots_are_counted_from_commit_trailers() {
        let (_t, p) = project("blank-2d");
        assert_eq!(count_snapshots(&p), 1);
        for i in 0..2 {
            write(&p, &format!("notes{i}.txt"), "x");
            snapshot::create_snapshot(&p, &format!("Change {i}"), None)
                .unwrap()
                .unwrap();
        }
        assert_eq!(count_snapshots(&p), 3);
        let j = compute(&p).unwrap();
        let s = criterion(&j, "prototype.snapshots");
        assert!(s.done);
        assert_eq!(s.evidence, "3 saved versions");

        // A plain commit without the trailer doesn't count.
        let repo = Repository::open(&p).unwrap();
        let sig = git2::Signature::now("t", "t@example.com").unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            "plain",
            &head.tree().unwrap(),
            &[&head],
        )
        .unwrap();
        assert_eq!(count_snapshots(&p), 3);

        // No repository at all.
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(count_snapshots(empty.path()), 0);
    }

    #[test]
    fn missing_main_scene_is_not_playable() {
        let (_t, p) = project("blank-2d");
        fs::remove_file(p.join("main.tscn")).unwrap();
        let j = compute(&p).unwrap();
        let c = criterion(&j, "prototype.playable");
        assert!(!c.done);
        assert!(c.evidence.contains("main.tscn"), "{}", c.evidence);
        fs::remove_file(p.join("project.godot")).unwrap();
        assert!(!criterion(&compute(&p).unwrap(), "prototype.playable").done);
    }

    #[test]
    fn assets_need_a_license_card_each() {
        let (_t, p) = project("blank-2d");
        write(&p, "assets/images/hero.png", "png");
        write(&p, "assets/images/hero.png.import", "import");
        write(&p, "assets/sounds/jump.ogg", "ogg");
        let j = compute(&p).unwrap();
        let c = criterion(&j, "slice.assets_licensed");
        assert!(!c.done);
        assert_eq!(c.evidence, "0 of 2 assets have a license");

        write(
            &p,
            ".ibproject/context/assets/hero.md",
            "---\ntype: asset\nfile: assets/images/hero.png\nlicense: CC0-1.0\n---\n",
        );
        write(
            &p,
            ".ibproject/context/assets/jump.md",
            "---\ntype: asset\nfile: \"res://assets/sounds/jump.ogg\"\nlicense:\n---\n",
        );
        let c = criterion(&compute(&p).unwrap(), "slice.assets_licensed").clone();
        assert!(!c.done);
        assert_eq!(c.evidence, "1 of 2 assets have a license");

        write(
            &p,
            ".ibproject/context/assets/jump.md",
            "---\ntype: asset\nfile: assets/sounds/jump.ogg\nlicense: \"CC-BY-4.0\"\n---\n",
        );
        let c = criterion(&compute(&p).unwrap(), "slice.assets_licensed").clone();
        assert!(c.done);
        assert_eq!(c.evidence, "2 of 2 assets have a license");
    }

    #[test]
    fn manual_ticks_round_trip() {
        let (_t, p) = project("blank-2d");
        set_manual(&p, "prototype.playtested_by_you", true).unwrap();
        let j = compute(&p).unwrap();
        let c = criterion(&j, "prototype.playtested_by_you");
        assert!(c.done);
        assert_eq!(c.signal, Signal::Manual);
        assert!(!criterion(&j, "alpha.sound").done);

        set_manual(&p, "prototype.playtested_by_you", false).unwrap();
        assert!(!criterion(&compute(&p).unwrap(), "prototype.playtested_by_you").done);

        // Refusals.
        let e = set_manual(&p, "idea.concept_written", true)
            .unwrap_err()
            .to_string();
        assert!(e.contains("can't be ticked"), "{e}");
        let e = set_manual(&p, "nope.nothing", true).unwrap_err().to_string();
        assert!(e.contains("nope.nothing"), "{e}");

        // Ticks for criteria that no longer exist are ignored; unknown fields too.
        write(
            &p,
            ".ibproject/journey.json",
            r#"{"manual":{"gone.away":true,"alpha.sound":true},"extra":1}"#,
        );
        let j = compute(&p).unwrap();
        assert!(criterion(&j, "alpha.sound").done);
        assert_eq!(j.stages.iter().flat_map(|s| &s.criteria).count(), 24);
        assert!(!p.join(".ibproject/journey.json.tmp").exists());
    }

    #[test]
    fn corrupt_journey_file_is_an_error_and_is_left_alone() {
        let (_t, p) = project("blank-2d");
        write(&p, ".ibproject/journey.json", "{not json");
        let e = compute(&p).unwrap_err().to_string();
        assert!(e.contains("damaged"), "{e}");
        assert!(set_manual(&p, "alpha.sound", true).is_err());
        assert!(record_boot(&p, true).is_err());
        assert_eq!(
            fs::read_to_string(p.join(".ibproject/journey.json")).unwrap(),
            "{not json"
        );
    }

    #[test]
    fn record_boot_feeds_runs_clean_and_keeps_ticks() {
        let (_t, p) = project("blank-2d");
        set_manual(&p, "alpha.sound", true).unwrap();
        record_boot(&p, false).unwrap();
        let c = criterion(&compute(&p).unwrap(), "prototype.runs_clean").clone();
        assert!(!c.done);
        assert_eq!(c.evidence, "The game's last run had errors.");
        record_boot(&p, true).unwrap();
        let j = compute(&p).unwrap();
        assert!(criterion(&j, "prototype.runs_clean").done);
        assert!(criterion(&j, "alpha.sound").done);

        let raw: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(p.join(".ibproject/journey.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(raw["last_boot"]["ok"], true);
        assert!(raw["last_boot"]["at"].as_u64().unwrap() > 1_700_000_000);
        assert_eq!(raw["manual"]["alpha.sound"], true);
    }

    #[test]
    fn everything_done_ends_on_launch_with_no_next_step() {
        let (_t, p) = project("blank-2d");
        write(
            &p,
            ".ibproject/context/concept.md",
            "---\ntype: concept\n---\n\n# G\n\n## Pitch\n\nA game.\n",
        );
        write(
            &p,
            ".ibproject/context/style-guide.md",
            "---\ntype: style-guide\n---\n\nBright colors.\n",
        );
        card(&p, "tasks/t.md", "task", "Plan", "done");
        for i in 0..3 {
            card(&p, &format!("mechanics/m{i}.md"), "mechanic", "M", "working");
            card(&p, &format!("playtests/p{i}.md"), "playtest", "P", "done");
        }
        card(&p, "levels/l.md", "level", "L", "done");
        card(&p, "characters/c.md", "character", "C", "done");
        write(&p, "assets/a.png", "png");
        write(
            &p,
            ".ibproject/context/assets/a.md",
            "---\ntype: asset\nfile: assets/a.png\nlicense: CC0-1.0\n---\n",
        );
        for i in 0..2 {
            write(&p, &format!("n{i}.txt"), "x");
            snapshot::create_snapshot(&p, "s", None).unwrap();
        }
        record_boot(&p, true).unwrap();
        let manual_ids: Vec<&str> = STAGES
            .iter()
            .flat_map(|s| s.criteria)
            .filter(|c| c.signal == Signal::Manual)
            .map(|c| c.id)
            .collect();
        for id in manual_ids.iter().filter(|id| !id.starts_with("launch.")) {
            set_manual(&p, id, true).unwrap();
        }
        let j = compute(&p).unwrap();
        assert_eq!(j.current, Stage::Launch);
        assert_eq!(
            j.next_step.as_ref().unwrap().criterion_id,
            "launch.store_page"
        );
        for id in &manual_ids {
            set_manual(&p, id, true).unwrap();
        }
        let j = compute(&p).unwrap();
        assert!(j.stages.iter().all(|s| s.complete));
        assert_eq!(j.current, Stage::Launch);
        assert!(j.next_step.is_none());
    }
}
