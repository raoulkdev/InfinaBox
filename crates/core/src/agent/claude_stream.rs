//! Turns Claude Code's `--output-format stream-json --verbose` output into
//! `AgentEvent`s. No I/O here: `ClaudeStream` is fed one stdout line at a
//! time and returns the events that line produced, so it's tested directly
//! against the real recordings in `crates/core/tests/fixtures/claude/`.
//!
//! It keeps a little state across lines, because some events need it: a
//! tool result only says which tool-use id it answers, and the list of
//! changed files is only known once each edit's result says it worked.
//!
//! What the recordings show (Claude Code 2.1.283), and how it maps:
//! - `system`/`init` → `SessionStarted` (`session_id`, `model`).
//! - `assistant` messages hold content blocks: `text` → `AssistantText`,
//!   `tool_use` → `ToolUse`, `thinking` is ignored (not user-facing text).
//! - Tool results come back as `user` messages holding `tool_result` blocks
//!   (`tool_use_id`, optional `is_error`) → `ToolResult`.
//! - `result` ends the turn → `FilesChanged` (if any), `Error` (if
//!   `is_error`), then `TurnCompleted` with only the figures it reported.
//! - Everything else (`rate_limit_event`, `system`/`permission_denied`,
//!   `system`/`thinking_tokens`, types added in later CLI versions) is
//!   ignored — except that `system`/`api_retry`'s error kind is remembered,
//!   to classify the turn's error if it ends in one.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use serde_json::Value;

use super::types::{AgentErrorKind, AgentEvent, Usage};

/// Claude Code's own tool for loading deferred tool definitions (seen before
/// the MCP call in the `d_mcp_tool` recording). It's plumbing, not work on
/// the user's project, so it and its result are left out of the events.
const HIDDEN_TOOLS: &[&str] = &["ToolSearch"];

/// The InfinaBox MCP tool that writes files (Context cards, relative to
/// this folder). Kept in sync with `crates/mcp-server/src/server.rs`.
const WRITE_CONTEXT_CARD: &str = "mcp__infinabox__write_context_card";
const CONTEXT_DIR: &str = ".ibproject/context";

/// Longest one-line summary kept from a failed tool's real error text.
const MAX_RESULT_SUMMARY: usize = 200;
/// Longest error message kept from stderr when the CLI dies without a
/// `result` (the end of stderr, where the actual failure usually is).
const MAX_STDERR_MESSAGE: usize = 2000;

/// The message used when the user stops a turn before it finished.
pub const STOPPED_MESSAGE: &str = "Stopped.";

/// How the CLI process ended, for turns that ended without a `result`.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamEnd {
    /// The user cancelled the turn (we killed the process).
    Cancelled,
    /// The process exited on its own. `code` is `None` when a signal
    /// ended it.
    Exited { code: Option<i32>, stderr: String },
}

struct PendingTool {
    hidden: bool,
    /// Project-relative path this tool writes, if it's a file-writing tool
    /// with a path inside the project. Counted as changed only once its
    /// result comes back without an error.
    writes: Option<String>,
}

pub struct ClaudeStream {
    /// The project directory, as given and (if different) canonicalized:
    /// the CLI reports absolute paths under whichever form its cwd has.
    roots: Vec<PathBuf>,
    tools: HashMap<String, PendingTool>,
    files_changed: Vec<String>,
    saw_result: bool,
    /// The error kind from the last `api_retry`/assistant error seen, used
    /// only if the turn ends in an error the message text doesn't classify.
    error_hint: Option<AgentErrorKind>,
}

impl ClaudeStream {
    pub fn new(project_roots: Vec<PathBuf>) -> Self {
        Self {
            roots: project_roots,
            tools: HashMap::new(),
            files_changed: Vec::new(),
            saw_result: false,
            error_hint: None,
        }
    }

    /// True once the CLI's final `result` message has been parsed.
    pub fn saw_result(&self) -> bool {
        self.saw_result
    }

    /// The events one line of stream-json output produces. Blank lines,
    /// non-JSON lines, and unknown message types produce none.
    pub fn parse_line(&mut self, line: &str) -> Vec<AgentEvent> {
        let line = line.trim();
        if line.is_empty() {
            return Vec::new();
        }
        let Ok(msg) = serde_json::from_str::<Value>(line) else {
            return Vec::new();
        };
        match msg.get("type").and_then(Value::as_str) {
            Some("system") => self.on_system(&msg),
            Some("assistant") => self.on_assistant(&msg),
            Some("user") => self.on_user(&msg),
            Some("result") => self.on_result(&msg),
            _ => Vec::new(),
        }
    }

    /// The events that close a turn the CLI never sent a `result` for, so
    /// the caller can always finish it. Returns nothing if it did.
    pub fn finish(&mut self, end: StreamEnd) -> Vec<AgentEvent> {
        if self.saw_result {
            return Vec::new();
        }
        self.saw_result = true;
        let mut events = self.take_files_changed();
        let (kind, message) = match end {
            StreamEnd::Cancelled => (AgentErrorKind::Other, STOPPED_MESSAGE.to_string()),
            StreamEnd::Exited { code, stderr } => {
                let text = tail(stderr.trim(), MAX_STDERR_MESSAGE);
                let kind = match classify_error(text, None) {
                    AgentErrorKind::Other => {
                        self.error_hint.unwrap_or(AgentErrorKind::ProcessFailed)
                    }
                    kind => kind,
                };
                let message = if !text.is_empty() {
                    text.to_string()
                } else {
                    match code {
                        Some(code) => format!(
                            "Claude Code stopped before finishing the turn (exit code {code})."
                        ),
                        None => "Claude Code was stopped before finishing the turn.".to_string(),
                    }
                };
                (kind, message)
            }
        };
        events.push(AgentEvent::Error { kind, message });
        events.push(AgentEvent::TurnCompleted {
            is_error: true,
            duration_ms: None,
            usage: None,
        });
        events
    }

    fn on_system(&mut self, msg: &Value) -> Vec<AgentEvent> {
        match msg.get("subtype").and_then(Value::as_str) {
            Some("init") => {
                let Some(id) = msg.get("session_id").and_then(Value::as_str) else {
                    return Vec::new();
                };
                vec![AgentEvent::SessionStarted {
                    provider_session_id: id.to_string(),
                    model: msg.get("model").and_then(Value::as_str).map(str::to_string),
                }]
            }
            Some("api_retry") => {
                let hint = hint_from(
                    msg.get("error").and_then(Value::as_str),
                    msg.get("error_status").and_then(Value::as_u64),
                );
                if hint.is_some() {
                    self.error_hint = hint;
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn on_assistant(&mut self, msg: &Value) -> Vec<AgentEvent> {
        // An assistant message carrying an `error` is the CLI relaying an
        // API failure as text; the `result` that follows reports the same
        // failure as the turn's error, so the text isn't shown twice (see
        // the real recording in `agent/testdata/auth_failure_tail.jsonl`:
        // `"error":"authentication_failed","is_api_error_message":true`).
        let api_error = msg.get("error").and_then(Value::as_str);
        if let Some(hint) = hint_from(api_error, None) {
            self.error_hint = Some(hint);
        }
        let mut events = Vec::new();
        for block in content_blocks(msg) {
            match block.get("type").and_then(Value::as_str) {
                Some("text") if api_error.is_none() => {
                    let text = block.get("text").and_then(Value::as_str).unwrap_or("");
                    if !text.trim().is_empty() {
                        events.push(AgentEvent::AssistantText {
                            text: text.to_string(),
                        });
                    }
                }
                Some("tool_use") => events.extend(self.on_tool_use(block)),
                _ => {}
            }
        }
        events
    }

    fn on_tool_use(&mut self, block: &Value) -> Option<AgentEvent> {
        let id = block.get("id").and_then(Value::as_str)?.to_string();
        let name = block
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let empty = Value::Null;
        let input = block.get("input").unwrap_or(&empty);
        let hidden = HIDDEN_TOOLS.contains(&name.as_str());
        let writes = written_path(&name, input).and_then(|p| self.project_relative(&p));
        self.tools
            .insert(id.clone(), PendingTool { hidden, writes });
        if hidden {
            return None;
        }
        let summary = self.summarize(&name, input);
        Some(AgentEvent::ToolUse { id, name, summary })
    }

    fn on_user(&mut self, msg: &Value) -> Vec<AgentEvent> {
        let mut events = Vec::new();
        for block in content_blocks(msg) {
            if block.get("type").and_then(Value::as_str) != Some("tool_result") {
                continue;
            }
            let Some(id) = block.get("tool_use_id").and_then(Value::as_str) else {
                continue;
            };
            let ok = !block
                .get("is_error")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let pending = self.tools.remove(id);
            if let Some(PendingTool {
                writes: Some(path), ..
            }) = &pending
                && ok
                && !self.files_changed.contains(path)
            {
                self.files_changed.push(path.clone());
            }
            if pending.as_ref().is_some_and(|p| p.hidden) {
                continue;
            }
            let summary = if ok {
                "Done".to_string()
            } else {
                failure_summary(block.get("content"))
            };
            events.push(AgentEvent::ToolResult {
                id: id.to_string(),
                ok,
                summary,
            });
        }
        events
    }

    fn on_result(&mut self, msg: &Value) -> Vec<AgentEvent> {
        self.saw_result = true;
        let is_error = msg
            .get("is_error")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mut events = self.take_files_changed();
        if is_error {
            let message = result_error_message(msg);
            let status = msg.get("api_error_status").and_then(Value::as_u64);
            let kind = match classify_error(&message, status) {
                AgentErrorKind::Other => self.error_hint.unwrap_or(AgentErrorKind::Other),
                kind => kind,
            };
            events.push(AgentEvent::Error { kind, message });
        }
        let usage = msg.get("usage").filter(|u| u.is_object()).map(|u| Usage {
            input_tokens: u.get("input_tokens").and_then(Value::as_u64),
            output_tokens: u.get("output_tokens").and_then(Value::as_u64),
        });
        events.push(AgentEvent::TurnCompleted {
            is_error,
            duration_ms: msg.get("duration_ms").and_then(Value::as_u64),
            usage,
        });
        events
    }

    fn take_files_changed(&mut self) -> Vec<AgentEvent> {
        if self.files_changed.is_empty() {
            return Vec::new();
        }
        vec![AgentEvent::FilesChanged {
            paths: std::mem::take(&mut self.files_changed),
        }]
    }

    /// `raw` as a normalized, `/`-separated path relative to the project, or
    /// `None` if it's outside the project.
    fn project_relative(&self, raw: &str) -> Option<String> {
        let path = Path::new(raw);
        let rel = if path.is_absolute() {
            self.roots
                .iter()
                .find_map(|root| path.strip_prefix(root).ok())?
        } else {
            path
        };
        let mut parts: Vec<String> = Vec::new();
        for component in rel.components() {
            match component {
                Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
                Component::CurDir => {}
                Component::ParentDir => {
                    parts.pop()?;
                }
                Component::RootDir | Component::Prefix(_) => return None,
            }
        }
        (!parts.is_empty()).then(|| parts.join("/"))
    }

    /// How a path is shown to the user: project-relative when it's inside
    /// the project, otherwise just its file name (never a long absolute
    /// path from someone's home directory).
    fn display_path(&self, raw: &str) -> String {
        self.project_relative(raw).unwrap_or_else(|| {
            Path::new(raw)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| raw.to_string())
        })
    }

    /// A short plain-language line for one tool use. Never the raw input:
    /// at most a file name, or the CLI's own one-line description of a
    /// shell command.
    fn summarize(&self, name: &str, input: &Value) -> String {
        let field = |key: &str| input.get(key).and_then(Value::as_str);
        let on_file = |verb: &str, key: &str, fallback: &str| match field(key) {
            Some(p) if !p.is_empty() => format!("{verb} {}", self.display_path(p)),
            _ => fallback.to_string(),
        };
        if let Some(tool) = name.strip_prefix("mcp__infinabox__") {
            let card = || field("path").map(|p| p.trim_end_matches(".md").to_string());
            return match tool {
                "run_game" => "Running the game".to_string(),
                "stop_game" => "Stopping the game".to_string(),
                "get_game_status" => "Checking whether the game is running".to_string(),
                "get_game_errors" => "Checking the game for errors".to_string(),
                "get_game_output" => "Reading the game's output".to_string(),
                "list_context_cards" => "Looking through the Context cards".to_string(),
                "read_context_card" => match card() {
                    Some(c) => format!("Reading the {c} Context card"),
                    None => "Reading a Context card".to_string(),
                },
                "search_context" => "Searching the Context cards".to_string(),
                "write_context_card" => match card() {
                    Some(c) => format!("Updating the {c} Context card"),
                    None => "Updating a Context card".to_string(),
                },
                "list_snapshots" => "Looking at the project's history".to_string(),
                other => format!("Using {}", other.replace('_', " ")),
            };
        }
        if let Some(rest) = name.strip_prefix("mcp__") {
            let tool = rest.split_once("__").map_or(rest, |(_, t)| t);
            return format!("Using {}", tool.replace('_', " "));
        }
        match name {
            "Read" => on_file("Reading", "file_path", "Reading a file"),
            "Edit" | "MultiEdit" => on_file("Editing", "file_path", "Editing a file"),
            "Write" => on_file("Writing", "file_path", "Writing a file"),
            "NotebookEdit" => on_file("Editing", "notebook_path", "Editing a notebook"),
            "Glob" | "LS" => "Looking for files".to_string(),
            "Grep" => "Searching the project".to_string(),
            "Bash" => match field("description") {
                Some(d) if !d.trim().is_empty() => first_line(d, 80),
                _ => "Running a command".to_string(),
            },
            "WebFetch" | "WebSearch" => "Looking things up online".to_string(),
            "TodoWrite" => "Planning the steps".to_string(),
            "Task" | "Agent" => "Asking a helper".to_string(),
            other => format!("Using {other}"),
        }
    }
}

/// The path a file-writing tool use writes, as the tool received it.
fn written_path(name: &str, input: &Value) -> Option<String> {
    let field = |key: &str| input.get(key).and_then(Value::as_str).map(str::to_string);
    match name {
        "Edit" | "MultiEdit" | "Write" => field("file_path"),
        "NotebookEdit" => field("notebook_path"),
        WRITE_CONTEXT_CARD => field("path").map(|p| format!("{CONTEXT_DIR}/{p}")),
        _ => None,
    }
}

fn content_blocks(msg: &Value) -> &[Value] {
    msg.get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// The first line of a failed tool's real error text, shortened.
fn failure_summary(content: Option<&Value>) -> String {
    let text = match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|b| b.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    };
    let text = text
        .replace("<tool_use_error>", "")
        .replace("</tool_use_error>", "");
    let line = first_line(&text, MAX_RESULT_SUMMARY);
    if line.is_empty() {
        "Failed".to_string()
    } else {
        line
    }
}

/// The real error text of a failed `result`: its `errors` list (e.g. the
/// bad-resume case), else its `result` text.
fn result_error_message(msg: &Value) -> String {
    let errors: Vec<&str> = msg
        .get("errors")
        .and_then(Value::as_array)
        .map(|e| e.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if !errors.is_empty() {
        return errors.join("\n");
    }
    match msg.get("result").and_then(Value::as_str) {
        Some(text) if !text.trim().is_empty() => text.trim().to_string(),
        _ => match msg.get("subtype").and_then(Value::as_str) {
            Some(subtype) => format!("Claude Code reported an error ({subtype})."),
            None => "Claude Code reported an error.".to_string(),
        },
    }
}

/// Classifies an error from its real message text (and HTTP status, when
/// the CLI reported one). Deliberately conservative: anything not clearly
/// a sign-in or rate-limit problem is `Other`, shown with its real text.
pub fn classify_error(message: &str, http_status: Option<u64>) -> AgentErrorKind {
    if let Some(kind) = hint_from(None, http_status) {
        return kind;
    }
    let lower = message.to_lowercase();
    const AUTH: &[&str] = &[
        "not logged in",
        "please run /login",
        "invalid api key",
        "authentication_failed",
        "authentication failed",
        "failed to authenticate",
        "oauth token has expired",
    ];
    const RATE: &[&str] = &["rate limit", "rate_limit", "usage limit", "hit your limit"];
    if AUTH.iter().any(|p| lower.contains(p)) {
        AgentErrorKind::NotAuthenticated
    } else if RATE.iter().any(|p| lower.contains(p)) {
        AgentErrorKind::RateLimited
    } else {
        AgentErrorKind::Other
    }
}

/// An error kind from the CLI's structured error fields (`api_retry`'s
/// `error`/`error_status`, a result's `api_error_status`).
fn hint_from(error: Option<&str>, status: Option<u64>) -> Option<AgentErrorKind> {
    match (error, status) {
        (Some("authentication_failed"), _) | (_, Some(401)) => {
            Some(AgentErrorKind::NotAuthenticated)
        }
        (Some("rate_limit"), _) | (_, Some(429)) => Some(AgentErrorKind::RateLimited),
        _ => None,
    }
}

fn first_line(text: &str, max_chars: usize) -> String {
    let line = text.trim().lines().next().unwrap_or("").trim();
    if line.chars().count() <= max_chars {
        line.to_string()
    } else {
        let cut: String = line.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{}…", cut.trim_end())
    }
}

/// The last `max_bytes` (or fewer, on a char boundary) of `text`.
fn tail(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut start = text.len() - max_bytes;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    &text[start..]
}

#[cfg(test)]
mod tests;
