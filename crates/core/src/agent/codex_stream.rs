//! Turns `codex exec --json` output into `AgentEvent`s. No I/O here:
//! `CodexStream` is fed one stdout line at a time and returns the events
//! that line produced, so it's tested directly against the real recordings
//! in `crates/core/tests/fixtures/codex/`.
//!
//! What the recordings show (Codex CLI 0.157.1), and how it maps:
//! - `thread.started` → `SessionStarted` (`thread_id`, the id `codex exec
//!   resume` takes; the stream doesn't name the model).
//! - `item.started` / `item.completed` carry one `item` each, by `type`:
//!   - `agent_message` (completed) → `AssistantText`.
//!   - `command_execution` (Codex's shell), `file_change` (its own
//!     `apply_patch` edits, with absolute `changes[].path`s),
//!     `mcp_tool_call` (`server`, `tool`, `arguments`, `result`/`error`),
//!     `web_search` → `ToolUse` when first seen, `ToolResult` on completion
//!     (`status` is `completed`, or `failed`/`declined`).
//!   - A completed, successful `file_change` is what `FilesChanged` is built
//!     from — plus the InfinaBox `write_context_card` tool, as for Claude.
//!   - The `infinabox` server's `propose_plan` call → `PlanProposed` once it
//!     completes successfully (the server accepted it), with nothing else
//!     shown; malformed input is shown as an ordinary tool use.
//!   - `reasoning`, `todo_list` and `error` items (non-fatal warnings, e.g.
//!     "Falling back from WebSockets to HTTPS transport") are ignored.
//! - Top-level `error` lines are retry notices ("Reconnecting... 2/5 (...)")
//!   and the final error; the last one is kept as a fallback message.
//! - `turn.completed` (`usage`) / `turn.failed` (`error.message`) end the
//!   turn → `FilesChanged` (if any), `Error` (if failed), `TurnCompleted`.
//!   Codex reports no duration, so `duration_ms` is always `None`.
//! - Errors before a thread exists (e.g. a resume id Codex doesn't know)
//!   print no JSON at all: just `Error: <message>` and a backtrace on stderr
//!   and exit code 1 — `finish` picks that line out.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use serde_json::Value;

use super::claude_stream::STOPPED_MESSAGE;
use super::prompt::PROPOSE_PLAN_TOOL;
use super::types::{AgentErrorKind, AgentEvent, Usage};

/// The name the InfinaBox MCP server is registered under (see `codex.rs`);
/// its tools are reported as `mcp_tool_call`s with this `server`.
pub const MCP_SERVER_NAME: &str = "infinabox";

/// What `codex exec resume <id>` prints on stderr when it has no
/// conversation with that id (see the `j_bad_resume` recording:
/// `Error: thread/resume: thread/resume failed: no rollout found for thread
/// id <id> (code -32600)`). The app retries such a turn fresh.
pub const BAD_RESUME_MARKER: &str = "no rollout found for thread id";

/// The InfinaBox MCP tool that writes files (Context cards, relative to
/// this folder). Kept in sync with `crates/mcp-server/src/server.rs`.
const WRITE_CONTEXT_CARD: &str = "write_context_card";
const CONTEXT_DIR: &str = ".ibproject/context";

/// Longest one-line summary kept from a failed tool's real error text.
const MAX_RESULT_SUMMARY: usize = 200;
/// Longest error message kept from stderr when the CLI dies without ending
/// the turn.
const MAX_STDERR_MESSAGE: usize = 2000;

/// How the CLI process ended, for turns that ended without a
/// `turn.completed`/`turn.failed`.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamEnd {
    /// The user cancelled the turn (we stopped the process).
    Cancelled,
    /// The process exited on its own. `code` is `None` when a signal
    /// ended it.
    Exited { code: Option<i32>, stderr: String },
}

/// An item already shown as started.
struct SeenItem {
    /// Shown as nothing (a `propose_plan` call waiting for its result).
    hidden: bool,
}

pub struct CodexStream {
    /// The project directory, as given and (if different) canonicalized:
    /// the CLI reports absolute paths under whichever form its cwd has.
    roots: Vec<PathBuf>,
    items: HashMap<String, SeenItem>,
    files_changed: Vec<String>,
    saw_end: bool,
    /// The last top-level `error` message, used only if the turn ends
    /// without a `turn.failed` saying why.
    last_error: Option<String>,
}

impl CodexStream {
    pub fn new(project_roots: Vec<PathBuf>) -> Self {
        Self {
            roots: project_roots,
            items: HashMap::new(),
            files_changed: Vec::new(),
            saw_end: false,
            last_error: None,
        }
    }

    /// True once the turn's `turn.completed`/`turn.failed` has been parsed.
    pub fn saw_end(&self) -> bool {
        self.saw_end
    }

    /// The events one line of `--json` output produces. Blank lines,
    /// non-JSON lines, and unknown event types produce none, and so does
    /// anything after the turn ended (so `TurnCompleted` always stays last).
    pub fn parse_line(&mut self, line: &str) -> Vec<AgentEvent> {
        let line = line.trim();
        if line.is_empty() || self.saw_end {
            return Vec::new();
        }
        let Ok(msg) = serde_json::from_str::<Value>(line) else {
            return Vec::new();
        };
        match msg.get("type").and_then(Value::as_str) {
            Some("thread.started") => match msg.get("thread_id").and_then(Value::as_str) {
                Some(id) => vec![AgentEvent::SessionStarted {
                    provider_session_id: id.to_string(),
                    model: None,
                }],
                None => Vec::new(),
            },
            Some("item.started") | Some("item.updated") => match msg.get("item") {
                Some(item) => self.on_item(item, false),
                None => Vec::new(),
            },
            Some("item.completed") => match msg.get("item") {
                Some(item) => self.on_item(item, true),
                None => Vec::new(),
            },
            Some("error") => {
                if let Some(message) = msg.get("message").and_then(Value::as_str) {
                    self.last_error = Some(message.to_string());
                }
                Vec::new()
            }
            Some("turn.completed") => self.on_turn_end(&msg, None),
            Some("turn.failed") => {
                let message = msg
                    .get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or_else(|| self.last_error.clone())
                    .unwrap_or_else(|| "Codex reported an error.".to_string());
                self.on_turn_end(&msg, Some(message))
            }
            _ => Vec::new(),
        }
    }

    /// The events that close a turn the CLI never ended, so the caller can
    /// always finish it. Returns nothing if it did.
    pub fn finish(&mut self, end: StreamEnd) -> Vec<AgentEvent> {
        if self.saw_end {
            return Vec::new();
        }
        self.saw_end = true;
        let mut events = self.take_files_changed();
        let (kind, message) = match end {
            StreamEnd::Cancelled => (AgentErrorKind::Cancelled, STOPPED_MESSAGE.to_string()),
            StreamEnd::Exited { code, stderr } => {
                let message = stderr_error(&stderr)
                    .or_else(|| self.last_error.clone())
                    .unwrap_or_else(|| match code {
                        Some(code) => {
                            format!("Codex stopped before finishing the turn (exit code {code}).")
                        }
                        None => "Codex was stopped before finishing the turn.".to_string(),
                    });
                let kind = match classify_error(&message) {
                    AgentErrorKind::Other if !message.contains(BAD_RESUME_MARKER) => {
                        AgentErrorKind::ProcessFailed
                    }
                    kind => kind,
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

    fn on_turn_end(&mut self, msg: &Value, error: Option<String>) -> Vec<AgentEvent> {
        self.saw_end = true;
        let mut events = self.take_files_changed();
        let is_error = error.is_some();
        if let Some(message) = error {
            events.push(AgentEvent::Error {
                kind: classify_error(&message),
                message,
            });
        }
        // As the CLI reports them; `input_tokens` includes the cached ones
        // it also reports separately as `cached_input_tokens`.
        let usage = msg.get("usage").filter(|u| u.is_object()).map(|u| Usage {
            input_tokens: u.get("input_tokens").and_then(Value::as_u64),
            output_tokens: u.get("output_tokens").and_then(Value::as_u64),
        });
        events.push(AgentEvent::TurnCompleted {
            is_error,
            duration_ms: None,
            usage,
        });
        events
    }

    fn on_item(&mut self, item: &Value, completed: bool) -> Vec<AgentEvent> {
        let Some(id) = item.get("id").and_then(Value::as_str) else {
            return Vec::new();
        };
        let kind = item.get("type").and_then(Value::as_str).unwrap_or("");
        let status = item.get("status").and_then(Value::as_str);
        let name = match kind {
            "agent_message" => {
                let text = item.get("text").and_then(Value::as_str).unwrap_or("");
                return if completed && !text.trim().is_empty() {
                    vec![AgentEvent::AssistantText {
                        text: text.to_string(),
                    }]
                } else {
                    Vec::new()
                };
            }
            "command_execution" | "file_change" | "web_search" => kind.to_string(),
            "mcp_tool_call" => {
                let server = item.get("server").and_then(Value::as_str).unwrap_or("");
                let tool = item.get("tool").and_then(Value::as_str).unwrap_or("");
                format!("mcp__{server}__{tool}")
            }
            // reasoning, todo_list, non-fatal `error` items, later additions.
            _ => return Vec::new(),
        };

        let mut events = Vec::new();
        let plan = plan_call(item);
        if !self.items.contains_key(id) {
            // A plan is only shown once the server has accepted it.
            let hidden = plan.is_some();
            self.items.insert(id.to_string(), SeenItem { hidden });
            if !hidden {
                events.push(AgentEvent::ToolUse {
                    id: id.to_string(),
                    name: name.clone(),
                    summary: self.summarize(kind, item),
                });
            }
        }
        if !completed {
            return events;
        }
        let ok = status == Some("completed")
            && match kind {
                "command_execution" => item.get("exit_code").and_then(Value::as_i64) == Some(0),
                "mcp_tool_call" => {
                    item.get("error").is_none_or(Value::is_null)
                        && !item
                            .get("result")
                            .and_then(|r| r.get("is_error"))
                            .and_then(Value::as_bool)
                            .unwrap_or(false)
                }
                _ => true,
            };
        let hidden = self.items.get(id).is_some_and(|s| s.hidden);
        if let Some((title, steps)) = plan {
            if ok {
                events.push(AgentEvent::PlanProposed { title, steps });
                return events;
            }
            if hidden {
                // The server refused it: show the call and its failure.
                events.push(AgentEvent::ToolUse {
                    id: id.to_string(),
                    name,
                    summary: "Proposing a plan".to_string(),
                });
            }
        }
        if ok {
            for path in self.written_paths(kind, item) {
                if !self.files_changed.contains(&path) {
                    self.files_changed.push(path);
                }
            }
        }
        events.push(AgentEvent::ToolResult {
            id: id.to_string(),
            ok,
            summary: if ok {
                "Done".to_string()
            } else {
                self.failure_summary(kind, item)
            },
        });
        events
    }

    /// Project-relative paths a successful tool item wrote.
    fn written_paths(&self, kind: &str, item: &Value) -> Vec<String> {
        match kind {
            "file_change" => changes(item)
                .filter_map(|(path, _)| self.project_relative(path))
                .collect(),
            "mcp_tool_call"
                if item.get("server").and_then(Value::as_str) == Some(MCP_SERVER_NAME)
                    && item.get("tool").and_then(Value::as_str) == Some(WRITE_CONTEXT_CARD) =>
            {
                item.get("arguments")
                    .and_then(|a| a.get("path"))
                    .and_then(Value::as_str)
                    .and_then(|p| self.project_relative(&format!("{CONTEXT_DIR}/{p}")))
                    .into_iter()
                    .collect()
            }
            _ => Vec::new(),
        }
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
    /// the project, otherwise just its file name.
    fn display_path(&self, raw: &str) -> String {
        self.project_relative(raw).unwrap_or_else(|| {
            Path::new(raw)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| raw.to_string())
        })
    }

    /// A short plain-language line for one tool item. Never the raw input
    /// (a shell command line can be long and cryptic): at most a file name.
    fn summarize(&self, kind: &str, item: &Value) -> String {
        match kind {
            "command_execution" => "Running a command".to_string(),
            "web_search" => "Looking things up online".to_string(),
            "file_change" => {
                let all: Vec<(&str, &str)> = changes(item).collect();
                match all.as_slice() {
                    [] => "Editing files".to_string(),
                    [(path, kind)] => {
                        let verb = match *kind {
                            "add" => "Creating",
                            "delete" => "Deleting",
                            _ => "Editing",
                        };
                        format!("{verb} {}", self.display_path(path))
                    }
                    many => format!("Editing {} files", many.len()),
                }
            }
            _ => {
                let server = item.get("server").and_then(Value::as_str).unwrap_or("");
                let tool = item.get("tool").and_then(Value::as_str).unwrap_or("");
                let field = |key: &str| {
                    item.get("arguments")
                        .and_then(|a| a.get(key))
                        .and_then(Value::as_str)
                };
                if server != MCP_SERVER_NAME {
                    return format!("Using {}", tool.replace('_', " "));
                }
                let card = || field("path").map(|p| p.trim_end_matches(".md").to_string());
                match tool {
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
                    "project_map" => "Looking at how the game is put together".to_string(),
                    "find_symbol" => "Looking up where something is used".to_string(),
                    "describe_scene" => "Reading a scene".to_string(),
                    "list_snapshots" => "Looking at the project's history".to_string(),
                    t if t == PROPOSE_PLAN_TOOL => "Proposing a plan".to_string(),
                    other => format!("Using {}", other.replace('_', " ")),
                }
            }
        }
    }

    /// One line on why a tool item failed, from its real error text, with
    /// the project's absolute path taken out (so a home-directory path
    /// doesn't end up in the chat files committed with the project).
    fn failure_summary(&self, kind: &str, item: &Value) -> String {
        let status = item.get("status").and_then(Value::as_str);
        let text = match kind {
            "command_execution" => match (status, item.get("exit_code").and_then(Value::as_i64)) {
                (Some("declined"), _) => "Not allowed".to_string(),
                (_, Some(code)) => format!("Exited with code {code}"),
                _ => String::new(),
            },
            "mcp_tool_call" => item
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| {
                    let content = item.get("result")?.get("content")?.as_array()?;
                    Some(
                        content
                            .iter()
                            .filter_map(|c| c.get("text").and_then(Value::as_str))
                            .collect::<Vec<_>>()
                            .join("\n"),
                    )
                })
                .unwrap_or_default(),
            _ => String::new(),
        };
        let line = first_line(&strip_roots(&text, &self.roots), MAX_RESULT_SUMMARY);
        if line.is_empty() {
            match status {
                Some("declined") => "Not allowed".to_string(),
                _ => "Failed".to_string(),
            }
        } else {
            line
        }
    }
}

/// The `(path, kind)` pairs of a `file_change` item.
fn changes(item: &Value) -> impl Iterator<Item = (&str, &str)> {
    item.get("changes")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .filter_map(|c| {
            Some((
                c.get("path").and_then(Value::as_str)?,
                c.get("kind").and_then(Value::as_str).unwrap_or("update"),
            ))
        })
}

/// The title and steps of an InfinaBox `propose_plan` call, if the item is
/// one and its input is well-formed: a non-empty `title` string and a
/// non-empty `steps` list of non-empty strings.
fn plan_call(item: &Value) -> Option<(String, Vec<String>)> {
    if item.get("type").and_then(Value::as_str) != Some("mcp_tool_call")
        || item.get("server").and_then(Value::as_str) != Some(MCP_SERVER_NAME)
        || item.get("tool").and_then(Value::as_str) != Some(PROPOSE_PLAN_TOOL)
    {
        return None;
    }
    let args = item.get("arguments")?;
    let title = args.get("title")?.as_str()?.trim();
    let steps = args
        .get("steps")?
        .as_array()?
        .iter()
        .map(|s| {
            s.as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
        .collect::<Option<Vec<String>>>()?;
    (!title.is_empty() && !steps.is_empty()).then(|| (title.to_string(), steps))
}

/// The CLI's own error line from stderr (`Error: <message>`, printed before
/// a backtrace when it fails outright), without the prefix.
fn stderr_error(stderr: &str) -> Option<String> {
    let stderr = tail(stderr, MAX_STDERR_MESSAGE);
    if let Some(line) = stderr
        .lines()
        .rev()
        .find_map(|l| l.trim().strip_prefix("Error: "))
    {
        return Some(line.trim().to_string());
    }
    // Otherwise whatever stderr says, minus the CLI's routine notices.
    let text: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|l| {
            !l.is_empty()
                && !l.starts_with("WARNING: proceeding")
                && !l.starts_with("Reading additional input from stdin")
        })
        .collect();
    (!text.is_empty()).then(|| text.join("\n"))
}

/// Classifies an error from its real message text. Deliberately
/// conservative: anything not clearly a sign-in or rate-limit problem is
/// `Other`, shown with its real text. Codex reports HTTP failures as
/// `unexpected status 401 Unauthorized: ...` (the `i_unauthorized`
/// recording).
pub fn classify_error(message: &str) -> AgentErrorKind {
    let lower = message.to_lowercase();
    const AUTH: &[&str] = &[
        "401 unauthorized",
        "not logged in",
        "invalid_api_key",
        "invalid api key",
        "missing bearer",
        "please log in",
        "codex login",
        "token_expired",
        "refresh token",
    ];
    const RATE: &[&str] = &[
        "429 too many requests",
        "rate limit",
        "rate_limit",
        "usage limit",
        "usage_limit",
        "quota",
    ];
    if AUTH.iter().any(|p| lower.contains(p)) {
        AgentErrorKind::NotAuthenticated
    } else if RATE.iter().any(|p| lower.contains(p)) {
        AgentErrorKind::RateLimited
    } else {
        AgentErrorKind::Other
    }
}

/// `text` with each project root taken out: paths inside it become
/// relative, the folder itself its name. Longest root first (a canonical
/// `/private/tmp/x` contains `/tmp/x`).
fn strip_roots(text: &str, roots: &[PathBuf]) -> String {
    let mut text = text.to_string();
    let mut roots: Vec<&PathBuf> = roots.iter().collect();
    roots.sort_by_key(|r| std::cmp::Reverse(r.as_os_str().len()));
    for root in roots {
        let root_str = root.to_string_lossy();
        let root_str = root_str.trim_end_matches(['/', '\\']);
        if root_str.is_empty() {
            continue;
        }
        let name = root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        text = text
            .replace(&format!("{root_str}/"), "")
            .replace(&format!("{root_str}\\"), "")
            .replace(root_str, &name);
    }
    text
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
