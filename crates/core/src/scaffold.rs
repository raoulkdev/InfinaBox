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
            bail!("{} already exists and isn't empty; choose a new folder", project.display());
        }
    } else {
        fs::create_dir_all(&project).with_context(|| format!("creating {}", project.display()))?;
    }

    match populate_project(&project, name) {
        Ok(()) => Ok(project),
        Err(err) => {
            // Everything inside is ours: the folder was missing or empty a
            // moment ago. Put it back the way it was.
            let _ = fs::remove_dir_all(&project);
            if existed {
                let _ = fs::create_dir(&project);
            }
            Err(err)
        }
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
pub fn ensure_addon(project: &Path) -> Result<bool> {
    let project_file = project.join("project.godot");
    let original = fs::read_to_string(&project_file)
        .with_context(|| format!("reading {}", project_file.display()))?;

    let mut changed = write_dir(&ADDON, &project.join(ADDON_DIR), None)?;

    let mut updated = set_setting(&original, "autoload", AUTOLOAD_NAME, AUTOLOAD_VALUE);
    let plugins = get_setting(&updated, "editor_plugins", "enabled");
    let enabled = with_plugin_enabled(plugins.as_deref(), PLUGIN_CFG_RES_PATH);
    updated = set_setting(&updated, "editor_plugins", "enabled", &enabled);

    if updated != original {
        fs::write(&project_file, updated)
            .with_context(|| format!("writing {}", project_file.display()))?;
        changed = true;
    }
    Ok(changed)
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
    let settings = set_setting(&settings, "application", "config/name", &godot_string(name));
    fs::write(&project_file, settings)?;

    ensure_addon(project)?;

    let ib = project.join(".ibproject");
    let marker = ProjectMarker {
        name: name.to_string(),
        created_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        format_version: 2,
        dimension: "2d".to_string(),
    };
    fs::write(ib.join(".ibx"), serde_json::to_string_pretty(&marker)? + "\n")?;
    // Chat threads are written here by `chat_store`. Git doesn't track empty
    // folders, so it only survives a clone once the first thread exists.
    fs::create_dir_all(ib.join("chat"))?;
    Ok(())
}

/// A project name becomes a folder name, so it must be one path component.
fn validate_name(name: &str) -> Result<()> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        bail!("the project needs a name");
    }
    if trimmed != name {
        bail!("the project name can't start or end with spaces");
    }
    if name == "." || name == ".." || name.starts_with('.') {
        bail!("the project name can't start with a dot");
    }
    if name.contains(['/', '\\', ':', '\0']) {
        bail!("the project name can't contain / \\ or :");
    }
    Ok(())
}

/// Writes an embedded directory tree under `dest`, skipping files whose
/// content is already identical. With `name`, `{{PROJECT_NAME}}` in `.md`
/// files is replaced by it. Returns true if anything was written.
fn write_dir(dir: &Dir<'_>, dest: &Path, name: Option<&str>) -> Result<bool> {
    let mut changed = false;
    for file in dir.files() {
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
        changed |= write_dir(sub, dest, name)?;
    }
    Ok(changed)
}

// --- project.godot editing ---------------------------------------------
//
// `project.godot` is an INI-like file: optional top-level keys, then
// `[section]` headers followed by `key=value` lines (a value can span
// several lines, e.g. input maps, but only its first line starts with
// `key=`). These helpers touch single lines only and leave everything else
// — other keys, comments, formatting — exactly as it was.

/// The (start, end) line range of a section's body, or None if absent.
fn section_range(lines: &[&str], section: &str) -> Option<(usize, usize)> {
    let header = format!("[{section}]");
    let start = lines.iter().position(|line| line.trim() == header)? + 1;
    let end = lines[start..]
        .iter()
        .position(|line| line.starts_with('[') && line.trim_end().ends_with(']') && !line.contains('='))
        .map_or(lines.len(), |offset| start + offset);
    Some((start, end))
}

fn get_setting(text: &str, section: &str, key: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let (start, end) = section_range(&lines, section)?;
    let prefix = format!("{key}=");
    lines[start..end]
        .iter()
        .find_map(|line| line.strip_prefix(prefix.as_str()).map(str::to_string))
}

/// Sets `key=value` in `[section]`, adding the section and/or key if
/// missing. Returns the text unchanged if it already has that value.
fn set_setting(text: &str, section: &str, key: &str, value: &str) -> String {
    let entry = format!("{key}={value}");
    let prefix = format!("{key}=");
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let borrowed: Vec<&str> = lines.iter().map(String::as_str).collect();

    match section_range(&borrowed, section) {
        Some((start, end)) => {
            if let Some(offset) = borrowed[start..end].iter().position(|l| l.starts_with(&prefix)) {
                if lines[start + offset] == entry {
                    return text.to_string();
                }
                lines[start + offset] = entry;
            } else {
                // After the section's last non-blank line (Godot leaves one
                // blank line after the header and before the next section).
                let last = (start..end).rev().find(|&i| !lines[i].trim().is_empty());
                match last {
                    Some(i) => lines.insert(i + 1, entry),
                    None => {
                        lines.insert(start, String::new());
                        lines.insert(start + 1, entry);
                    }
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
    lines.join("\n") + "\n"
}

/// `enabled=PackedStringArray(...)` with `plugin` added if it isn't there,
/// keeping any other enabled plugins.
fn with_plugin_enabled(current: Option<&str>, plugin: &str) -> String {
    let mut plugins: Vec<String> = current
        .map(|value| {
            let re = regex::Regex::new(r#""((?:[^"\\]|\\.)*)""#).expect("valid regex");
            re.captures_iter(value).map(|c| c[1].to_string()).collect()
        })
        .unwrap_or_default();
    if !plugins.iter().any(|p| p == plugin) {
        plugins.push(plugin.to_string());
    }
    let quoted: Vec<String> = plugins.iter().map(|p| format!("\"{p}\"")).collect();
    format!("PackedStringArray({})", quoted.join(", "))
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
        assert!(read(&project.join(".gitignore")).lines().any(|l| l.trim() == ".godot/"));

        let marker: ProjectMarker = serde_json::from_str(&read(&project.join(".ibproject/.ibx"))).unwrap();
        assert_eq!(marker.name, "Star Hopper");
        assert_eq!(marker.format_version, 2);
        assert_eq!(marker.dimension, "2d");
        chrono::DateTime::parse_from_rfc3339(&marker.created_at).unwrap();
        let raw: serde_json::Value = serde_json::from_str(&read(&project.join(".ibproject/.ibx"))).unwrap();
        assert_eq!(raw["formatVersion"], 2);
        assert!(raw["createdAt"].is_string());

        let settings = read(&project.join("project.godot"));
        assert_eq!(get_setting(&settings, "application", "config/name").as_deref(), Some("\"Star Hopper\""));
        assert_eq!(get_setting(&settings, "application", "run/main_scene").as_deref(), Some("\"res://main.tscn\""));
        assert_eq!(get_setting(&settings, "autoload", "InfinaBox").as_deref(), Some(AUTOLOAD_VALUE));
        assert_eq!(
            get_setting(&settings, "editor_plugins", "enabled").as_deref(),
            Some("PackedStringArray(\"res://addons/infinabox/plugin.cfg\")")
        );

        let concept = read(&project.join(".ibproject/context/concept.md"));
        assert!(concept.starts_with("---\ntype: concept\n"));
        assert!(concept.contains("# Star Hopper"));
        assert!(read(&project.join("AGENTS.md")).starts_with("# Star Hopper"));
        assert!(read(&project.join("CLAUDE.md")).contains("@AGENTS.md"));
        for md in ["CLAUDE.md", "AGENTS.md", ".ibproject/context/concept.md"] {
            assert!(!read(&project.join(md)).contains(NAME_PLACEHOLDER), "{md} kept the placeholder");
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
        assert!(statuses.is_empty(), "uncommitted files after create_project");
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
        for name in ["", " padded ", "..", ".hidden", "a/b", "a\\b"] {
            assert!(create_project(tmp.path(), name).is_err(), "accepted {name:?}");
        }
        assert_eq!(fs::read_dir(tmp.path()).unwrap().count(), 0);
    }

    #[test]
    fn scaffold_ensure_addon_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path();
        fs::write(project.join("project.godot"), TEMPLATE_BLANK_2D.get_file("project.godot").unwrap().contents())
            .unwrap();

        assert!(ensure_addon(project).unwrap(), "first install changes things");
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
        assert_eq!(read(&project.join("addons/infinabox/extra.txt")), "not ours\n");
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
        assert_eq!(get_setting(&settings, "autoload", "Music").as_deref(), Some("\"*res://music.gd\""));
        assert_eq!(get_setting(&settings, "autoload", "InfinaBox").as_deref(), Some(AUTOLOAD_VALUE));
        assert_eq!(
            get_setting(&settings, "editor_plugins", "enabled").as_deref(),
            Some("PackedStringArray(\"res://addons/other/plugin.cfg\", \"res://addons/infinabox/plugin.cfg\")")
        );
        assert!(settings.contains("jump={\n\"deadzone\": 0.5,\n\"events\": []\n}\n"));
        assert!(settings.contains("config/name=\"Mine\""));
        assert!(!ensure_addon(project).unwrap());
    }

    #[test]
    fn scaffold_addon_versions_agree() {
        let runtime = ADDON.get_file("infinabox_runtime.gd").unwrap().contents_utf8().unwrap();
        assert!(runtime.contains(&format!("const VERSION := \"{ADDON_VERSION}\"")));
        let cfg = ADDON.get_file("plugin.cfg").unwrap().contents_utf8().unwrap();
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
        let godot = std::env::var("INFINABOX_GODOT").expect("set INFINABOX_GODOT to a Godot 4 binary");
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
        assert!(output.status.success(), "godot exited with {}\n{stderr}", output.status);
        // Godot exits 0 even when scripts fail; stderr is the error signal.
        assert!(stderr.trim().is_empty(), "godot wrote to stderr:\n{stderr}");
        assert!(
            stdout.lines().any(|l| l == format!("[infinabox] ready {ADDON_VERSION}")),
            "no ready line in stdout:\n{stdout}"
        );
    }
}
