//! A structural reader for a Godot project, for the AI's `project_map`,
//! `find_symbol` and `describe_scene` tools: what the game's scripts define,
//! where each name is used, and how its scenes are put together. It reads
//! the project's own files each time (nothing is cached or stored), so it is
//! never out of date, and it never runs anything from the project.
//!
//! This is a reader, not a GDScript parser: definitions are recognised by the
//! keyword that starts a line (`func`, `signal`, `class_name`, ...) and uses
//! are whole-word matches. That is exact for how Godot code is written and
//! cheap, and it says so rather than guess when a name is spelled out of the
//! ordinary (`call("name")`, strings) — those show up as plain "mentions".

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use serde::Serialize;

const MAX_FILES: usize = 5000;
const MAX_FILE_BYTES: u64 = 1024 * 1024;
const MAX_HITS: usize = 120;
const SKIP_DIRS: &[&str] = &[".git", ".godot", ".import", ".ibproject", "node_modules", "target", "build"];

/// Files the index reads, by extension.
fn is_scanned(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("gd" | "tscn" | "tres" | "godot" | "cs")
    )
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        if out.len() >= MAX_FILES {
            return;
        }
        let path = entry.path();
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_symlink() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if kind.is_dir() {
            if SKIP_DIRS.contains(&name.as_str()) || (dir == root && name == "addons") {
                continue;
            }
            walk(root, &path, out);
        } else if is_scanned(&path) && entry.metadata().map(|m| m.len() <= MAX_FILE_BYTES).unwrap_or(false) {
            out.push(path);
        }
    }
}

struct Source {
    /// `/`-separated, relative to the project.
    path: String,
    text: String,
}

fn load(project: &Path) -> Result<Vec<Source>> {
    if !project.is_dir() {
        bail!("The project folder {} doesn't exist.", project.display());
    }
    let mut files = Vec::new();
    walk(project, project, &mut files);
    Ok(files
        .into_iter()
        .filter_map(|p| {
            let text = fs::read_to_string(&p).ok()?;
            let rel = p.strip_prefix(project).ok()?.to_string_lossy().replace('\\', "/");
            Some(Source { path: rel, text })
        })
        .collect())
}

fn is_ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// Where `word` occurs as a whole identifier in `line` (byte offsets).
fn word_positions(line: &str, word: &str) -> Vec<usize> {
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = line[from..].find(word) {
        let at = from + i;
        let end = at + word.len();
        let before_ok = at == 0 || !is_ident(bytes[at - 1]);
        let after_ok = end >= bytes.len() || !is_ident(bytes[end]);
        if before_ok && after_ok {
            out.push(at);
        }
        from = end;
    }
    out
}

// ---- GDScript ----

#[derive(Serialize, Clone, Debug, PartialEq, Default)]
pub struct ScriptInfo {
    pub path: String,
    pub class_name: Option<String>,
    pub extends: Option<String>,
    pub signals: Vec<String>,
    pub functions: Vec<String>,
}

#[derive(Debug, PartialEq)]
struct Def {
    kind: &'static str,
    name: String,
}

/// What a trimmed GDScript line defines, if anything.
fn definition(line: &str) -> Option<Def> {
    let mut rest = line.trim_start();
    if rest.starts_with('#') {
        return None;
    }
    // Annotations like `@export var x` or `@onready var y`.
    while let Some(after) = rest.strip_prefix('@') {
        let end = after.find(|c: char| c.is_whitespace()).unwrap_or(after.len());
        // `@export_range(0, 10) var x`: skip a parenthesised argument list.
        let mut tail = &after[end..];
        if after[..end].contains('(') {
            // The space was inside the arguments; find the closing paren.
            let close = after.find(')')?;
            tail = &after[close + 1..];
        }
        rest = tail.trim_start();
    }
    for (keyword, kind) in [
        ("static func ", "func"),
        ("func ", "func"),
        ("signal ", "signal"),
        ("class_name ", "class_name"),
        ("class ", "class"),
        ("enum ", "enum"),
        ("const ", "const"),
        ("static var ", "var"),
        ("var ", "var"),
    ] {
        if let Some(after) = rest.strip_prefix(keyword) {
            let name: String = after.trim_start().chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
            if !name.is_empty() {
                return Some(Def { kind, name });
            }
        }
    }
    None
}

fn script_info(src: &Source) -> ScriptInfo {
    let mut info = ScriptInfo { path: src.path.clone(), ..Default::default() };
    for line in src.text.lines() {
        let top_level = !line.starts_with([' ', '\t']);
        let trimmed = line.trim();
        if top_level {
            if let Some(rest) = trimmed.strip_prefix("extends ") {
                info.extends.get_or_insert_with(|| rest.split_whitespace().next().unwrap_or("").to_string());
            }
        }
        if let Some(def) = definition(line) {
            match def.kind {
                "class_name" if info.class_name.is_none() => info.class_name = Some(def.name),
                "signal" if top_level => info.signals.push(def.name),
                "func" if top_level => info.functions.push(def.name),
                _ => {}
            }
        }
        // `class_name Foo extends Bar` on one line.
        if info.extends.is_none() && trimmed.starts_with("class_name ") {
            if let Some(i) = trimmed.find(" extends ") {
                info.extends = trimmed[i + 9..].split_whitespace().next().map(str::to_string);
            }
        }
    }
    info
}

// ---- Scenes ----

/// The `key=value` pairs of a `[heading key=value ...]` line. Values are
/// quoted strings, or a bare token such as `ExtResource("2")`.
fn attrs(line: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let b = line.as_bytes();
    let mut i = 0;
    while i < b.len() {
        // A key is an identifier followed by `=`.
        if is_ident(b[i]) && (i == 0 || !is_ident(b[i - 1])) {
            let start = i;
            while i < b.len() && is_ident(b[i]) {
                i += 1;
            }
            if i < b.len() && b[i] == b'=' {
                let key = line[start..i].to_string();
                i += 1;
                let value_start = i;
                if i < b.len() && b[i] == b'"' {
                    i += 1;
                    let s = i;
                    while i < b.len() && b[i] != b'"' {
                        if b[i] == b'\\' {
                            i += 1;
                        }
                        i += 1;
                    }
                    out.insert(key, line[s..i.min(line.len())].to_string());
                    i += 1;
                } else {
                    let mut depth = 0;
                    let mut quoted = false;
                    while i < b.len() {
                        match b[i] {
                            b'"' => quoted = !quoted,
                            b'(' | b'[' if !quoted => depth += 1,
                            // The `]` that closes the heading ends the value.
                            b']' if !quoted && depth == 0 => break,
                            b')' | b']' if !quoted => depth -= 1,
                            b' ' if !quoted && depth == 0 => break,
                            _ => {}
                        }
                        i += 1;
                    }
                    out.insert(key, line[value_start..i].to_string());
                }
                continue;
            }
            continue;
        }
        i += 1;
    }
    out
}

/// `ExtResource("2")` -> `2`.
fn ext_id(value: &str) -> Option<&str> {
    let inner = value.strip_prefix("ExtResource(")?.strip_suffix(')')?;
    Some(inner.trim().trim_matches('"'))
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct SceneNode {
    /// Path from the root, `.` for the root itself.
    pub path: String,
    pub name: String,
    /// The node's class, or `None` for an instanced scene.
    pub kind: Option<String>,
    pub script: Option<String>,
    /// The scene this node is an instance of.
    pub instance_of: Option<String>,
    pub groups: Vec<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct SceneConnection {
    pub signal: String,
    pub from: String,
    pub to: String,
    pub method: String,
}

#[derive(Serialize, Clone, Debug, PartialEq, Default)]
pub struct SceneInfo {
    pub path: String,
    pub nodes: Vec<SceneNode>,
    pub connections: Vec<SceneConnection>,
    /// Other scenes and scripts it loads.
    pub uses: Vec<String>,
}

fn res_to_rel(res: &str) -> String {
    res.strip_prefix("res://").unwrap_or(res).to_string()
}

fn scene_info(src: &Source) -> SceneInfo {
    let mut info = SceneInfo { path: src.path.clone(), ..Default::default() };
    let mut resources: BTreeMap<String, String> = BTreeMap::new();
    // Pass 1: external resources by id.
    for line in src.text.lines().filter(|l| l.starts_with("[ext_resource")) {
        let a = attrs(line);
        if let (Some(id), Some(path)) = (a.get("id"), a.get("path")) {
            let rel = res_to_rel(path);
            if !info.uses.contains(&rel) {
                info.uses.push(rel.clone());
            }
            resources.insert(id.clone(), rel);
        }
    }
    // Pass 2: nodes (with the property lines that follow each) and connections.
    let mut current: Option<usize> = None;
    for line in src.text.lines() {
        if line.starts_with("[node") {
            let a = attrs(line);
            let name = a.get("name").cloned().unwrap_or_default();
            let path = match a.get("parent").map(String::as_str) {
                None => ".".to_string(),
                Some(".") => name.clone(),
                Some(parent) => format!("{parent}/{name}"),
            };
            info.nodes.push(SceneNode {
                path,
                name,
                kind: a.get("type").cloned(),
                script: None,
                instance_of: a.get("instance").and_then(|v| ext_id(v)).and_then(|id| resources.get(id)).cloned(),
                groups: a
                    .get("groups")
                    .map(|g| {
                        g.trim_matches(['[', ']'])
                            .split(',')
                            .map(|s| s.trim().trim_matches(['&', '"']).to_string())
                            .filter(|s| !s.is_empty())
                            .collect()
                    })
                    .unwrap_or_default(),
            });
            current = Some(info.nodes.len() - 1);
        } else if line.starts_with("[connection") {
            let a = attrs(line);
            info.connections.push(SceneConnection {
                signal: a.get("signal").cloned().unwrap_or_default(),
                from: a.get("from").cloned().unwrap_or_default(),
                to: a.get("to").cloned().unwrap_or_default(),
                method: a.get("method").cloned().unwrap_or_default(),
            });
            current = None;
        } else if line.starts_with('[') {
            current = None;
        } else if let (Some(i), Some(rest)) = (current, line.strip_prefix("script = ")) {
            info.nodes[i].script = ext_id(rest.trim()).and_then(|id| resources.get(id)).cloned();
        }
    }
    info
}

// ---- project.godot ----

#[derive(Serialize, Clone, Debug, PartialEq, Default)]
pub struct ProjectSettings {
    pub name: Option<String>,
    pub main_scene: Option<String>,
    /// Autoload name -> script or scene path.
    pub autoloads: BTreeMap<String, String>,
    pub input_actions: Vec<String>,
}

fn project_settings(text: &str) -> ProjectSettings {
    let mut out = ProjectSettings::default();
    let mut section = String::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name.to_string();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        let (key, value) = (key.trim(), value.trim());
        match (section.as_str(), key) {
            ("application", "config/name") => out.name = Some(value.trim_matches('"').to_string()),
            ("application", "run/main_scene") => out.main_scene = Some(res_to_rel(value.trim_matches('"'))),
            ("autoload", _) => {
                // `"*res://scripts/events.gd"`: the star marks a global singleton.
                let path = value.trim_matches('"').trim_start_matches('*');
                out.autoloads.insert(key.to_string(), res_to_rel(path));
            }
            ("input", _) => out.input_actions.push(key.to_string()),
            _ => {}
        }
    }
    out
}

// ---- The tools ----

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct ProjectMap {
    pub settings: ProjectSettings,
    pub scripts: Vec<ScriptInfo>,
    /// Each scene with the script on its root node and how many nodes it has.
    pub scenes: Vec<SceneSummary>,
    pub other_resources: Vec<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct SceneSummary {
    pub path: String,
    pub root_script: Option<String>,
    pub nodes: usize,
}

/// The shape of the game: settings, autoloads, scripts and scenes.
pub fn project_map(project: &Path) -> Result<ProjectMap> {
    let sources = load(project)?;
    let mut map = ProjectMap {
        settings: ProjectSettings::default(),
        scripts: Vec::new(),
        scenes: Vec::new(),
        other_resources: Vec::new(),
    };
    for src in &sources {
        match src.path.rsplit('.').next() {
            Some("godot") if src.path == "project.godot" => map.settings = project_settings(&src.text),
            Some("gd") => map.scripts.push(script_info(src)),
            Some("tscn") => {
                let scene = scene_info(src);
                map.scenes.push(SceneSummary {
                    path: scene.path.clone(),
                    root_script: scene.nodes.first().and_then(|n| n.script.clone()),
                    nodes: scene.nodes.len(),
                });
            }
            Some("tres") => map.other_resources.push(src.path.clone()),
            _ => {}
        }
    }
    Ok(map)
}

/// One scene's nodes, scripts and signal connections.
pub fn describe_scene(project: &Path, scene: &str) -> Result<SceneInfo> {
    let rel = scene.trim().trim_start_matches("res://").trim_start_matches("./");
    if rel.is_empty() || rel.split('/').any(|p| p == ".." || p.is_empty()) || !rel.ends_with(".tscn") {
        bail!("Give a scene path inside the game, like scenes/main.tscn.");
    }
    let path = project.join(rel);
    let text = fs::read_to_string(&path).map_err(|_| anyhow::anyhow!("There's no scene at {rel}."))?;
    Ok(scene_info(&Source { path: rel.to_string(), text }))
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Hit {
    pub path: String,
    pub line: usize,
    /// definition kinds (`func`, `signal`, ...), or how it is used: `emit`,
    /// `connect`, `call`, `scene connection`, `scene script`, `mention`.
    pub kind: String,
    pub text: String,
}

#[derive(Serialize, Clone, Debug, PartialEq, Default)]
pub struct SymbolReport {
    pub name: String,
    pub definitions: Vec<Hit>,
    pub uses: Vec<Hit>,
    /// More matches existed than are listed.
    pub truncated: bool,
}

fn clip(line: &str) -> String {
    let t = line.trim();
    if t.chars().count() > 160 {
        format!("{}…", t.chars().take(160).collect::<String>())
    } else {
        t.to_string()
    }
}

/// How a use of `name` on `line` reads.
fn use_kind(line: &str, at: usize, name: &str) -> &'static str {
    let after = &line[at + name.len()..];
    let before = &line[..at];
    if after.starts_with(".emit(") {
        "emit"
    } else if after.starts_with(".connect(") || before.trim_end().ends_with("connect(") {
        "connect"
    } else if after.starts_with('(') {
        "call"
    } else if before.ends_with('"') || before.ends_with('\'') {
        "mention"
    } else {
        "use"
    }
}

/// Every definition and use of an identifier across the game's scripts and
/// scenes (whole-word, case-sensitive).
pub fn find_symbol(project: &Path, name: &str) -> Result<SymbolReport> {
    let name = name.trim();
    if name.is_empty() || !name.bytes().all(is_ident) {
        bail!("Give one name (letters, digits and underscores), like player_died or Events.");
    }
    let sources = load(project)?;
    let mut report = SymbolReport { name: name.to_string(), ..Default::default() };
    for src in &sources {
        let is_script = src.path.ends_with(".gd");
        let is_scene = src.path.ends_with(".tscn");
        for (i, line) in src.text.lines().enumerate() {
            let positions = word_positions(line, name);
            if positions.is_empty() {
                continue;
            }
            let hit = |kind: &str| Hit { path: src.path.clone(), line: i + 1, kind: kind.to_string(), text: clip(line) };
            if is_script {
                if let Some(def) = definition(line).filter(|d| d.name == name) {
                    report.definitions.push(hit(def.kind));
                    continue;
                }
            }
            let kind = if is_scene && line.starts_with("[connection") {
                "scene connection"
            } else if is_scene && line.starts_with("[node") {
                "scene node"
            } else {
                use_kind(line, positions[0], name)
            };
            if line.trim_start().starts_with('#') {
                continue;
            }
            report.uses.push(hit(kind));
        }
    }
    let total = report.definitions.len() + report.uses.len();
    if total > MAX_HITS {
        report.truncated = true;
        report.definitions.truncate(MAX_HITS);
        let room = MAX_HITS.saturating_sub(report.definitions.len());
        report.uses.truncate(room);
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, rel: &str, text: &str) {
        let p = dir.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }

    fn game() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        write(
            d,
            "project.godot",
            "config_version=5\n\n[application]\nconfig/name=\"Demo\"\nrun/main_scene=\"res://scenes/main.tscn\"\n\n[autoload]\nEvents=\"*res://scripts/events.gd\"\n\n[input]\nmove_left={\n}\njump={\n}\n",
        );
        write(
            d,
            "scripts/events.gd",
            "extends Node\n\nsignal player_died(reason)\nsignal score_changed(total: int)\n\nfunc announce(text: String) -> void:\n\tprint(text)\n",
        );
        write(
            d,
            "scripts/player.gd",
            "class_name Player\nextends CharacterBody2D\n\n@export var speed := 200.0\n@export_range(0, 10) var lives := 3\nconst GRAVITY = 900\n\nfunc _ready() -> void:\n\tEvents.score_changed.connect(_on_score)\n\nfunc die() -> void:\n\t# player_died is announced elsewhere too\n\tEvents.player_died.emit(\"fell\")\n\nfunc _on_score(total: int) -> void:\n\tpass\n",
        );
        write(
            d,
            "scenes/main.tscn",
            "[gd_scene load_steps=3 format=3]\n\n[ext_resource type=\"Script\" path=\"res://scripts/player.gd\" id=\"1_a\"]\n[ext_resource type=\"PackedScene\" path=\"res://scenes/coin.tscn\" id=\"2_b\"]\n\n[node name=\"Main\" type=\"Node2D\"]\n\n[node name=\"Player\" type=\"CharacterBody2D\" parent=\".\" groups=[\"player\"]]\nscript = ExtResource(\"1_a\")\n\n[node name=\"Coin\" parent=\".\" instance=ExtResource(\"2_b\")]\n\n[node name=\"Hitbox\" type=\"Area2D\" parent=\"Player\"]\n\n[connection signal=\"body_entered\" from=\"Player/Hitbox\" to=\"Player\" method=\"_on_score\"]\n",
        );
        write(d, "scenes/coin.tscn", "[gd_scene format=3]\n\n[node name=\"Coin\" type=\"Area2D\"]\n");
        // Never read: engine cache, the app's own folder, addons.
        write(d, ".godot/cache.gd", "func hidden():\n\tpass\n");
        write(d, "addons/infinabox/infinabox_runtime.gd", "func vendor():\n\tpass\n");
        dir
    }

    #[test]
    fn definitions_are_recognised_by_their_keyword() {
        let d = |l: &str| definition(l).map(|d| (d.kind, d.name));
        assert_eq!(d("func _ready() -> void:"), Some(("func", "_ready".into())));
        assert_eq!(d("\tstatic func make(x):"), Some(("func", "make".into())));
        assert_eq!(d("signal hit(damage)"), Some(("signal", "hit".into())));
        assert_eq!(d("@export var speed := 1.0"), Some(("var", "speed".into())));
        assert_eq!(d("@export_range(0, 10) var lives := 3"), Some(("var", "lives".into())));
        assert_eq!(d("@onready var sprite = $Sprite"), Some(("var", "sprite".into())));
        assert_eq!(d("const MAX = 4"), Some(("const", "MAX".into())));
        assert_eq!(d("class_name Player"), Some(("class_name", "Player".into())));
        assert_eq!(d("# func nope():"), None);
        assert_eq!(d("\tprint(1)"), None);
    }

    #[test]
    fn the_map_lists_settings_scripts_and_scenes_and_skips_engine_folders() {
        let dir = game();
        let map = project_map(dir.path()).unwrap();
        assert_eq!(map.settings.name.as_deref(), Some("Demo"));
        assert_eq!(map.settings.main_scene.as_deref(), Some("scenes/main.tscn"));
        assert_eq!(map.settings.autoloads.get("Events").map(String::as_str), Some("scripts/events.gd"));
        assert_eq!(map.settings.input_actions, vec!["move_left", "jump"]);

        let paths: Vec<_> = map.scripts.iter().map(|s| s.path.as_str()).collect();
        assert_eq!(paths, vec!["scripts/events.gd", "scripts/player.gd"]);
        let player = &map.scripts[1];
        assert_eq!(player.class_name.as_deref(), Some("Player"));
        assert_eq!(player.extends.as_deref(), Some("CharacterBody2D"));
        assert_eq!(player.functions, vec!["_ready", "die", "_on_score"]);
        assert_eq!(map.scripts[0].signals, vec!["player_died", "score_changed"]);

        let main = map.scenes.iter().find(|s| s.path == "scenes/main.tscn").unwrap();
        assert_eq!(main.nodes, 4);
        assert_eq!(main.root_script, None);
    }

    #[test]
    fn a_scene_is_described_with_scripts_instances_groups_and_connections() {
        let dir = game();
        let scene = describe_scene(dir.path(), "res://scenes/main.tscn").unwrap();
        let by = |p: &str| scene.nodes.iter().find(|n| n.path == p).unwrap();
        assert_eq!(by(".").kind.as_deref(), Some("Node2D"));
        assert_eq!(by("Player").script.as_deref(), Some("scripts/player.gd"));
        assert_eq!(by("Player").groups, vec!["player"]);
        assert_eq!(by("Coin").instance_of.as_deref(), Some("scenes/coin.tscn"));
        assert_eq!(by("Player/Hitbox").kind.as_deref(), Some("Area2D"));
        assert_eq!(scene.connections.len(), 1);
        assert_eq!(scene.connections[0].method, "_on_score");
        assert!(scene.uses.contains(&"scripts/player.gd".to_string()));

        assert!(describe_scene(dir.path(), "../x.tscn").is_err());
        assert!(describe_scene(dir.path(), "scenes/nope.tscn").is_err());
        assert!(describe_scene(dir.path(), "scripts/player.gd").is_err());
    }

    #[test]
    fn a_signal_is_found_where_it_is_defined_emitted_and_connected() {
        let dir = game();
        let r = find_symbol(dir.path(), "player_died").unwrap();
        assert_eq!(r.definitions.len(), 1);
        assert_eq!((r.definitions[0].path.as_str(), r.definitions[0].kind.as_str()), ("scripts/events.gd", "signal"));
        // The comment mentioning it is not a use; the emit is.
        assert_eq!(r.uses.len(), 1, "{:?}", r.uses);
        assert_eq!((r.uses[0].path.as_str(), r.uses[0].line, r.uses[0].kind.as_str()), ("scripts/player.gd", 13, "emit"));

        let r = find_symbol(dir.path(), "_on_score").unwrap();
        assert_eq!(r.definitions.len(), 1);
        let kinds: Vec<_> = r.uses.iter().map(|u| u.kind.as_str()).collect();
        assert!(kinds.contains(&"scene connection"), "{kinds:?}");
        // `score_changed.connect(_on_score)` is a connect.
        assert!(kinds.contains(&"connect"), "{kinds:?}");
    }

    #[test]
    fn whole_words_only_and_one_plain_name() {
        let dir = game();
        // `Player` the class is not `Player` inside `PlayerHud`.
        write(dir.path(), "scripts/hud.gd", "extends Control\nvar PlayerHud = 1\n");
        let r = find_symbol(dir.path(), "Player").unwrap();
        assert!(r.uses.iter().all(|u| u.path != "scripts/hud.gd"), "{:?}", r.uses);
        assert!(find_symbol(dir.path(), "").is_err());
        assert!(find_symbol(dir.path(), "a b").is_err());
        assert!(find_symbol(dir.path(), "a.b").is_err());
        // Vendored and cache folders aren't searched.
        assert!(find_symbol(dir.path(), "hidden").unwrap().definitions.is_empty());
        assert!(find_symbol(dir.path(), "vendor").unwrap().definitions.is_empty());
    }

    #[test]
    fn attrs_handles_quotes_calls_and_arrays() {
        let a = attrs("[node name=\"Hit Box\" type=\"Area2D\" parent=\".\" instance=ExtResource(\"3_x\") groups=[\"a\", \"b\"]]");
        assert_eq!(a["name"], "Hit Box");
        assert_eq!(a["instance"], "ExtResource(\"3_x\")");
        assert_eq!(ext_id(&a["instance"]), Some("3_x"));
        assert_eq!(a["groups"], "[\"a\", \"b\"]");
    }
}
