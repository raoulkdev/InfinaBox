//! Sandboxed file tools for the API runtimes: `read_file`, `write_file`,
//! `edit_file` (exact-string replace), `list_files`, `search_files`. Paths
//! are project-relative and must stay inside the project (symlinks are
//! followed only if they lead back inside it); writes to `.git`,
//! `addons/infinabox`, `.ibproject/.ibx` and `.ibproject/chat` are refused —
//! the same limits the CLI runtimes work under.
//!
//! Every tool returns `Result<_, String>`: an `Err` is text the model reads
//! as a failed tool result and can act on, never a crash. The events the
//! app sees use the CLI runtimes' tool names (`Read`, `Write`, `Edit`,
//! `Glob`, `Grep`) so its "may this have changed the game?" check treats
//! both kinds of runtime alike: see `event_name`.

use std::fs;
use std::io::Write as _;
use std::path::{Component, Path, PathBuf};

use regex::Regex;
use serde_json::{Value, json};

use super::ToolSpec;

/// Most a `read_file` returns, in bytes of output.
pub const MAX_READ_BYTES: usize = 256 * 1024;
/// Largest file `read_file` will open at all (it is read whole to number the lines).
const MAX_OPEN_BYTES: u64 = 16 * 1024 * 1024;
/// Most `write_file` content accepted, in bytes.
pub const MAX_WRITE_BYTES: usize = 8 * 1024 * 1024;
/// Most paths `list_files` returns.
pub const MAX_LIST_ENTRIES: usize = 500;
/// Most matching lines `search_files` returns.
pub const MAX_SEARCH_MATCHES: usize = 200;
/// Files bigger than this are skipped by `search_files`.
const MAX_SEARCH_FILE_BYTES: u64 = 2 * 1024 * 1024;
/// Longest matching line shown by `search_files`, in characters.
const MAX_MATCH_LINE_CHARS: usize = 300;
/// Deepest folder nesting the walkers descend into.
const MAX_DEPTH: usize = 32;

/// The names of the file tools, as the model sees them.
pub const FILE_TOOL_NAMES: [&str; 5] = [
    "read_file",
    "write_file",
    "edit_file",
    "list_files",
    "search_files",
];

pub fn is_file_tool(name: &str) -> bool {
    FILE_TOOL_NAMES.contains(&name)
}

/// The name a tool use goes by in `AgentEvent::ToolUse`: the CLI runtimes'
/// name for the same job for file tools, `mcp__infinabox__<tool>` for the
/// InfinaBox server's tools (`is_mcp`), the model's own name otherwise.
pub fn event_name(name: &str, is_mcp: bool) -> String {
    match name {
        "read_file" => "Read".into(),
        "write_file" => "Write".into(),
        "edit_file" => "Edit".into(),
        "list_files" => "Glob".into(),
        "search_files" => "Grep".into(),
        other if is_mcp => format!("mcp__infinabox__{other}"),
        other => other.to_string(),
    }
}

/// A short plain-language line for one tool use, worded like the CLI
/// runtimes' ("Editing player.gd", "Running the game"). Never the raw input.
pub fn summary(name: &str, args: &Value) -> String {
    let field = |key: &str| args.get(key).and_then(Value::as_str).map(str::trim);
    let on_file = |verb: &str, fallback: &str| match field("path") {
        Some(p) if !p.is_empty() => format!("{verb} {}", display_path(p)),
        _ => fallback.to_string(),
    };
    match name {
        "read_file" => on_file("Reading", "Reading a file"),
        "write_file" => on_file("Writing", "Writing a file"),
        "edit_file" => on_file("Editing", "Editing a file"),
        "list_files" => "Looking for files".into(),
        "search_files" => "Searching the project".into(),
        "run_game" => "Running the game".into(),
        "stop_game" => "Stopping the game".into(),
        "get_game_status" => "Checking whether the game is running".into(),
        "get_game_errors" => "Checking the game for errors".into(),
        "get_game_output" => "Reading the game's output".into(),
        "list_context_cards" => "Looking through the Context cards".into(),
        "read_context_card" => match field("path") {
            Some(c) => format!("Reading the {} Context card", c.trim_end_matches(".md")),
            None => "Reading a Context card".into(),
        },
        "search_context" => "Searching the Context cards".into(),
        "write_context_card" => match field("path") {
            Some(c) => format!("Updating the {} Context card", c.trim_end_matches(".md")),
            None => "Updating a Context card".into(),
        },
        "project_map" => "Looking at how the game is put together".into(),
        "find_symbol" => "Looking up where something is used".into(),
        "describe_scene" => "Reading a scene".into(),
        "playtest" => "Playing the game to check it".into(),
        "list_snapshots" => "Looking at the project's history".into(),
        "propose_plan" => "Writing up a plan".into(),
        other => format!("Using {}", other.replace('_', " ")),
    }
}

/// A path as shown to the person: as given without a leading `./`, or just
/// the file name for an absolute path (never someone's home directory).
fn display_path(raw: &str) -> String {
    let p = Path::new(raw);
    if p.is_absolute() {
        return p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| raw.to_string());
    }
    raw.trim_start_matches("./").to_string()
}

/// The file tools' JSON Schemas, for the model.
pub fn specs() -> Vec<ToolSpec> {
    let spec = |name: &str, description: &str, parameters: Value| ToolSpec {
        name: name.into(),
        description: description.into(),
        parameters,
    };
    vec![
        spec(
            "read_file",
            "Read a text file in the project. Lines come back numbered. Use offset and limit to read part of a long file.",
            json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "File path relative to the project folder, e.g. \"scripts/player.gd\"."},
                    "offset": {"type": "integer", "description": "First line to read (1 = the start). Optional."},
                    "limit": {"type": "integer", "description": "How many lines to read. Optional."}
                },
                "required": ["path"]
            }),
        ),
        spec(
            "write_file",
            "Create a file in the project, or replace it completely, with the given text. Missing folders are created. For a small change to an existing file use edit_file instead.",
            json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "File path relative to the project folder."},
                    "content": {"type": "string", "description": "The whole new contents of the file."}
                },
                "required": ["path", "content"]
            }),
        ),
        spec(
            "edit_file",
            "Change a file by replacing an exact piece of its text. old_string must match the file exactly (including spaces and line breaks) and be unique in it, unless replace_all is true. Read the file first.",
            json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "File path relative to the project folder."},
                    "old_string": {"type": "string", "description": "The exact text to replace. Must not be empty."},
                    "new_string": {"type": "string", "description": "The text to put in its place."},
                    "replace_all": {"type": "boolean", "description": "Replace every occurrence instead of requiring exactly one. Optional."}
                },
                "required": ["path", "old_string", "new_string"]
            }),
        ),
        spec(
            "list_files",
            "List the files in the project (folders like .git and .godot and hidden files are skipped). Shows at most 500. Optionally start from a folder and/or keep only names matching a pattern such as \"*.gd\" or \"scenes/**/*.tscn\".",
            json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Folder relative to the project folder. Defaults to the whole project."},
                    "pattern": {"type": "string", "description": "Glob pattern: * matches within a name, ** across folders, ? one character. A pattern without / is matched against file names."}
                }
            }),
        ),
        spec(
            "search_files",
            "Search the text files of the project for a regular expression. Returns matching lines as path:line: text, at most 200.",
            json!({
                "type": "object",
                "properties": {
                    "pattern": {"type": "string", "description": "Regular expression to look for."},
                    "path": {"type": "string", "description": "File or folder to search, relative to the project folder. Defaults to the whole project."},
                    "glob": {"type": "string", "description": "Only search files whose names match this glob, e.g. \"*.gd\". Optional."}
                },
                "required": ["pattern"]
            }),
        ),
    ]
}

/// What a successful file tool call gives back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileToolOutput {
    pub text: String,
    /// The project-relative path this call wrote, for `FilesChanged`.
    pub written: Option<String>,
}

impl FileToolOutput {
    fn text(text: String) -> Self {
        Self {
            text,
            written: None,
        }
    }
}

/// A path checked against the sandbox.
struct Resolved {
    abs: PathBuf,
    /// Project-relative, `/`-separated, `""` for the project folder itself.
    rel: String,
    /// The same after resolving symlinks in the part that exists.
    canonical_rel: String,
}

/// The file tools, bound to one project folder.
pub struct FileTools {
    /// The project folder with symlinks resolved.
    root: PathBuf,
    /// The project folder as given, for absolute paths spelled that way.
    given_root: PathBuf,
}

impl FileTools {
    pub fn new(project: &Path) -> Result<Self, String> {
        let root = project
            .canonicalize()
            .map_err(|e| format!("The project folder can't be opened: {e}"))?;
        Ok(Self {
            root,
            given_root: project.to_path_buf(),
        })
    }

    /// Runs one file tool. `Err` is the message for the model.
    pub fn call(&self, name: &str, args: &Value) -> Result<FileToolOutput, String> {
        match name {
            "read_file" => self.read_file(args).map(FileToolOutput::text),
            "write_file" => self.write_file(args),
            "edit_file" => self.edit_file(args),
            "list_files" => self.list_files(args).map(FileToolOutput::text),
            "search_files" => self.search_files(args).map(FileToolOutput::text),
            other => Err(format!("There is no file tool called {other}.")),
        }
    }

    // ---- sandbox --------------------------------------------------------

    fn resolve(&self, raw: &str) -> Result<Resolved, String> {
        let raw = raw.trim();
        let outside = || {
            format!(
                "\"{}\" is outside the project. Use a path inside the project folder.",
                display_path(raw)
            )
        };
        let path = Path::new(raw);
        let relative: &Path = if path.is_absolute() {
            path.strip_prefix(&self.root)
                .or_else(|_| path.strip_prefix(&self.given_root))
                .map_err(|_| outside())?
        } else {
            path
        };
        let mut parts: Vec<String> = Vec::new();
        for component in relative.components() {
            match component {
                Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
                Component::CurDir => {}
                Component::ParentDir => {
                    parts.pop().ok_or_else(outside)?;
                }
                Component::RootDir | Component::Prefix(_) => return Err(outside()),
            }
        }
        let abs = parts.iter().fold(self.root.clone(), |p, part| p.join(part));

        // Follow symlinks in whatever part of the path exists already and make
        // sure it still ends up inside the project.
        let mut anchor = abs.clone();
        let mut remainder: Vec<&str> = Vec::new();
        while fs::symlink_metadata(&anchor).is_err() {
            let Some(i) = parts.len().checked_sub(remainder.len() + 1) else {
                break;
            };
            remainder.push(parts[i].as_str());
            anchor.pop();
        }
        let canonical_anchor = anchor.canonicalize().map_err(|_| outside())?;
        if !canonical_anchor.starts_with(&self.root) {
            return Err(outside());
        }
        let canonical_rel = {
            let rel = canonical_anchor
                .strip_prefix(&self.root)
                .unwrap_or(Path::new(""));
            let mut segments: Vec<String> = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            segments.extend(remainder.iter().rev().map(|s| s.to_string()));
            segments.join("/")
        };
        Ok(Resolved {
            abs,
            rel: parts.join("/"),
            canonical_rel,
        })
    }

    fn check_writable(&self, target: &Resolved) -> Result<(), String> {
        if target.rel.is_empty() {
            return Err("That is the project folder itself, not a file.".into());
        }
        for rel in [&target.rel, &target.canonical_rel] {
            if let Some(reason) = protected(rel) {
                return Err(format!(
                    "\"{}\" is {reason}, so it can't be changed. Pick another file.",
                    target.rel
                ));
            }
        }
        Ok(())
    }

    // ---- tools ----------------------------------------------------------

    fn read_file(&self, args: &Value) -> Result<String, String> {
        let target = self.resolve(str_arg(args, "path")?)?;
        let meta = fs::metadata(&target.abs)
            .map_err(|_| format!("\"{}\" doesn't exist.", target.rel))?;
        if meta.is_dir() {
            return Err(format!(
                "\"{}\" is a folder. Use list_files to see what's in it.",
                target.rel
            ));
        }
        if meta.len() > MAX_OPEN_BYTES {
            return Err(format!("\"{}\" is too large to read.", target.rel));
        }
        let bytes =
            fs::read(&target.abs).map_err(|e| format!("Couldn't read \"{}\": {e}", target.rel))?;
        let text = text_of(&bytes)
            .ok_or_else(|| format!("\"{}\" isn't a text file, so it can't be read.", target.rel))?;

        let offset = opt_usize(args, "offset")?.unwrap_or(1).max(1);
        let limit = opt_usize(args, "limit")?.unwrap_or(usize::MAX);
        let total = text.lines().count();
        if total == 0 {
            return Ok(format!("\"{}\" is empty.", target.rel));
        }
        if offset > total {
            return Err(format!(
                "\"{}\" has only {total} lines; offset {offset} is past the end.",
                target.rel
            ));
        }
        let mut out = String::new();
        let mut next = offset;
        for (i, line) in text.lines().enumerate().skip(offset - 1).take(limit) {
            let numbered = format!("{:>6}\t{}\n", i + 1, line);
            if out.len() + numbered.len() > MAX_READ_BYTES {
                out.push_str(&format!(
                    "[Cut at {} KB. Call read_file again with offset {} to continue.]\n",
                    MAX_READ_BYTES / 1024,
                    i + 1
                ));
                return Ok(out);
            }
            out.push_str(&numbered);
            next = i + 2;
        }
        if next <= total {
            out.push_str(&format!(
                "[{} more lines. Call read_file again with offset {next} to continue.]\n",
                total + 1 - next
            ));
        }
        Ok(out)
    }

    fn write_file(&self, args: &Value) -> Result<FileToolOutput, String> {
        let target = self.resolve(str_arg(args, "path")?)?;
        self.check_writable(&target)?;
        let content = args
            .get("content")
            .and_then(Value::as_str)
            .ok_or("Missing required argument `content` (a string).")?;
        if content.len() > MAX_WRITE_BYTES {
            return Err(format!(
                "That file is too large to write in one go ({} bytes; the limit is {} MB).",
                content.len(),
                MAX_WRITE_BYTES / (1024 * 1024)
            ));
        }
        let existed = target.abs.is_file();
        write_atomic(&target.abs, content.as_bytes())
            .map_err(|e| format!("Couldn't write \"{}\": {e}", target.rel))?;
        Ok(FileToolOutput {
            text: format!(
                "{} {} ({} bytes).",
                if existed { "Replaced" } else { "Created" },
                target.rel,
                content.len()
            ),
            written: Some(target.rel),
        })
    }

    fn edit_file(&self, args: &Value) -> Result<FileToolOutput, String> {
        let target = self.resolve(str_arg(args, "path")?)?;
        self.check_writable(&target)?;
        let old = str_arg(args, "old_string")?;
        let new = args
            .get("new_string")
            .and_then(Value::as_str)
            .ok_or("Missing required argument `new_string` (a string).")?;
        let replace_all = args
            .get("replace_all")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if old.is_empty() {
            return Err(
                "old_string can't be empty. To create or replace a whole file use write_file."
                    .into(),
            );
        }
        if old == new {
            return Err("old_string and new_string are the same, so nothing would change.".into());
        }
        let bytes = fs::read(&target.abs).map_err(|_| {
            format!(
                "\"{}\" doesn't exist. Use write_file to create it.",
                target.rel
            )
        })?;
        let text = text_of(&bytes).ok_or_else(|| {
            format!("\"{}\" isn't a text file, so it can't be edited.", target.rel)
        })?;
        let count = text.matches(old).count();
        if count == 0 {
            return Err(format!(
                "old_string wasn't found in \"{}\". It must match the file exactly, including spaces and line breaks. Read the file again and copy the text exactly.",
                target.rel
            ));
        }
        if count > 1 && !replace_all {
            return Err(format!(
                "old_string appears {count} times in \"{}\". Include more of the surrounding text so it matches once, or set replace_all to true.",
                target.rel
            ));
        }
        let updated = if replace_all {
            text.replace(old, new)
        } else {
            text.replacen(old, new, 1)
        };
        write_atomic(&target.abs, updated.as_bytes())
            .map_err(|e| format!("Couldn't write \"{}\": {e}", target.rel))?;
        Ok(FileToolOutput {
            text: format!(
                "Edited {}: replaced {count} occurrence{} ({} bytes before, {} after).",
                target.rel,
                if count == 1 { "" } else { "s" },
                bytes.len(),
                updated.len()
            ),
            written: Some(target.rel),
        })
    }

    fn list_files(&self, args: &Value) -> Result<String, String> {
        let base = self.resolve(opt_str(args, "path").unwrap_or(""))?;
        let matcher = opt_str(args, "pattern")
            .filter(|p| !p.trim().is_empty())
            .map(GlobMatcher::new)
            .transpose()?;
        if !base.abs.is_dir() {
            return Err(format!(
                "\"{}\" isn't a folder in the project.",
                shown(&base.rel)
            ));
        }
        let mut found: Vec<String> = Vec::new();
        let mut truncated = false;
        walk(&base.abs, &mut |file, rel_to_base| {
            if matcher.as_ref().is_none_or(|m| m.matches(rel_to_base)) {
                if found.len() == MAX_LIST_ENTRIES {
                    truncated = true;
                    return false;
                }
                found.push(self.project_rel(file));
            }
            true
        });
        if found.is_empty() {
            return Ok("No files found.".into());
        }
        let mut out = found.join("\n");
        out.push('\n');
        if truncated {
            out.push_str(&format!(
                "[Showing the first {MAX_LIST_ENTRIES} files. Narrow it with path or pattern.]\n"
            ));
        }
        Ok(out)
    }

    fn search_files(&self, args: &Value) -> Result<String, String> {
        let pattern = str_arg(args, "pattern")?;
        let re = Regex::new(pattern)
            .map_err(|e| format!("That isn't a valid regular expression: {e}"))?;
        let base = self.resolve(opt_str(args, "path").unwrap_or(""))?;
        let matcher = opt_str(args, "glob")
            .filter(|p| !p.trim().is_empty())
            .map(GlobMatcher::new)
            .transpose()?;
        if !base.abs.exists() {
            return Err(format!("\"{}\" doesn't exist.", shown(&base.rel)));
        }
        let mut hits: Vec<String> = Vec::new();
        let mut truncated = false;
        let mut search = |file: &Path, rel_to_base: &str| -> bool {
            if matcher.as_ref().is_some_and(|m| !m.matches(rel_to_base)) {
                return true;
            }
            let Ok(meta) = fs::metadata(file) else {
                return true;
            };
            if meta.len() > MAX_SEARCH_FILE_BYTES {
                return true;
            }
            let Ok(bytes) = fs::read(file) else {
                return true;
            };
            let Some(text) = text_of(&bytes) else {
                return true;
            };
            let shown_path = self.project_rel(file);
            for (i, line) in text.lines().enumerate() {
                if re.is_match(line) {
                    if hits.len() == MAX_SEARCH_MATCHES {
                        truncated = true;
                        return false;
                    }
                    hits.push(format!(
                        "{shown_path}:{}: {}",
                        i + 1,
                        clip(line.trim_end())
                    ));
                }
            }
            true
        };
        if base.abs.is_file() {
            let name = base
                .abs
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            search(&base.abs, &name);
        } else {
            walk(&base.abs, &mut search);
        }
        if hits.is_empty() {
            return Ok("No matches.".into());
        }
        let mut out = hits.join("\n");
        out.push('\n');
        if truncated {
            out.push_str(&format!(
                "[Showing the first {MAX_SEARCH_MATCHES} matches. Narrow it with path, glob or a more specific pattern.]\n"
            ));
        }
        Ok(out)
    }

    fn project_rel(&self, abs: &Path) -> String {
        abs.strip_prefix(&self.root)
            .unwrap_or(abs)
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/")
    }
}

fn shown(rel: &str) -> &str {
    if rel.is_empty() { "." } else { rel }
}

/// Why writing to this project-relative path is refused, if it is.
fn protected(rel: &str) -> Option<&'static str> {
    // Compared case-insensitively: on macOS and Windows `.GIT` is `.git`.
    let lower = rel.to_lowercase();
    if lower.split('/').any(|part| part == ".git") {
        return Some("part of the project's git history");
    }
    if lower == "addons/infinabox" || lower.starts_with("addons/infinabox/") {
        return Some("InfinaBox's own game add-on");
    }
    if lower == ".ibproject/.ibx" {
        return Some("InfinaBox's project marker");
    }
    if lower == ".ibproject/chat" || lower.starts_with(".ibproject/chat/") {
        return Some("this chat's saved history");
    }
    None
}

fn str_arg<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Missing required argument `{key}` (a string)."))
}

fn opt_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

/// An optional whole-number argument; models sometimes send it as a float
/// or a string.
fn opt_usize(args: &Value, key: &str) -> Result<Option<usize>, String> {
    let Some(v) = args.get(key).filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let n = v
        .as_u64()
        .or_else(|| v.as_f64().filter(|f| *f >= 0.0).map(|f| f as u64))
        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()));
    n.map(|n| Some(n as usize))
        .ok_or_else(|| format!("`{key}` must be a whole number."))
}

/// The bytes as text, or `None` for a binary file.
fn text_of(bytes: &[u8]) -> Option<&str> {
    if bytes[..bytes.len().min(8000)].contains(&0) {
        return None;
    }
    std::str::from_utf8(bytes).ok()
}

fn clip(line: &str) -> String {
    if line.chars().count() <= MAX_MATCH_LINE_CHARS {
        return line.to_string();
    }
    let cut: String = line.chars().take(MAX_MATCH_LINE_CHARS).collect();
    format!("{cut}…")
}

/// Writes through a temporary file in the same folder and renames it over
/// the target, so a crash never leaves half a file. Creates parent folders.
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| std::io::Error::other("no parent folder"))?;
    fs::create_dir_all(dir)?;
    if path.is_dir() {
        return Err(std::io::Error::other("it is a folder"));
    }
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = dir.join(format!(".{name}.ibx-tmp-{}", std::process::id()));
    let result = (|| {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        if let Ok(meta) = fs::metadata(path) {
            let _ = fs::set_permissions(&tmp, meta.permissions());
        }
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Calls `visit(file, path relative to base)` for every regular file under
/// `base`, in name order, skipping hidden entries (which covers `.git` and
/// `.godot`) and symlinks; stops when `visit` returns false.
fn walk(base: &Path, visit: &mut dyn FnMut(&Path, &str) -> bool) {
    fn go(
        dir: &Path,
        rel: &str,
        depth: usize,
        visit: &mut dyn FnMut(&Path, &str) -> bool,
    ) -> bool {
        if depth > MAX_DEPTH {
            return true;
        }
        let Ok(read) = fs::read_dir(dir) else {
            return true;
        };
        let mut entries: Vec<_> = read.filter_map(Result::ok).collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let child_rel = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            if kind.is_dir() {
                if !go(&entry.path(), &child_rel, depth + 1, visit) {
                    return false;
                }
            } else if kind.is_file() && !visit(&entry.path(), &child_rel) {
                return false;
            }
        }
        true
    }
    go(base, "", 0, visit);
}

/// A glob (`*`, `**`, `?`, `{a,b}`) matched against a path relative to the
/// folder being searched — or, when the glob has no `/`, against the file
/// name alone.
struct GlobMatcher {
    re: Regex,
    name_only: bool,
}

impl GlobMatcher {
    fn new(glob: &str) -> Result<Self, String> {
        let glob = glob.trim().trim_start_matches("./");
        let name_only = !glob.contains('/');
        let mut re = String::from("^");
        let chars: Vec<char> = glob.chars().collect();
        let mut i = 0;
        let mut in_braces = false;
        while i < chars.len() {
            match chars[i] {
                '*' if chars.get(i + 1) == Some(&'*') => {
                    if chars.get(i + 2) == Some(&'/') {
                        re.push_str("(?:.*/)?");
                        i += 2;
                    } else {
                        re.push_str(".*");
                        i += 1;
                    }
                }
                '*' => re.push_str("[^/]*"),
                '?' => re.push_str("[^/]"),
                '{' => {
                    in_braces = true;
                    re.push_str("(?:");
                }
                '}' if in_braces => {
                    in_braces = false;
                    re.push(')');
                }
                ',' if in_braces => re.push('|'),
                c => re.push_str(&regex::escape(&c.to_string())),
            }
            i += 1;
        }
        if in_braces {
            return Err("The pattern has an unclosed {.".into());
        }
        re.push('$');
        let re = Regex::new(&re).map_err(|e| format!("That isn't a usable pattern: {e}"))?;
        Ok(Self { re, name_only })
    }

    fn matches(&self, rel: &str) -> bool {
        if self.name_only {
            self.re.is_match(rel.rsplit('/').next().unwrap_or(rel))
        } else {
            self.re.is_match(rel)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> (tempfile::TempDir, FileTools) {
        let dir = tempfile::tempdir().unwrap();
        for (path, body) in [
            (
                "scripts/player.gd",
                "extends Node\nvar speed = 3\nvar speed2 = 4\n",
            ),
            ("scenes/main.tscn", "[gd_scene]\n"),
            ("scenes/ui/hud.tscn", "[gd_scene]\n"),
            (".hidden/secret.txt", "hush"),
            (".git/config", "[core]"),
            (".godot/cache.txt", "x"),
            ("addons/infinabox/plugin.cfg", "x"),
            (".ibproject/.ibx", "{}"),
            (".ibproject/chat/t.jsonl", "{}"),
        ] {
            let p = dir.path().join(path);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, body).unwrap();
        }
        fs::write(dir.path().join("logo.bin"), [0u8, 1, 2, 255]).unwrap();
        let tools = FileTools::new(dir.path()).unwrap();
        (dir, tools)
    }

    fn call(t: &FileTools, name: &str, args: Value) -> Result<FileToolOutput, String> {
        t.call(name, &args)
    }

    #[test]
    fn read_numbers_lines_and_windows() {
        let (_d, t) = project();
        let all = call(&t, "read_file", json!({"path": "scripts/player.gd"})).unwrap();
        assert_eq!(
            all.text,
            "     1\textends Node\n     2\tvar speed = 3\n     3\tvar speed2 = 4\n"
        );
        assert_eq!(all.written, None);
        let part = call(
            &t,
            "read_file",
            json!({"path": "./scripts/player.gd", "offset": 2, "limit": 1}),
        )
        .unwrap();
        assert!(part.text.starts_with("     2\tvar speed = 3\n"));
        assert!(part.text.contains("offset 3"));
        assert!(call(&t, "read_file", json!({"path": "scripts/player.gd", "offset": 9})).is_err());
    }

    #[test]
    fn read_refuses_binary_folders_and_missing() {
        let (_d, t) = project();
        assert!(
            call(&t, "read_file", json!({"path": "logo.bin"}))
                .unwrap_err()
                .contains("isn't a text file")
        );
        assert!(
            call(&t, "read_file", json!({"path": "scenes"}))
                .unwrap_err()
                .contains("folder")
        );
        assert!(
            call(&t, "read_file", json!({"path": "nope.gd"}))
                .unwrap_err()
                .contains("doesn't exist")
        );
        assert!(
            call(&t, "read_file", json!({}))
                .unwrap_err()
                .contains("path")
        );
    }

    #[test]
    fn read_is_capped_with_a_way_to_continue() {
        let (d, t) = project();
        let big: String = (0..40_000).map(|i| format!("line number {i}\n")).collect();
        fs::write(d.path().join("big.txt"), &big).unwrap();
        let out = call(&t, "read_file", json!({"path": "big.txt"}))
            .unwrap()
            .text;
        assert!(out.len() <= MAX_READ_BYTES + 200);
        assert!(out.contains("Call read_file again with offset"));
    }

    #[test]
    fn write_creates_parents_replaces_and_reports_bytes() {
        let (d, t) = project();
        let made = call(
            &t,
            "write_file",
            json!({"path": "levels/one/level.gd", "content": "abc"}),
        )
        .unwrap();
        assert_eq!(made.text, "Created levels/one/level.gd (3 bytes).");
        assert_eq!(made.written.as_deref(), Some("levels/one/level.gd"));
        let again = call(
            &t,
            "write_file",
            json!({"path": "levels/one/level.gd", "content": "abcdef"}),
        )
        .unwrap();
        assert!(
            again
                .text
                .starts_with("Replaced levels/one/level.gd (6 bytes)")
        );
        assert_eq!(
            fs::read_to_string(d.path().join("levels/one/level.gd")).unwrap(),
            "abcdef"
        );
        // No temp files left behind.
        let names: Vec<_> = fs::read_dir(d.path().join("levels/one"))
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names.len(), 1);
    }

    #[test]
    fn edit_needs_a_unique_exact_match() {
        let (d, t) = project();
        let p = "scripts/player.gd";
        let e = call(
            &t,
            "edit_file",
            json!({"path": p, "old_string": "speed", "new_string": "pace"}),
        )
        .unwrap_err();
        assert!(e.contains("2 times"), "{e}");
        let e = call(
            &t,
            "edit_file",
            json!({"path": p, "old_string": "jump", "new_string": "x"}),
        )
        .unwrap_err();
        assert!(e.contains("wasn't found"));
        let ok = call(
            &t,
            "edit_file",
            json!({"path": p, "old_string": "var speed = 3", "new_string": "var speed = 9"}),
        )
        .unwrap();
        assert_eq!(ok.written.as_deref(), Some(p));
        assert!(ok.text.contains("1 occurrence"));
        let all = call(
            &t,
            "edit_file",
            json!({"path": p, "old_string": "speed", "new_string": "pace", "replace_all": true}),
        )
        .unwrap();
        assert!(all.text.contains("2 occurrences"));
        assert_eq!(
            fs::read_to_string(d.path().join(p)).unwrap(),
            "extends Node\nvar pace = 9\nvar pace2 = 4\n"
        );
        assert!(
            call(
                &t,
                "edit_file",
                json!({"path": p, "old_string": "", "new_string": "x"})
            )
            .is_err()
        );
        assert!(
            call(
                &t,
                "edit_file",
                json!({"path": "missing.gd", "old_string": "a", "new_string": "b"})
            )
            .unwrap_err()
            .contains("write_file")
        );
    }

    #[test]
    fn writes_to_protected_places_and_outside_are_refused() {
        let (d, t) = project();
        for path in [
            ".git/config",
            ".git/hooks/pre-commit",
            ".GIT/config",
            "addons/infinabox/plugin.cfg",
            "addons/infinabox/new.gd",
            ".ibproject/.ibx",
            ".ibproject/chat/t.jsonl",
            ".ibproject/chat/new.jsonl",
            "scenes/../.git/config",
            "../outside.txt",
            "scenes/../../outside.txt",
            "/etc/passwd",
            "",
        ] {
            let w = call(&t, "write_file", json!({"path": path, "content": "x"}));
            assert!(w.is_err(), "write to {path:?} should fail");
            let e = call(
                &t,
                "edit_file",
                json!({"path": path, "old_string": "a", "new_string": "b"}),
            );
            assert!(e.is_err(), "edit of {path:?} should fail");
        }
        assert_eq!(
            fs::read_to_string(d.path().join(".git/config")).unwrap(),
            "[core]"
        );
        assert!(!d.path().parent().unwrap().join("outside.txt").exists());
        // The rest of .ibproject is fair game (Context cards live there).
        assert!(
            call(
                &t,
                "write_file",
                json!({"path": ".ibproject/context/x.md", "content": "x"})
            )
            .is_ok()
        );
        // Absolute paths inside the project are fine.
        let abs = d.path().join("abs.txt");
        assert_eq!(
            call(
                &t,
                "write_file",
                json!({"path": abs.to_str().unwrap(), "content": "x"})
            )
            .unwrap()
            .written
            .as_deref(),
            Some("abs.txt")
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_cannot_lead_out_or_into_protected_places() {
        use std::os::unix::fs::symlink;
        let (d, t) = project();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("target.txt"), "secret").unwrap();
        symlink(outside.path(), d.path().join("link")).unwrap();
        symlink(outside.path().join("target.txt"), d.path().join("filelink")).unwrap();
        symlink(outside.path().join("nothing"), d.path().join("dangling")).unwrap();
        symlink(d.path().join(".git"), d.path().join("gitlink")).unwrap();
        assert!(
            call(&t, "read_file", json!({"path": "link/target.txt"}))
                .unwrap_err()
                .contains("outside")
        );
        assert!(
            call(
                &t,
                "write_file",
                json!({"path": "link/new.txt", "content": "x"})
            )
            .unwrap_err()
            .contains("outside")
        );
        assert!(
            call(
                &t,
                "write_file",
                json!({"path": "filelink", "content": "x"})
            )
            .unwrap_err()
            .contains("outside")
        );
        assert!(
            call(
                &t,
                "write_file",
                json!({"path": "dangling", "content": "x"})
            )
            .is_err()
        );
        assert!(
            call(
                &t,
                "write_file",
                json!({"path": "gitlink/config", "content": "x"})
            )
            .unwrap_err()
            .contains("git")
        );
        assert!(call(&t, "list_files", json!({"path": "link"})).is_err());
        assert_eq!(
            fs::read_to_string(outside.path().join("target.txt")).unwrap(),
            "secret"
        );
        // Searching doesn't follow links out either.
        assert_eq!(
            call(&t, "search_files", json!({"pattern": "secret"}))
                .unwrap()
                .text,
            "No matches."
        );
    }

    #[test]
    fn list_skips_hidden_and_filters() {
        let (_d, t) = project();
        let all = call(&t, "list_files", json!({})).unwrap().text;
        assert_eq!(
            all,
            "addons/infinabox/plugin.cfg\nlogo.bin\nscenes/main.tscn\nscenes/ui/hud.tscn\nscripts/player.gd\n"
        );
        let tscn = call(&t, "list_files", json!({"pattern": "*.tscn"}))
            .unwrap()
            .text;
        assert_eq!(tscn, "scenes/main.tscn\nscenes/ui/hud.tscn\n");
        let direct = call(
            &t,
            "list_files",
            json!({"path": "scenes", "pattern": "ui/*.tscn"}),
        )
        .unwrap()
        .text;
        assert_eq!(direct, "scenes/ui/hud.tscn\n");
        let deep = call(&t, "list_files", json!({"pattern": "scenes/**/*.tscn"}))
            .unwrap()
            .text;
        assert_eq!(deep, "scenes/main.tscn\nscenes/ui/hud.tscn\n");
        let braces = call(&t, "list_files", json!({"pattern": "*.{gd,bin}"}))
            .unwrap()
            .text;
        assert_eq!(braces, "logo.bin\nscripts/player.gd\n");
        assert_eq!(
            call(&t, "list_files", json!({"pattern": "*.png"}))
                .unwrap()
                .text,
            "No files found."
        );
        assert!(call(&t, "list_files", json!({"path": "logo.bin"})).is_err());
    }

    #[test]
    fn list_stops_at_the_cap() {
        let (d, t) = project();
        for i in 0..(MAX_LIST_ENTRIES + 20) {
            fs::write(d.path().join(format!("f{i:04}.txt")), "x").unwrap();
        }
        let out = call(&t, "list_files", json!({"pattern": "f*.txt"}))
            .unwrap()
            .text;
        assert_eq!(
            out.lines().filter(|l| l.ends_with(".txt")).count(),
            MAX_LIST_ENTRIES
        );
        assert!(out.contains("Showing the first 500"));
    }

    #[test]
    fn search_finds_lines_skips_binary_and_caps() {
        let (d, t) = project();
        let hits = call(&t, "search_files", json!({"pattern": r"var speed\d"}))
            .unwrap()
            .text;
        assert_eq!(hits, "scripts/player.gd:3: var speed2 = 4\n");
        let globbed = call(
            &t,
            "search_files",
            json!({"pattern": "gd_scene", "glob": "*.tscn", "path": "scenes"}),
        )
        .unwrap()
        .text;
        assert_eq!(globbed.lines().count(), 2);
        assert_eq!(
            call(
                &t,
                "search_files",
                json!({"pattern": "gd_scene", "path": "scenes/main.tscn"})
            )
            .unwrap()
            .text,
            "scenes/main.tscn:1: [gd_scene]\n"
        );
        assert_eq!(
            call(&t, "search_files", json!({"pattern": "core"}))
                .unwrap()
                .text,
            "No matches."
        );
        assert!(
            call(&t, "search_files", json!({"pattern": "("}))
                .unwrap_err()
                .contains("regular expression")
        );
        fs::write(
            d.path().join("many.txt"),
            "hit\n".repeat(MAX_SEARCH_MATCHES + 50),
        )
        .unwrap();
        let many = call(&t, "search_files", json!({"pattern": "hit"}))
            .unwrap()
            .text;
        assert_eq!(
            many.lines().filter(|l| l.contains("many.txt:")).count(),
            MAX_SEARCH_MATCHES
        );
        assert!(many.contains("Showing the first 200"));
    }

    #[test]
    fn event_names_and_summaries_mirror_the_cli_runtimes() {
        assert_eq!(event_name("edit_file", false), "Edit");
        assert_eq!(event_name("write_file", false), "Write");
        assert_eq!(event_name("read_file", false), "Read");
        assert_eq!(event_name("list_files", false), "Glob");
        assert_eq!(event_name("search_files", false), "Grep");
        assert_eq!(event_name("run_game", true), "mcp__infinabox__run_game");
        assert_eq!(
            summary("edit_file", &json!({"path": "./player.gd"})),
            "Editing player.gd"
        );
        assert_eq!(
            summary("read_file", &json!({"path": "/home/me/g/main.tscn"})),
            "Reading main.tscn"
        );
        assert_eq!(summary("write_file", &json!({})), "Writing a file");
        assert_eq!(summary("run_game", &json!({})), "Running the game");
        assert_eq!(summary("mystery_tool", &json!({})), "Using mystery tool");
    }

    #[test]
    fn specs_cover_every_tool_with_object_schemas() {
        let s = specs();
        let names: Vec<_> = s.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, FILE_TOOL_NAMES);
        for t in &s {
            assert_eq!(t.parameters["type"], "object");
            assert!(!t.description.is_empty());
        }
    }
}
