//! Creates new InfinaBox game projects from the bundled template
//! (`templates/blank-2d/`) and installs/updates the InfinaBox Godot addon
//! (`godot-addon/infinabox/`) — spec §9 (project format) and §10.3 (the
//! addon).
//!
//! Both directories are embedded into the binary with `include_dir`, so a
//! shipped app never depends on files next to its executable. (Edits to an
//! embedded file trigger a rebuild; a newly *added* file only gets picked up
//! once this crate is rebuilt for another reason — `touch` this file.)

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use include_dir::{Dir, include_dir};
use serde::{Deserialize, Serialize};

use crate::snapshot;

static TEMPLATE_BLANK_2D: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../templates/blank-2d");
static ADDON: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../godot-addon/infinabox");

/// The addon's version, printed by the running game as
/// `[infinabox] ready <version>`. Must match `VERSION` in
/// `godot-addon/infinabox/infinabox_runtime.gd` and `version` in its
/// `plugin.cfg` (a test checks both).
pub const ADDON_VERSION: u32 = 1;

/// Where the addon lives inside a project.
const ADDON_DIR: &str = "addons/infinabox";
/// The autoload singleton that runs the addon inside the game. Editor
/// plugins don't run in the game, so the autoload is what makes the addon
/// work at runtime; the `*` prefix makes Godot instance it as a singleton.
const AUTOLOAD_NAME: &str = "InfinaBox";
const AUTOLOAD_VALUE: &str = "\"*res://addons/infinabox/infinabox_runtime.gd\"";
const PLUGIN_CFG_RES_PATH: &str = "res://addons/infinabox/plugin.cfg";

/// Replaced with the project's name in the template's Markdown files.
const NAME_PLACEHOLDER: &str = "{{PROJECT_NAME}}";

/// The project marker, `.ibproject/.ibx`. `src/lib/project-picker.ts` only
/// checks that the file exists, so the shape is free to grow; version 2 is
/// the AI-studio project format (spec §9).
#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMarker {
    pub name: String,
    /// ISO 8601, UTC, matching the old frontend's `toISOString()`.
    pub created_at: String,
    pub format_version: u32,
    /// "2d" or "3d". Phase A only creates "2d".
    pub dimension: String,
}

/// Creates `<parent_dir>/<name>` as a new project (Phase A: blank 2D only)
/// and returns its path. Refuses an existing non-empty directory.
///
/// Copies the template, installs the addon, writes `.ibproject/` (marker,
/// the starting Concept card, an empty `chat/`), runs `git init`, and makes
/// the first snapshot, "New project". If any step fails, what this call
/// wrote is removed again so the user can simply retry.
pub fn create_project(parent_dir: &Path, name: &str) -> Result<PathBuf> {
    validate_name(name)?;
    let project = parent_dir.join(name);

    let existed = project.exists();
    if existed {
        if !project.is_dir() {
            bail!("{} already exists and is not a folder", project.display());
        }
        let non_empty = fs::read_dir(&project)
            .with_context(|| format!("reading {}", project.display()))?
            .next()
            .is_some();
        if non_empty {
            bail!(
                "{} already exists and isn't empty; choose a new folder",
                project.display()
            );
        }
    } else {
        fs::create_dir_all(&project).with_context(|| format!("creating {}", project.display()))?;
    }

    match populate_project(&project, name) {
        Ok(()) => Ok(project),
        Err(err) => {
            // Everything inside is ours: the folder was missing or empty a
            // moment ago. Put it back the way it was. A folder that already
            // existed is kept itself (it may be a symlink, or have its own
            // permissions); only what we put in it goes.
            if existed {
                remove_children(&project);
            } else {
                let _ = fs::remove_dir_all(&project);
            }
            Err(err)
        }
    }
}

/// Best-effort removal of everything inside `dir`, keeping `dir` itself.
fn remove_children(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_real_dir = fs::symlink_metadata(&path).is_ok_and(|m| m.is_dir());
        let _ = if is_real_dir {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path)
        };
    }
}

/// Installs or updates the addon files and its autoload entry. Returns
/// true if anything on disk changed.
///
/// Only rewrites files whose content differs, and never deletes anything
/// else in `addons/infinabox/`, so running it on an up-to-date project is
/// a no-op. The addon ships the `.uid` files the Godot editor would
/// otherwise generate next to its scripts on first open, so opening the
/// project in the editor doesn't leave uncommitted changes behind.
///
/// The `InfinaBox` autoload name is reserved for the addon: if
/// `project.godot` already has an `InfinaBox` autoload pointing anywhere
/// else, it is replaced. Other autoloads and enabled editor plugins are
/// kept. `project.godot` is edited in place (same line endings, other
/// lines untouched) and written atomically. If its `[editor_plugins]`
/// value can't be parsed safely, this refuses rather than rewriting it.
pub fn ensure_addon(project: &Path) -> Result<bool> {
    let project_file = project.join("project.godot");
    let original = fs::read_to_string(&project_file)
        .with_context(|| format!("reading {}", project_file.display()))?;

    // Work out the new project.godot before touching anything on disk, so
    // a refusal leaves the project as it was.
    let mut updated = set_setting(&original, "autoload", AUTOLOAD_NAME, AUTOLOAD_VALUE)?;
    let plugins = get_setting(&updated, "editor_plugins", "enabled")?;
    let enabled = with_plugin_enabled(plugins.as_deref(), PLUGIN_CFG_RES_PATH)?;
    updated = set_setting(&updated, "editor_plugins", "enabled", &enabled)?;

    let mut changed = write_dir(&ADDON, &project.join(ADDON_DIR), None)?;
    if updated != original {
        write_atomically(&project_file, &updated)?;
        changed = true;
    }
    Ok(changed)
}

/// Writes via a temp file in the same folder plus a rename, so a crash or
/// full disk never leaves a half-written file behind.
fn write_atomically(path: &Path, contents: &str) -> Result<()> {
    let file_name = path
        .file_name()
        .context("path has no file name")?
        .to_string_lossy();
    let tmp = path.with_file_name(format!(".{file_name}.infinabox-tmp"));
    fs::write(&tmp, contents).with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| {
        let _ = fs::remove_file(&tmp);
        format!("replacing {}", path.display())
    })
}

/// Everything `create_project` does after the target folder is ready.
fn populate_project(project: &Path, name: &str) -> Result<()> {
    write_project_files(project, name)?;

    let mut init = git2::RepositoryInitOptions::new();
    init.initial_head("main");
    git2::Repository::init_opts(project, &init)
        .with_context(|| format!("running git init in {}", project.display()))?;

    snapshot::create_snapshot(project, "New project", None)
        .context("saving the first snapshot")?
        .context("saving the first snapshot: nothing to commit")?;
    Ok(())
}

/// Writes every file of a new project (no git): the template with the
/// name filled in, the addon, and `.ibproject/`.
fn write_project_files(project: &Path, name: &str) -> Result<()> {
    write_dir(&TEMPLATE_BLANK_2D, project, Some(name))?;

    let project_file = project.join("project.godot");
    let settings = fs::read_to_string(&project_file)?;
    let settings = set_setting(&settings, "application", "config/name", &godot_string(name))?;
    write_atomically(&project_file, &settings)?;

    ensure_addon(project)?;

    let ib = project.join(".ibproject");
    let marker = ProjectMarker {
        name: name.to_string(),
        created_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        format_version: 2,
        dimension: "2d".to_string(),
    };
    fs::write(
        ib.join(".ibx"),
        serde_json::to_string_pretty(&marker)? + "\n",
    )?;
    // Chat threads are written here by `chat_store`. Git doesn't track empty
    // folders, so it only survives a clone once the first thread exists.
    fs::create_dir_all(ib.join("chat"))?;
    Ok(())
}

/// A project name becomes a folder name, so it must be one path component
/// that is valid on every OS (projects move between machines), which in
/// practice means Windows' rules.
fn validate_name(name: &str) -> Result<()> {
    if name.trim().is_empty() {
        bail!("the project needs a name");
    }
    if name.starts_with(char::is_whitespace) || name.ends_with(char::is_whitespace) {
        bail!("the project name can't start or end with spaces");
    }
    if name.starts_with('.') {
        bail!("the project name can't start with a dot");
    }
    if name.ends_with('.') {
        bail!("the project name can't end with a dot");
    }
    if name.contains(['/', '\\', ':', '<', '>', '"', '|', '?', '*']) {
        bail!("the project name can't contain any of / \\ : < > \" | ? *");
    }
    if name.chars().any(char::is_control) {
        bail!("the project name can't contain control characters");
    }
    // Windows reserves these device names, with or without an extension.
    let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && matches!(stem.as_bytes()[3], b'1'..=b'9'));
    if reserved {
        bail!("\"{name}\" is a reserved name on Windows; choose another");
    }
    Ok(())
}

/// Writes an embedded directory tree under `dest`, skipping files whose
/// content is already identical. With `name`, `{{PROJECT_NAME}}` in `.md`
/// files is replaced by it. Returns true if anything was written.
///
/// Skips any `.godot/` folder: `include_dir!` embeds whatever is on disk,
/// including Godot's cache if someone opened the template in the editor
/// while developing InfinaBox, and that must never reach a user's project.
fn write_dir(dir: &Dir<'_>, dest: &Path, name: Option<&str>) -> Result<bool> {
    let mut changed = false;
    for file in dir.files() {
        if is_godot_cache(file.path()) {
            continue;
        }
        let target = dest.join(file.path());
        let is_md = file.path().extension().is_some_and(|ext| ext == "md");
        let contents: Vec<u8> = match (name, is_md, file.contents_utf8()) {
            (Some(name), true, Some(text)) => text.replace(NAME_PLACEHOLDER, name).into_bytes(),
            _ => file.contents().to_vec(),
        };
        if fs::read(&target).ok().as_deref() == Some(contents.as_slice()) {
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        fs::write(&target, contents).with_context(|| format!("writing {}", target.display()))?;
        changed = true;
    }
    for sub in dir.dirs() {
        if !is_godot_cache(sub.path()) {
            changed |= write_dir(sub, dest, name)?;
        }
    }
    Ok(changed)
}

fn is_godot_cache(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == ".godot")
}

// --- project.godot editing ---------------------------------------------
//
// `project.godot` is an INI-like file: optional top-level keys, then
// `[section]` headers followed by `key=value` lines. A value can span
// several lines (input maps, or an array split across lines); it continues
// until its brackets, parentheses, and braces balance. These helpers
// replace exactly one setting's lines and leave everything else — other
// keys, comments, formatting, line endings — as it was.

/// Where a setting sits: its section's body lines and, if the key is
/// present, the inclusive line range of its (possibly multi-line) value.
struct SettingSpan {
    section: (usize, usize),
    value: Option<(usize, usize)>,
}

/// The (start, end) line range of a section's body, or None if absent.
fn section_range(lines: &[String], section: &str) -> Option<(usize, usize)> {
    let header = format!("[{section}]");
    let start = lines.iter().position(|line| line.trim() == header)? + 1;
    let end = lines[start..]
        .iter()
        .position(|line| {
            line.starts_with('[') && line.trim_end().ends_with(']') && !line.contains('=')
        })
        .map_or(lines.len(), |offset| start + offset);
    Some((start, end))
}

/// Net bracket depth change over `text`, ignoring anything inside strings.
fn bracket_depth(text: &str) -> i32 {
    let (mut depth, mut in_string, mut escaped) = (0, false, false);
    for c in text.chars() {
        if in_string {
            match (escaped, c) {
                (true, _) => escaped = false,
                (false, '\\') => escaped = true,
                (false, '"') => in_string = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ => {}
        }
    }
    depth
}

/// `None` if the section is missing. Errors if the value's brackets never
/// balance within the section, rather than guessing where it ends.
fn find_setting(lines: &[String], section: &str, key: &str) -> Result<Option<SettingSpan>> {
    let Some((start, end)) = section_range(lines, section) else {
        return Ok(None);
    };
    let prefix = format!("{key}=");
    let Some(first) = (start..end).find(|&i| lines[i].starts_with(&prefix)) else {
        return Ok(Some(SettingSpan {
            section: (start, end),
            value: None,
        }));
    };
    let mut depth = bracket_depth(&lines[first][prefix.len()..]);
    let mut last = first;
    while depth > 0 {
        last += 1;
        if last >= end {
            bail!(
                "can't read `{key}` in the [{section}] section of project.godot (its brackets \
                 never close); fix it in Godot and try again"
            );
        }
        depth += bracket_depth(&lines[last]);
    }
    Ok(Some(SettingSpan {
        section: (start, end),
        value: Some((first, last)),
    }))
}

/// The file's lines (without line endings) and the line ending it uses.
fn split_lines(text: &str) -> (Vec<String>, &'static str) {
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    (text.lines().map(str::to_string).collect(), newline)
}

/// A setting's raw value; a multi-line value's lines are joined with `\n`.
fn get_setting(text: &str, section: &str, key: &str) -> Result<Option<String>> {
    let (lines, _) = split_lines(text);
    let Some(SettingSpan {
        value: Some((first, last)),
        ..
    }) = find_setting(&lines, section, key)?
    else {
        return Ok(None);
    };
    let joined = lines[first..=last].join("\n");
    Ok(Some(joined[key.len() + 1..].to_string()))
}

/// Sets `key=value` in `[section]`, adding the section and/or key if
/// missing and replacing every line of an existing multi-line value.
/// Returns the text unchanged if it already has that value.
fn set_setting(text: &str, section: &str, key: &str, value: &str) -> Result<String> {
    if get_setting(text, section, key)?.as_deref() == Some(value) {
        return Ok(text.to_string());
    }
    let entry = format!("{key}={value}");
    let (mut lines, newline) = split_lines(text);

    match find_setting(&lines, section, key)? {
        Some(SettingSpan {
            value: Some((first, last)),
            ..
        }) => {
            lines.splice(first..=last, [entry]);
        }
        Some(SettingSpan {
            section: (start, end),
            value: None,
        }) => {
            // After the section's last non-blank line (Godot leaves one
            // blank line after the header and before the next section).
            match (start..end).rev().find(|&i| !lines[i].trim().is_empty()) {
                Some(i) => lines.insert(i + 1, entry),
                None => {
                    lines.insert(start, String::new());
                    lines.insert(start + 1, entry);
                }
            }
        }
        None => {
            while lines.last().is_some_and(|l| l.trim().is_empty()) {
                lines.pop();
            }
            lines.extend([String::new(), format!("[{section}]"), String::new(), entry]);
        }
    }
    Ok(lines.join(newline) + newline)
}

/// `PackedStringArray(...)` with `plugin` added if it isn't there, keeping
/// any other enabled plugins. Refuses a value it doesn't recognise.
fn with_plugin_enabled(current: Option<&str>, plugin: &str) -> Result<String> {
    let mut plugins: Vec<String> = Vec::new();
    if let Some(value) = current.map(str::trim) {
        if !(value.starts_with("PackedStringArray(") && value.ends_with(')')) {
            bail!(
                "`enabled` in the [editor_plugins] section of project.godot isn't a \
                 PackedStringArray; fix it in Godot and try again"
            );
        }
        let re = regex::Regex::new(r#""((?:[^"\\]|\\.)*)""#).expect("valid regex");
        plugins = re.captures_iter(value).map(|c| c[1].to_string()).collect();
    }
    if !plugins.iter().any(|p| p == plugin) {
        plugins.push(plugin.to_string());
    }
    let quoted: Vec<String> = plugins.iter().map(|p| format!("\"{p}\"")).collect();
    Ok(format!("PackedStringArray({})", quoted.join(", ")))
}

/// A Godot string literal.
fn godot_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(path: &Path) -> String {
        fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
    }

    fn setting(text: &str, section: &str, key: &str) -> Option<String> {
        get_setting(text, section, key).unwrap()
    }

    /// Every file path embedded from `dir`, recursively.
    fn embedded_paths(dir: &Dir<'_>) -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = dir.files().map(|f| f.path().to_path_buf()).collect();
        for sub in dir.dirs() {
            paths.push(sub.path().to_path_buf());
            paths.extend(embedded_paths(sub));
        }
        paths
    }

    /// `include_dir!` embeds whatever is on disk, so a `.godot/` cache from
    /// opening the template in the editor would ship to every new project.
    /// `write_dir` skips it anyway; this catches it at the source.
    #[test]
    fn scaffold_embeds_no_godot_cache() {
        for (label, dir) in [("template", &TEMPLATE_BLANK_2D), ("addon", &ADDON)] {
            for path in embedded_paths(dir) {
                assert!(
                    !is_godot_cache(&path),
                    "{label} embeds {} — delete that .godot/ folder",
                    path.display()
                );
            }
        }
        assert!(is_godot_cache(Path::new(".godot/imported/x.ctex")));
        assert!(!is_godot_cache(Path::new("addons/infinabox/plugin.cfg")));
    }

    #[test]
    fn scaffold_edits_multi_line_settings_whole() {
        let original = "config_version=5\n\n[editor_plugins]\n\n\
            enabled=PackedStringArray(\"res://addons/a/plugin.cfg\",\n\"res://addons/b/plugin.cfg\")\n\n\
            [input]\n\njump={\n\"deadzone\": 0.5,\n\"events\": [Object(InputEventKey,\"keycode\":32)]\n}\n";
        assert_eq!(
            setting(original, "editor_plugins", "enabled").as_deref(),
            Some(
                "PackedStringArray(\"res://addons/a/plugin.cfg\",\n\"res://addons/b/plugin.cfg\")"
            )
        );
        assert_eq!(
            setting(original, "input", "jump").as_deref(),
            Some("{\n\"deadzone\": 0.5,\n\"events\": [Object(InputEventKey,\"keycode\":32)]\n}")
        );

        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("project.godot"), original).unwrap();
        assert!(ensure_addon(tmp.path()).unwrap());
        let settings = read(&tmp.path().join("project.godot"));
        assert_eq!(
            setting(&settings, "editor_plugins", "enabled").as_deref(),
            Some(
                "PackedStringArray(\"res://addons/a/plugin.cfg\", \"res://addons/b/plugin.cfg\", \
                 \"res://addons/infinabox/plugin.cfg\")"
            )
        );
        assert!(
            !settings.contains("\n\"res://addons/b/plugin.cfg\")"),
            "left a dangling line:\n{settings}"
        );
        assert!(settings.contains("jump={\n\"deadzone\": 0.5,\n"));
        assert!(!ensure_addon(tmp.path()).unwrap(), "second run is a no-op");
    }

    #[test]
    fn scaffold_refuses_settings_it_cannot_parse() {
        let tmp = tempfile::tempdir().unwrap();
        let unbalanced =
            "config_version=5\n\n[editor_plugins]\n\nenabled=PackedStringArray(\"a\",\n\n[input]\n";
        fs::write(tmp.path().join("project.godot"), unbalanced).unwrap();
        assert!(ensure_addon(tmp.path()).is_err());
        // Refused before touching anything.
        assert_eq!(read(&tmp.path().join("project.godot")), unbalanced);
        assert!(!tmp.path().join("addons").exists());

        let odd = "config_version=5\n\n[editor_plugins]\n\nenabled=[\"a\"]\n";
        fs::write(tmp.path().join("project.godot"), odd).unwrap();
        assert!(ensure_addon(tmp.path()).is_err());
        assert_eq!(read(&tmp.path().join("project.godot")), odd);
    }

    #[test]
    fn scaffold_keeps_crlf_line_endings() {
        let tmp = tempfile::tempdir().unwrap();
        let original = "config_version=5\r\n\r\n[application]\r\n\r\nconfig/name=\"Mine\"\r\n";
        fs::write(tmp.path().join("project.godot"), original).unwrap();
        assert!(ensure_addon(tmp.path()).unwrap());
        let settings = read(&tmp.path().join("project.godot"));
        assert!(settings.starts_with(original));
        assert!(
            !settings.replace("\r\n", "").contains('\n'),
            "mixed line endings:\n{settings:?}"
        );
        assert_eq!(
            setting(&settings, "autoload", "InfinaBox").as_deref(),
            Some(AUTOLOAD_VALUE)
        );
        assert!(!ensure_addon(tmp.path()).unwrap());
    }

    #[test]
    fn scaffold_replaces_a_different_infinabox_autoload() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("project.godot"),
            "[autoload]\n\nInfinaBox=\"*res://mine.gd\"\n",
        )
        .unwrap();
        assert!(ensure_addon(tmp.path()).unwrap());
        let settings = read(&tmp.path().join("project.godot"));
        assert_eq!(
            setting(&settings, "autoload", "InfinaBox").as_deref(),
            Some(AUTOLOAD_VALUE)
        );
        assert!(!settings.contains("mine.gd"));
    }

    /// The file layout of a new project, without git (so it doesn't depend
    /// on `snapshot`).
    #[test]
    fn scaffold_writes_the_expected_files() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("Star Hopper");
        fs::create_dir(&project).unwrap();
        write_project_files(&project, "Star Hopper").unwrap();

        for file in [
            "project.godot",
            "main.tscn",
            "player.gd",
            "player.gd.uid",
            ".gitignore",
            "CLAUDE.md",
            "AGENTS.md",
            "addons/infinabox/plugin.cfg",
            "addons/infinabox/infinabox_plugin.gd",
            "addons/infinabox/infinabox_runtime.gd",
            "addons/infinabox/infinabox_runtime.gd.uid",
            ".ibproject/.ibx",
            ".ibproject/context/concept.md",
        ] {
            assert!(project.join(file).is_file(), "missing {file}");
        }
        assert!(project.join(".ibproject/chat").is_dir());
        assert!(
            read(&project.join(".gitignore"))
                .lines()
                .any(|l| l.trim() == ".godot/")
        );

        let marker: ProjectMarker =
            serde_json::from_str(&read(&project.join(".ibproject/.ibx"))).unwrap();
        assert_eq!(marker.name, "Star Hopper");
        assert_eq!(marker.format_version, 2);
        assert_eq!(marker.dimension, "2d");
        chrono::DateTime::parse_from_rfc3339(&marker.created_at).unwrap();
        let raw: serde_json::Value =
            serde_json::from_str(&read(&project.join(".ibproject/.ibx"))).unwrap();
        assert_eq!(raw["formatVersion"], 2);
        assert!(raw["createdAt"].is_string());

        let settings = read(&project.join("project.godot"));
        assert_eq!(
            setting(&settings, "application", "config/name").as_deref(),
            Some("\"Star Hopper\"")
        );
        assert_eq!(
            setting(&settings, "application", "run/main_scene").as_deref(),
            Some("\"res://main.tscn\"")
        );
        assert_eq!(
            setting(&settings, "autoload", "InfinaBox").as_deref(),
            Some(AUTOLOAD_VALUE)
        );
        assert_eq!(
            setting(&settings, "editor_plugins", "enabled").as_deref(),
            Some("PackedStringArray(\"res://addons/infinabox/plugin.cfg\")")
        );

        let concept = read(&project.join(".ibproject/context/concept.md"));
        assert!(concept.starts_with("---\ntype: concept\n"));
        assert!(concept.contains("# Star Hopper"));
        assert!(read(&project.join("AGENTS.md")).starts_with("# Star Hopper"));
        assert!(read(&project.join("CLAUDE.md")).contains("@AGENTS.md"));
        for md in ["CLAUDE.md", "AGENTS.md", ".ibproject/context/concept.md"] {
            assert!(
                !read(&project.join(md)).contains(NAME_PLACEHOLDER),
                "{md} kept the placeholder"
            );
        }
    }

    /// The full flow, including `git init` and the first snapshot.
    ///
    /// DEPENDS ON TASK D: `snapshot::create_snapshot`/`list_snapshots` are
    /// Wave 0 stubs that error until Task D merges, so this test fails
    /// until then. The layout itself is covered by
    /// `scaffold_writes_the_expected_files` above.
    #[test]
    fn scaffold_create_project_makes_exactly_one_snapshot() {
        let tmp = tempfile::tempdir().unwrap();
        let project = create_project(tmp.path(), "My Game").unwrap();
        assert_eq!(project, tmp.path().join("My Game"));
        assert!(project.join(".git").is_dir());
        assert!(project.join(".ibproject/.ibx").is_file());

        let snapshots = snapshot::list_snapshots(&project, 10).unwrap();
        assert_eq!(snapshots.len(), 1, "{snapshots:?}");
        assert_eq!(snapshots[0].title, "New project");

        // Everything was committed.
        let repo = git2::Repository::open(&project).unwrap();
        let statuses = repo.statuses(None).unwrap();
        assert!(
            statuses.is_empty(),
            "uncommitted files after create_project"
        );
    }

    #[test]
    fn scaffold_refuses_a_non_empty_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("taken");
        fs::create_dir(&project).unwrap();
        fs::write(project.join("notes.txt"), "mine").unwrap();

        let err = create_project(tmp.path(), "taken").unwrap_err();
        assert!(err.to_string().contains("isn't empty"), "{err}");
        // The user's file is untouched and nothing was added.
        assert_eq!(read(&project.join("notes.txt")), "mine");
        assert_eq!(fs::read_dir(&project).unwrap().count(), 1);

        fs::write(tmp.path().join("a-file"), "").unwrap();
        assert!(create_project(tmp.path(), "a-file").is_err());
    }

    #[test]
    fn scaffold_refuses_names_that_are_not_one_folder() {
        let tmp = tempfile::tempdir().unwrap();
        for name in [
            "",
            "   ",
            " padded",
            "padded ",
            "..",
            ".hidden",
            "trailing.",
            "a/b",
            "a\\b",
            "a:b",
            "a<b",
            "a>b",
            "a\"b",
            "a|b",
            "a?b",
            "a*b",
            "tab\there",
            "new\nline",
            "nul\0",
            "bell\u{7}",
            "CON",
            "con",
            "Prn",
            "aux.txt",
            "NUL.tar.gz",
            "COM1",
            "com9",
            "LPT1",
            "lpt5.md",
        ] {
            assert!(
                create_project(tmp.path(), name).is_err(),
                "accepted {name:?}"
            );
        }
        assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 0);

        for name in [
            "My Game",
            "Console",
            "COM0",
            "COM10",
            "LPTX",
            "aux-game",
            "v1.2 final",
            "Ünïcødé 游戏",
        ] {
            validate_name(name).unwrap_or_else(|e| panic!("refused {name:?}: {e}"));
        }
    }

    /// When creation fails part-way into a folder that already existed
    /// (empty), what was written goes but the folder itself stays.
    #[test]
    fn scaffold_cleanup_keeps_an_existing_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("kept");
        fs::create_dir(&project).unwrap();
        fs::write(project.join("project.godot"), "x").unwrap();
        fs::create_dir_all(project.join("addons/infinabox")).unwrap();
        remove_children(&project);
        assert!(project.is_dir());
        assert_eq!(fs::read_dir(&project).unwrap().count(), 0);
    }

    #[test]
    fn scaffold_ensure_addon_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        fs::write(
            project.join("project.godot"),
            TEMPLATE_BLANK_2D
                .get_file("project.godot")
                .unwrap()
                .contents(),
        )
        .unwrap();

        assert!(
            ensure_addon(project).unwrap(),
            "first install changes things"
        );
        let settings = read(&project.join("project.godot"));
        assert!(!ensure_addon(project).unwrap(), "second run is a no-op");
        assert_eq!(read(&project.join("project.godot")), settings);

        // Updating: a stale addon file is rewritten, and other files in the
        // addon folder are left alone.
        let runtime = project.join("addons/infinabox/infinabox_runtime.gd");
        fs::write(&runtime, "extends Node\n").unwrap();
        fs::write(project.join("addons/infinabox/extra.txt"), "not ours\n").unwrap();
        assert!(ensure_addon(project).unwrap());
        assert!(read(&runtime).contains("[infinabox] ready"));
        assert_eq!(
            read(&project.join("addons/infinabox/extra.txt")),
            "not ours\n"
        );
        assert!(!ensure_addon(project).unwrap());
    }

    #[test]
    fn scaffold_ensure_addon_keeps_other_settings_and_plugins() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        let original = "config_version=5\n\n[application]\n\nconfig/name=\"Mine\"\n\n\
            [autoload]\n\nMusic=\"*res://music.gd\"\n\n\
            [editor_plugins]\n\nenabled=PackedStringArray(\"res://addons/other/plugin.cfg\")\n\n\
            [input]\n\njump={\n\"deadzone\": 0.5,\n\"events\": []\n}\n";
        fs::write(project.join("project.godot"), original).unwrap();

        assert!(ensure_addon(project).unwrap());
        let settings = read(&project.join("project.godot"));
        assert_eq!(
            setting(&settings, "autoload", "Music").as_deref(),
            Some("\"*res://music.gd\"")
        );
        assert_eq!(
            setting(&settings, "autoload", "InfinaBox").as_deref(),
            Some(AUTOLOAD_VALUE)
        );
        assert_eq!(
            setting(&settings, "editor_plugins", "enabled").as_deref(),
            Some(
                "PackedStringArray(\"res://addons/other/plugin.cfg\", \"res://addons/infinabox/plugin.cfg\")"
            )
        );
        assert!(settings.contains("jump={\n\"deadzone\": 0.5,\n\"events\": []\n}\n"));
        assert!(settings.contains("config/name=\"Mine\""));
        assert!(!ensure_addon(project).unwrap());
    }

    #[test]
    fn scaffold_addon_versions_agree() {
        let runtime = ADDON
            .get_file("infinabox_runtime.gd")
            .unwrap()
            .contents_utf8()
            .unwrap();
        assert!(runtime.contains(&format!("const VERSION := \"{ADDON_VERSION}\"")));
        let cfg = ADDON
            .get_file("plugin.cfg")
            .unwrap()
            .contents_utf8()
            .unwrap();
        assert!(cfg.contains(&format!("version=\"{ADDON_VERSION}\"")));
    }

    #[test]
    fn scaffold_escapes_the_name_in_project_godot() {
        assert_eq!(godot_string(r#"Say "hi" \o/"#), r#""Say \"hi\" \\o/""#);
    }

    /// Boots a freshly scaffolded project in a real Godot, headless, the way
    /// the Task 0.2 fixtures were recorded, and checks it starts cleanly
    /// with the addon loaded. Uses the Godot binary directly because Task
    /// C's `validate` was being built in parallel.
    #[test]
    #[ignore = "needs Godot (set INFINABOX_GODOT); run with --ignored"]
    fn scaffold_boots_in_real_godot() {
        let godot =
            std::env::var("INFINABOX_GODOT").expect("set INFINABOX_GODOT to a Godot 4 binary");
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("Boot Test");
        fs::create_dir(&project).unwrap();
        write_project_files(&project, "Boot Test").unwrap();

        let output = std::process::Command::new(godot)
            .args(["--headless", "--path"])
            .arg(&project)
            .args(["--quit-after", "30"])
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "godot exited with {}\n{stderr}",
            output.status
        );
        // Godot exits 0 even when scripts fail; stderr is the error signal.
        assert!(stderr.trim().is_empty(), "godot wrote to stderr:\n{stderr}");
        assert!(
            stdout
                .lines()
                .any(|l| l == format!("[infinabox] ready {ADDON_VERSION}")),
            "no ready line in stdout:\n{stdout}"
        );
    }
}
