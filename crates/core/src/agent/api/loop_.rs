//! The tool loop: model → tool calls → tool results → model, up to
//! `MAX_MODEL_CALLS`, emitting `AgentEvent`s as it goes.
//!
//! It keeps every guarantee the CLI runtimes give the layers above:
//! - `SessionStarted` comes first (a fresh id: there is no provider-side
//!   session to resume, so the conversation is rebuilt from the chat store);
//! - a turn that got this far always ends with `TurnCompleted`, preceded by
//!   `FilesChanged` (the project-relative paths the file tools and
//!   `write_context_card` really wrote) and, if it went wrong, one `Error`;
//! - a stopped turn ends with `Error { Cancelled, "Stopped." }`;
//! - `Usage` only sums figures the provider reported.
//!
//! Events are named like the CLI runtimes' (`Read`/`Write`/`Edit`/`Glob`/
//! `Grep` for the file tools, `mcp__infinabox__<tool>` for the InfinaBox
//! server's), so the app's checks on them work unchanged. A valid
//! `propose_plan` call becomes `PlanProposed` instead of a `ToolUse` (its
//! result is hidden, as with the CLIs), and the turn ends after that round:
//! the plan card is where the conversation waits for the person.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use serde_json::Value;

use super::mcp::{McpBridge, McpError};
use super::tools::{self, FileTools};
use super::{
    BackendError, ChatBackend, Completion, MAX_MODEL_CALLS, MAX_TOOL_RESULT_BYTES, Message,
    ToolCall, ToolSpec,
};
use crate::agent::claude_stream::STOPPED_MESSAGE;
use crate::agent::prompt::{self, PROPOSE_PLAN_TOOL};
use crate::agent::types::{AgentErrorKind, AgentEvent, TurnRequest, Usage};
use crate::chat_store::{self, ChatRecord};
use crate::redact::redact;

/// Earlier records of the thread put back into the conversation.
const HISTORY_MAX_RECORDS: usize = 40;
/// Most text of them, in bytes.
const HISTORY_MAX_BYTES: usize = 60 * 1024;
/// Longest error text shown as a tool step's summary.
const MAX_RESULT_SUMMARY: usize = 200;

const MCP_UNAVAILABLE: &str =
    "I couldn't start InfinaBox's game tools, so I can only edit files this time.";

/// The MCP tool whose `path` argument names a card the server writes.
const WRITE_CONTEXT_CARD: &str = "write_context_card";
const CONTEXT_DIR: &str = ".ibproject/context";

/// Runs one turn. Always returns `Ok` once it has started: a failure is
/// reported through the events, which end with `TurnCompleted`.
pub fn run_turn(
    backend: &dyn ChatBackend,
    req: &TurnRequest,
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(AgentEvent),
) -> anyhow::Result<()> {
    let started = Instant::now();
    let mut turn = Turn {
        on_event,
        usage: None,
        files: Vec::new(),
    };
    turn.emit(AgentEvent::SessionStarted {
        provider_session_id: uuid::Uuid::new_v4().to_string(),
        model: Some(backend.model()),
    });

    let file_tools = match FileTools::new(&req.project_path) {
        Ok(t) => t,
        Err(message) => {
            turn.finish(started, Some((AgentErrorKind::Other, message)));
            return Ok(());
        }
    };

    // The InfinaBox server: optional. Without it the model still edits files.
    let mut bridge: Option<McpBridge> = None;
    let mut mcp_tools: Vec<ToolSpec> = Vec::new();
    match McpBridge::start(&req.mcp, cancel) {
        Ok((b, listed)) => {
            mcp_tools = listed
                .into_iter()
                .filter(|t| !tools::is_file_tool(&t.name))
                .collect();
            bridge = Some(b);
        }
        Err(McpError::Cancelled) => {
            turn.finish(
                started,
                Some((AgentErrorKind::Cancelled, STOPPED_MESSAGE.into())),
            );
            return Ok(());
        }
        Err(McpError::Failed(_)) => turn.emit(AgentEvent::AssistantText {
            text: MCP_UNAVAILABLE.into(),
        }),
    }
    let mut specs = tools::specs();
    specs.extend(mcp_tools.iter().cloned());

    let agents_md = prompt::read_agents_md(&req.project_path);
    let system = prompt::system_prompt(&req.options, agents_md.as_deref());
    let mut messages = history_messages(&req.project_path, &req.thread_id, &req.message);

    let mut failure: Option<(AgentErrorKind, String)> = None;
    let mut done = false;
    let mut said_anything = false;
    'rounds: for _ in 0..MAX_MODEL_CALLS {
        if cancel.load(Ordering::SeqCst) {
            failure = Some((AgentErrorKind::Cancelled, STOPPED_MESSAGE.into()));
            break;
        }
        let completion: Completion = match backend.complete(&system, &messages, &specs, cancel) {
            Ok(c) => c,
            Err(e) => {
                failure = Some(match e {
                    BackendError::Cancelled => {
                        (AgentErrorKind::Cancelled, STOPPED_MESSAGE.to_string())
                    }
                    other => (other.kind(), other.message()),
                });
                break;
            }
        };
        turn.add_usage(completion.usage.as_ref());
        let text = completion.text.clone().filter(|t| !t.trim().is_empty());
        if let Some(text) = &text {
            said_anything = true;
            turn.emit(AgentEvent::AssistantText { text: text.clone() });
        }
        messages.push(Message::Assistant {
            text: text.clone(),
            tool_calls: completion.tool_calls.clone(),
        });
        if completion.tool_calls.is_empty() {
            if !said_anything {
                failure = Some((
                    AgentErrorKind::Other,
                    "The AI sent back an empty answer. Try asking again.".into(),
                ));
            }
            done = true;
            break;
        }
        said_anything = true;

        let mut plan_proposed = false;
        for call in &completion.tool_calls {
            if cancel.load(Ordering::SeqCst) {
                failure = Some((AgentErrorKind::Cancelled, STOPPED_MESSAGE.into()));
                break 'rounds;
            }
            let is_mcp = mcp_tools.iter().any(|t| t.name == call.name);
            let outcome = run_tool(call, is_mcp, &file_tools, bridge.as_mut(), cancel, &mut turn);
            let ToolOutcome::Finished { content, is_error } = outcome else {
                failure = Some((AgentErrorKind::Cancelled, STOPPED_MESSAGE.into()));
                break 'rounds;
            };
            plan_proposed |= plan_from(call, is_mcp).is_some();
            messages.push(Message::ToolResult {
                call_id: call.id.clone(),
                content: truncate_result(content),
                is_error,
            });
        }
        if plan_proposed {
            done = true;
            break;
        }
    }
    if !done && failure.is_none() {
        failure = Some((
            AgentErrorKind::Other,
            format!(
                "I stopped after {MAX_MODEL_CALLS} steps without finishing. \
                 Ask me to keep going, or try a smaller request."
            ),
        ));
    }
    // The server is gone before the turn is reported over.
    drop(bridge);
    turn.finish(started, failure);
    Ok(())
}

struct Turn<'a> {
    on_event: &'a mut dyn FnMut(AgentEvent),
    usage: Option<Usage>,
    files: Vec<String>,
}

impl Turn<'_> {
    fn emit(&mut self, event: AgentEvent) {
        (self.on_event)(event);
    }

    /// Adds a completion's reported figures; a figure nobody reported stays
    /// unknown.
    fn add_usage(&mut self, reported: Option<&Usage>) {
        let Some(reported) = reported else { return };
        let sum = |a: Option<u64>, b: Option<u64>| match (a, b) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or(0).saturating_add(b.unwrap_or(0))),
        };
        let total = self.usage.get_or_insert(Usage {
            input_tokens: None,
            output_tokens: None,
        });
        total.input_tokens = sum(total.input_tokens, reported.input_tokens);
        total.output_tokens = sum(total.output_tokens, reported.output_tokens);
    }

    fn note_written(&mut self, path: String) {
        if !self.files.contains(&path) {
            self.files.push(path);
        }
    }

    /// `FilesChanged`, then the `Error` if the turn failed, then
    /// `TurnCompleted`.
    fn finish(&mut self, started: Instant, failure: Option<(AgentErrorKind, String)>) {
        if !self.files.is_empty() {
            let paths = std::mem::take(&mut self.files);
            self.emit(AgentEvent::FilesChanged { paths });
        }
        let is_error = failure.is_some();
        if let Some((kind, message)) = failure {
            self.emit(AgentEvent::Error { kind, message });
        }
        let usage = self.usage.take();
        self.emit(AgentEvent::TurnCompleted {
            is_error,
            duration_ms: Some(started.elapsed().as_millis() as u64),
            usage,
        });
    }
}

enum ToolOutcome {
    Finished { content: String, is_error: bool },
    Cancelled,
}

/// A `propose_plan` call the InfinaBox server would accept.
fn plan_from(call: &ToolCall, is_mcp: bool) -> Option<prompt::ValidPlan> {
    if !is_mcp || call.name != PROPOSE_PLAN_TOOL {
        return None;
    }
    let title = call.arguments.get("title")?.as_str()?;
    let steps = call
        .arguments
        .get("steps")?
        .as_array()?
        .iter()
        .map(|s| s.as_str().map(str::to_string))
        .collect::<Option<Vec<String>>>()?;
    prompt::validate_plan(title, &steps).ok()
}

/// Announces one tool call, runs it and reports its result.
fn run_tool(
    call: &ToolCall,
    is_mcp: bool,
    file_tools: &FileTools,
    bridge: Option<&mut McpBridge>,
    cancel: &AtomicBool,
    turn: &mut Turn,
) -> ToolOutcome {
    let plan = plan_from(call, is_mcp);
    if let Some(plan) = &plan {
        turn.emit(AgentEvent::PlanProposed {
            title: plan.title.clone(),
            steps: plan.steps.clone(),
        });
    } else {
        turn.emit(AgentEvent::ToolUse {
            id: call.id.clone(),
            name: tools::event_name(&call.name, is_mcp),
            summary: tools::summary(&call.name, &call.arguments),
        });
    }

    let mut written: Option<String> = None;
    let result: Result<String, String> = if tools::is_file_tool(&call.name) {
        file_tools.call(&call.name, &call.arguments).map(|out| {
            written = out.written;
            out.text
        })
    } else if is_mcp {
        match bridge {
            Some(bridge) => match bridge.call(&call.name, &call.arguments, cancel) {
                Ok(r) if r.is_error => Err(r.text),
                Ok(r) => {
                    if call.name == WRITE_CONTEXT_CARD
                        && let Some(card) = call.arguments.get("path").and_then(Value::as_str)
                        && let Some(path) = context_card_path(card)
                    {
                        written = Some(path);
                    }
                    Ok(r.text)
                }
                Err(McpError::Cancelled) => return ToolOutcome::Cancelled,
                Err(McpError::Failed(m)) => Err(format!("InfinaBox's game tools failed: {m}")),
            },
            None => Err("InfinaBox's game tools aren't available this time.".into()),
        }
    } else {
        Err(format!(
            "There is no tool called {}. Use one of the tools you were given.",
            call.name
        ))
    };

    let ok = result.is_ok();
    if let (true, Some(path)) = (ok, written) {
        turn.note_written(path);
    }
    if plan.is_none() {
        turn.emit(AgentEvent::ToolResult {
            id: call.id.clone(),
            ok,
            summary: match &result {
                Ok(_) => "Done".into(),
                Err(message) => failure_summary(message),
            },
        });
    }
    match result {
        Ok(content) => ToolOutcome::Finished {
            content,
            is_error: false,
        },
        Err(content) => ToolOutcome::Finished {
            content,
            is_error: true,
        },
    }
}

/// The project-relative path of a Context card the server was asked to write
/// (its `path` is relative to the context folder), if it stays inside it.
fn context_card_path(card: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for part in card.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => return None,
            p => parts.push(p),
        }
    }
    (!parts.is_empty()).then(|| format!("{CONTEXT_DIR}/{}", parts.join("/")))
}

/// The first line of a failed tool's message, shortened.
fn failure_summary(message: &str) -> String {
    let line = message.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
    if line.is_empty() {
        return "Failed".into();
    }
    let mut short: String = line.chars().take(MAX_RESULT_SUMMARY).collect();
    if line.chars().count() > MAX_RESULT_SUMMARY {
        short.push('…');
    }
    short
}

/// A tool result cut to `MAX_TOOL_RESULT_BYTES` with a note saying so.
fn truncate_result(content: String) -> String {
    if content.len() <= MAX_TOOL_RESULT_BYTES {
        return content;
    }
    let mut end = MAX_TOOL_RESULT_BYTES;
    while !content.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{}\n[Result cut: it was {} bytes; only the first {} KB is shown.]",
        &content[..end],
        content.len(),
        MAX_TOOL_RESULT_BYTES / 1024
    )
}

/// The conversation so far as messages: the thread's earlier records (user
/// texts, assistant texts, one-line tool summaries as plain assistant text;
/// the last `HISTORY_MAX_RECORDS` and at most `HISTORY_MAX_BYTES`), then the
/// new message. The record of the new message itself, which the app saves
/// before the turn starts, is not repeated.
pub fn history_messages(project: &Path, thread_id: &str, new_message: &str) -> Vec<Message> {
    let mut records = chat_store::load_thread(project, thread_id)
        .map(|(_, records)| records)
        .unwrap_or_default();
    if let Some(ChatRecord::User { text, .. }) = records.last()
        && (text == new_message || *text == redact(new_message))
    {
        records.pop();
    }
    let start = records.len().saturating_sub(HISTORY_MAX_RECORDS);
    // (is_user, text) with neighbours of the same role merged.
    let mut turns: Vec<(bool, String)> = Vec::new();
    let mut push = |is_user: bool, text: String| {
        if text.trim().is_empty() {
            return;
        }
        match turns.last_mut() {
            Some((last_user, last)) if *last_user == is_user => {
                last.push_str(if is_user { "\n\n" } else { "\n" });
                last.push_str(&text);
            }
            _ => turns.push((is_user, text)),
        }
    };
    for record in &records[start..] {
        match record {
            ChatRecord::User { text, .. } => push(true, text.clone()),
            ChatRecord::Event { event, .. } => match event {
                AgentEvent::AssistantText { text } => push(false, text.clone()),
                AgentEvent::ToolUse { summary, .. } => push(false, format!("({summary})")),
                AgentEvent::PlanProposed { title, steps } => push(
                    false,
                    format!("(Proposed a plan: {title}. Steps: {})", steps.join("; ")),
                ),
                _ => {}
            },
        }
    }
    // Drop the oldest until it fits; a conversation starts with the person.
    let size = |turns: &[(bool, String)]| turns.iter().map(|(_, t)| t.len()).sum::<usize>();
    while size(&turns) > HISTORY_MAX_BYTES && turns.len() > 1 {
        turns.remove(0);
    }
    if let Some((_, only)) = turns.first_mut()
        && only.len() > HISTORY_MAX_BYTES
    {
        let mut cut = only.len() - HISTORY_MAX_BYTES;
        while !only.is_char_boundary(cut) {
            cut += 1;
        }
        *only = only[cut..].to_string();
    }
    while turns.first().is_some_and(|(is_user, _)| !is_user) {
        turns.remove(0);
    }

    let mut messages: Vec<Message> = turns
        .into_iter()
        .map(|(is_user, text)| {
            if is_user {
                Message::User(text)
            } else {
                Message::Assistant {
                    text: Some(text),
                    tool_calls: Vec::new(),
                }
            }
        })
        .collect();
    match messages.last_mut() {
        // An earlier turn that never got an answer: one message, both parts.
        Some(Message::User(last)) => {
            last.push_str("\n\n");
            last.push_str(new_message);
        }
        _ => messages.push(Message::User(new_message.to_string())),
    }
    messages
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::api::{ApiRuntime, StopReason};
    use crate::agent::types::{AgentRuntime, McpLaunch, TurnOptions};
    use serde_json::json;
    use std::collections::VecDeque;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    /// A backend that plays back a queue of completions and remembers what
    /// it was asked.
    struct Scripted {
        queue: Mutex<VecDeque<Result<Completion, BackendError>>>,
        seen: Mutex<Vec<(String, Vec<Message>, Vec<String>)>>,
    }

    impl Scripted {
        fn new(items: Vec<Result<Completion, BackendError>>) -> Arc<Self> {
            Arc::new(Self {
                queue: Mutex::new(items.into()),
                seen: Mutex::new(Vec::new()),
            })
        }
    }

    impl ChatBackend for Scripted {
        fn label(&self) -> String {
            "Fake".into()
        }
        fn model(&self) -> String {
            "fake-1".into()
        }
        fn complete(
            &self,
            system: &str,
            messages: &[Message],
            tools: &[ToolSpec],
            _cancel: &AtomicBool,
        ) -> Result<Completion, BackendError> {
            self.seen.lock().unwrap().push((
                system.to_string(),
                messages.to_vec(),
                tools.iter().map(|t| t.name.clone()).collect(),
            ));
            self.queue
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(Err(BackendError::Other("script ran out".into())))
        }
    }

    fn text(t: &str) -> Result<Completion, BackendError> {
        Ok(Completion {
            text: Some(t.into()),
            tool_calls: vec![],
            usage: None,
            stop: StopReason::EndTurn,
        })
    }

    fn calls(text: Option<&str>, calls: Vec<(&str, Value)>) -> Result<Completion, BackendError> {
        Ok(Completion {
            text: text.map(str::to_string),
            tool_calls: calls
                .into_iter()
                .enumerate()
                .map(|(i, (name, arguments))| ToolCall {
                    id: format!("call_{i}_{name}"),
                    name: name.into(),
                    arguments,
                })
                .collect(),
            usage: Some(Usage {
                input_tokens: Some(10),
                output_tokens: Some(5),
            }),
            stop: StopReason::ToolUse,
        })
    }

    fn node_launch() -> McpLaunch {
        McpLaunch {
            command: PathBuf::from("node"),
            args: vec![
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../scripts/fixtures/echo-mcp-server.mjs")
                    .to_string_lossy()
                    .into_owned(),
                "--propose-plan".into(),
            ],
            env: vec![],
        }
    }

    fn request(dir: &Path, thread: &str, message: &str, mcp: McpLaunch) -> TurnRequest {
        TurnRequest {
            thread_id: thread.into(),
            project_path: dir.to_path_buf(),
            message: message.into(),
            resume_provider_session_id: None,
            mcp,
            options: TurnOptions::default(),
        }
    }

    fn run(backend: Arc<Scripted>, req: TurnRequest) -> Vec<AgentEvent> {
        let runtime = ApiRuntime::new(backend);
        let mut events = Vec::new();
        runtime.run_turn(req, &mut |e| events.push(e)).unwrap();
        events
    }

    fn project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("player.gd"), "var speed = 3\n").unwrap();
        dir
    }

    fn last_two(events: &[AgentEvent]) -> (&AgentEvent, &AgentEvent) {
        (&events[events.len() - 2], &events[events.len() - 1])
    }

    #[test]
    fn a_text_only_turn() {
        let dir = project();
        let backend = Scripted::new(vec![text("Hello there.")]);
        let events = run(backend.clone(), request(dir.path(), "t1", "hi", node_launch()));
        assert!(matches!(
            &events[0],
            AgentEvent::SessionStarted { provider_session_id, model }
                if !provider_session_id.is_empty() && model.as_deref() == Some("fake-1")
        ));
        assert_eq!(
            events[1],
            AgentEvent::AssistantText {
                text: "Hello there.".into()
            }
        );
        assert!(matches!(
            events.last(),
            Some(AgentEvent::TurnCompleted { is_error: false, .. })
        ));
        assert_eq!(events.len(), 3);
        let seen = backend.seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        // The instructions and the tools reached the model.
        assert!(seen[0].0.contains("Plans come first"));
        assert_eq!(seen[0].1, vec![Message::User("hi".into())]);
        assert!(seen[0].2.contains(&"read_file".to_string()));
        assert!(seen[0].2.contains(&"propose_plan".to_string()));
    }

    #[test]
    fn agents_md_reaches_the_system_prompt() {
        let dir = project();
        std::fs::write(dir.path().join("AGENTS.md"), "Always use tabs.").unwrap();
        let backend = Scripted::new(vec![text("ok")]);
        run(backend.clone(), request(dir.path(), "t1", "hi", node_launch()));
        assert!(backend.seen.lock().unwrap()[0].0.contains("Always use tabs."));
    }

    #[test]
    fn a_tool_round_trip_really_edits_and_reports_files_changed() {
        let dir = project();
        let backend = Scripted::new(vec![
            calls(
                Some("Speeding up."),
                vec![
                    ("read_file", json!({"path": "player.gd"})),
                    (
                        "edit_file",
                        json!({"path": "player.gd", "old_string": "3", "new_string": "9"}),
                    ),
                    ("echo", json!({"text": "x"})),
                ],
            ),
            text("Done: speed is 9."),
        ]);
        let events = run(backend.clone(), request(dir.path(), "t1", "faster", node_launch()));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("player.gd")).unwrap(),
            "var speed = 9\n"
        );
        let uses: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                AgentEvent::ToolUse { name, summary, .. } => Some((name.as_str(), summary.as_str())),
                _ => None,
            })
            .collect();
        assert_eq!(
            uses,
            [
                ("Read", "Reading player.gd"),
                ("Edit", "Editing player.gd"),
                ("mcp__infinabox__echo", "Using echo")
            ]
        );
        let results: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                AgentEvent::ToolResult { ok, .. } => Some(*ok),
                _ => None,
            })
            .collect();
        assert_eq!(results, [true, true, true]);
        let (before, last) = last_two(&events);
        assert_eq!(
            *before,
            AgentEvent::FilesChanged {
                paths: vec!["player.gd".into()]
            }
        );
        // Usage is the sum of what was reported (the text-only round reported none).
        assert!(matches!(
            last,
            AgentEvent::TurnCompleted {
                is_error: false,
                usage: Some(Usage { input_tokens: Some(10), output_tokens: Some(5) }),
                ..
            }
        ));
        // The second model call saw the tool results.
        let seen = backend.seen.lock().unwrap();
        let second = &seen[1].1;
        assert!(matches!(&second[1], Message::Assistant { tool_calls, .. } if tool_calls.len() == 3));
        assert!(matches!(&second[3], Message::ToolResult { content, is_error: false, .. } if content.starts_with("Edited player.gd")));
        assert!(matches!(&second[4], Message::ToolResult { content, .. } if content == "echo: x"));
    }

    #[test]
    fn sandbox_refusals_are_tool_errors_the_model_can_read() {
        let dir = project();
        let backend = Scripted::new(vec![
            calls(
                None,
                vec![
                    ("write_file", json!({"path": ".git/config", "content": "x"})),
                    ("read_file", json!({"path": "../secret"})),
                    ("nonexistent_tool", json!({})),
                ],
            ),
            text("I couldn't."),
        ]);
        let events = run(backend.clone(), request(dir.path(), "t1", "go", node_launch()));
        let results: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                AgentEvent::ToolResult { ok, summary, .. } => Some((*ok, summary.clone())),
                _ => None,
            })
            .collect();
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|(ok, _)| !ok));
        assert!(results[0].1.contains("git history"));
        assert!(results[1].1.contains("outside the project"));
        assert!(!events.iter().any(|e| matches!(e, AgentEvent::FilesChanged { .. })));
        let seen = backend.seen.lock().unwrap();
        assert!(matches!(&seen[1].1[2], Message::ToolResult { is_error: true, .. }));
        assert!(matches!(
            events.last(),
            Some(AgentEvent::TurnCompleted { is_error: false, .. })
        ));
    }

    #[test]
    fn long_results_are_truncated_with_a_note() {
        let dir = project();
        let big = "x".repeat(MAX_TOOL_RESULT_BYTES * 2);
        let content = truncate_result(big);
        assert!(content.len() < MAX_TOOL_RESULT_BYTES + 200);
        assert!(content.contains("Result cut"));
        assert_eq!(truncate_result("short".into()), "short");
        // Multi-byte text is cut on a character boundary.
        let wide = "é".repeat(MAX_TOOL_RESULT_BYTES);
        assert!(truncate_result(wide).contains("Result cut"));
        // And through the loop: a big search result arrives capped.
        std::fs::write(dir.path().join("big.txt"), "y".repeat(300 * 1024)).unwrap();
        let backend = Scripted::new(vec![
            calls(None, vec![("read_file", json!({"path": "big.txt"}))]),
            text("ok"),
        ]);
        run(backend.clone(), request(dir.path(), "t1", "read", node_launch()));
        let seen = backend.seen.lock().unwrap();
        let Message::ToolResult { content, .. } = &seen[1].1[2] else {
            panic!()
        };
        assert!(content.len() <= MAX_TOOL_RESULT_BYTES + 200);
    }

    #[test]
    fn the_model_call_limit_ends_the_turn() {
        let dir = project();
        let script = (0..MAX_MODEL_CALLS + 5)
            .map(|_| calls(None, vec![("list_files", json!({}))]))
            .collect();
        let backend = Scripted::new(script);
        let events = run(backend.clone(), request(dir.path(), "t1", "loop", node_launch()));
        assert_eq!(backend.seen.lock().unwrap().len(), MAX_MODEL_CALLS);
        let (before, last) = last_two(&events);
        assert!(matches!(
            before,
            AgentEvent::Error { kind: AgentErrorKind::Other, message } if message.contains("stopped after 40 steps")
        ));
        assert!(matches!(last, AgentEvent::TurnCompleted { is_error: true, .. }));
    }

    #[test]
    fn a_backend_error_becomes_an_error_event() {
        let dir = project();
        let cases = [
            (BackendError::NotAuthenticated("bad key".into()), AgentErrorKind::NotAuthenticated, "bad key"),
            (BackendError::RateLimited("slow down".into()), AgentErrorKind::RateLimited, "slow down"),
            (BackendError::Unreachable("no route".into()), AgentErrorKind::ProcessFailed, "no route"),
            (BackendError::Other("weird".into()), AgentErrorKind::Other, "weird"),
        ];
        for (error, kind, message) in cases {
            let backend = Scripted::new(vec![Err(error)]);
            let events = run(backend, request(dir.path(), "t1", "hi", node_launch()));
            let (before, last) = last_two(&events);
            assert_eq!(
                *before,
                AgentEvent::Error {
                    kind,
                    message: message.into()
                }
            );
            assert!(matches!(last, AgentEvent::TurnCompleted { is_error: true, .. }));
        }
    }

    #[test]
    fn an_empty_answer_is_reported() {
        let dir = project();
        let backend = Scripted::new(vec![Ok(Completion {
            text: None,
            tool_calls: vec![],
            usage: None,
            stop: StopReason::EndTurn,
        })]);
        let events = run(backend, request(dir.path(), "t1", "hi", node_launch()));
        assert!(events.iter().any(|e| matches!(e, AgentEvent::Error { kind: AgentErrorKind::Other, .. })));
    }

    #[test]
    fn a_cancelled_backend_call_ends_with_stopped_and_files_changed_first() {
        let dir = project();
        let backend = Scripted::new(vec![
            calls(None, vec![("write_file", json!({"path": "a.txt", "content": "a"}))]),
            Err(BackendError::Cancelled),
        ]);
        let events = run(backend, request(dir.path(), "t1", "hi", node_launch()));
        let n = events.len();
        assert_eq!(
            events[n - 3],
            AgentEvent::FilesChanged {
                paths: vec!["a.txt".into()]
            }
        );
        assert_eq!(
            events[n - 2],
            AgentEvent::Error {
                kind: AgentErrorKind::Cancelled,
                message: "Stopped.".into()
            }
        );
        assert!(matches!(events[n - 1], AgentEvent::TurnCompleted { is_error: true, .. }));
    }

    /// Blocks in `complete` until cancelled, like a backend waiting on the network.
    struct Blocking {
        entered: Arc<AtomicBool>,
    }
    impl ChatBackend for Blocking {
        fn label(&self) -> String {
            "Blocking".into()
        }
        fn model(&self) -> String {
            "b".into()
        }
        fn complete(
            &self,
            _: &str,
            _: &[Message],
            _: &[ToolSpec],
            cancel: &AtomicBool,
        ) -> Result<Completion, BackendError> {
            self.entered.store(true, Ordering::SeqCst);
            while !cancel.load(Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(BackendError::Cancelled)
        }
    }

    #[test]
    fn cancel_stops_a_running_turn_and_a_second_turn_is_refused_meanwhile() {
        let dir = project();
        let entered = Arc::new(AtomicBool::new(false));
        let runtime = Arc::new(ApiRuntime::new(Arc::new(Blocking {
            entered: entered.clone(),
        })));
        let req = request(dir.path(), "t1", "hi", node_launch());
        let worker = {
            let runtime = runtime.clone();
            let req = req.clone();
            std::thread::spawn(move || {
                let mut events = Vec::new();
                runtime.run_turn(req, &mut |e| events.push(e)).unwrap();
                events
            })
        };
        while !entered.load(Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        // A second turn on the same thread: Err, nothing emitted.
        let mut second = Vec::new();
        assert!(runtime.run_turn(req.clone(), &mut |e| second.push(e)).is_err());
        assert!(second.is_empty());
        // Cancelling another thread does nothing to this one.
        runtime.cancel("other-thread");
        runtime.cancel("t1");
        let events = worker.join().unwrap();
        let (before, last) = last_two(&events);
        assert!(matches!(before, AgentEvent::Error { kind: AgentErrorKind::Cancelled, message } if message == "Stopped."));
        assert!(matches!(last, AgentEvent::TurnCompleted { is_error: true, .. }));
        // The slot is free again.
        assert!(runtime.running.lock().unwrap().is_empty());
    }

    #[test]
    fn a_plan_proposal_becomes_a_plan_card_and_ends_the_turn() {
        let dir = project();
        let backend = Scripted::new(vec![
            calls(
                Some("Here's my plan."),
                vec![(
                    "propose_plan",
                    json!({"title": "Add a double jump", "steps": ["Add the input", "Change the player"]}),
                )],
            ),
            text("this must never be requested"),
        ]);
        let events = run(backend.clone(), request(dir.path(), "t1", "double jump", node_launch()));
        assert_eq!(backend.seen.lock().unwrap().len(), 1);
        assert!(events.contains(&AgentEvent::PlanProposed {
            title: "Add a double jump".into(),
            steps: vec!["Add the input".into(), "Change the player".into()],
        }));
        // No ToolUse/ToolResult for the plan call, and the turn ended normally.
        assert!(!events.iter().any(|e| matches!(e, AgentEvent::ToolUse { .. } | AgentEvent::ToolResult { .. })));
        assert!(matches!(
            events.last(),
            Some(AgentEvent::TurnCompleted { is_error: false, .. })
        ));
    }

    #[test]
    fn an_invalid_plan_is_an_ordinary_failed_tool_call_and_the_loop_goes_on() {
        let dir = project();
        let backend = Scripted::new(vec![
            calls(None, vec![("propose_plan", json!({"title": "", "steps": []}))]),
            text("Sorry, retrying."),
        ]);
        let events = run(backend.clone(), request(dir.path(), "t1", "x", node_launch()));
        assert_eq!(backend.seen.lock().unwrap().len(), 2);
        assert!(!events.iter().any(|e| matches!(e, AgentEvent::PlanProposed { .. })));
        assert!(events.iter().any(|e| matches!(e,
            AgentEvent::ToolUse { name, summary, .. } if name == "mcp__infinabox__propose_plan" && summary == "Writing up a plan")));
        assert!(events.iter().any(|e| matches!(e, AgentEvent::ToolResult { ok: false, .. })));
    }

    #[test]
    fn without_the_mcp_server_the_turn_continues_with_file_tools_only() {
        let dir = project();
        let backend = Scripted::new(vec![text("fine")]);
        let launch = McpLaunch {
            command: PathBuf::from("/definitely/not/a/program"),
            args: vec![],
            env: vec![],
        };
        let events = run(backend.clone(), request(dir.path(), "t1", "hi", launch));
        assert_eq!(
            events[1],
            AgentEvent::AssistantText {
                text: MCP_UNAVAILABLE.into()
            }
        );
        assert!(matches!(
            events.last(),
            Some(AgentEvent::TurnCompleted { is_error: false, .. })
        ));
        let seen = backend.seen.lock().unwrap();
        assert_eq!(seen[0].2, tools::FILE_TOOL_NAMES.map(String::from).to_vec());
    }

    #[test]
    fn the_thread_so_far_is_rebuilt_into_the_first_messages() {
        let dir = project();
        let thread = chat_store::create_thread(dir.path(), "Chat", "api").unwrap().id;
        let ev = |event| ChatRecord::Event { event, at: 1 };
        for record in [
            ChatRecord::User { text: "make a jumper".into(), at: 1, origin: None },
            ev(AgentEvent::SessionStarted { provider_session_id: "s".into(), model: None }),
            ev(AgentEvent::AssistantText { text: "On it.".into() }),
            ev(AgentEvent::ToolUse { id: "1".into(), name: "Edit".into(), summary: "Editing player.gd".into() }),
            ev(AgentEvent::ToolResult { id: "1".into(), ok: true, summary: "Done".into() }),
            ev(AgentEvent::AssistantText { text: "Made it.".into() }),
            ev(AgentEvent::TurnCompleted { is_error: false, duration_ms: None, usage: None }),
            // The new message, saved by the app before the turn starts.
            ChatRecord::User { text: "now faster".into(), at: 2, origin: None },
        ] {
            chat_store::append(dir.path(), &thread, &record).unwrap();
        }
        let messages = history_messages(dir.path(), &thread, "now faster");
        assert_eq!(
            messages,
            vec![
                Message::User("make a jumper".into()),
                Message::Assistant {
                    text: Some("On it.\n(Editing player.gd)\nMade it.".into()),
                    tool_calls: vec![]
                },
                Message::User("now faster".into()),
            ]
        );
        // A thread that can't be loaded is just the new message.
        assert_eq!(
            history_messages(dir.path(), "missing", "hi"),
            vec![Message::User("hi".into())]
        );
    }

    #[test]
    fn history_is_capped_by_records_and_bytes() {
        let dir = project();
        let thread = chat_store::create_thread(dir.path(), "Chat", "api").unwrap().id;
        for i in 0..100 {
            chat_store::append(
                dir.path(),
                &thread,
                &ChatRecord::User { text: format!("question {i}"), at: 1, origin: None },
            )
            .unwrap();
            chat_store::append(
                dir.path(),
                &thread,
                &ChatRecord::Event { event: AgentEvent::AssistantText { text: format!("answer {i}") }, at: 1 },
            )
            .unwrap();
        }
        let messages = history_messages(dir.path(), &thread, "next");
        // 40 records = 20 exchanges; the last message is the new one.
        assert_eq!(messages.len(), 41);
        assert_eq!(messages[0], Message::User("question 80".into()));
        assert_eq!(messages[39], Message::Assistant { text: Some("answer 99".into()), tool_calls: vec![] });
        assert_eq!(messages[40], Message::User("next".into()));

        let thread = chat_store::create_thread(dir.path(), "Big", "api").unwrap().id;
        for i in 0..6 {
            chat_store::append(
                dir.path(),
                &thread,
                &ChatRecord::User { text: format!("{i}{}", "z".repeat(20 * 1024)), at: 1, origin: None },
            )
            .unwrap();
            chat_store::append(
                dir.path(),
                &thread,
                &ChatRecord::Event { event: AgentEvent::AssistantText { text: "a".into() }, at: 1 },
            )
            .unwrap();
        }
        let messages = history_messages(dir.path(), &thread, "next");
        let bytes: usize = messages
            .iter()
            .map(|m| match m {
                Message::User(t) => t.len(),
                Message::Assistant { text, .. } => text.as_deref().map_or(0, str::len),
                _ => 0,
            })
            .sum();
        assert!(bytes <= HISTORY_MAX_BYTES + 10, "{bytes}");
        assert!(matches!(&messages[0], Message::User(_)));
    }

    #[test]
    fn context_card_paths_stay_in_the_context_folder() {
        assert_eq!(context_card_path("mechanics/jump.md").as_deref(), Some(".ibproject/context/mechanics/jump.md"));
        assert_eq!(context_card_path("../x"), None);
        assert_eq!(context_card_path(""), None);
    }

    /// Against the real InfinaBox MCP server, when `infinabox-cli` has been built.
    #[test]
    #[ignore = "needs the built infinabox-cli"]
    fn the_real_infinabox_server_offers_its_tools() {
        let cli = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/infinabox-cli");
        if !cli.exists() {
            eprintln!("infinabox-cli isn't built; skipping");
            return;
        }
        let dir = project();
        let launch = McpLaunch {
            command: cli,
            args: vec!["mcp-server".into()],
            env: vec![("INFINABOX_PROJECT".into(), dir.path().to_string_lossy().into_owned())],
        };
        let backend = Scripted::new(vec![
            calls(None, vec![("list_context_cards", json!({}))]),
            text("ok"),
        ]);
        let events = run(backend.clone(), request(dir.path(), "t1", "cards?", launch));
        let seen = backend.seen.lock().unwrap();
        assert_eq!(seen[0].2.len(), 5 + 11, "{:?}", seen[0].2);
        assert!(seen[0].2.contains(&"propose_plan".to_string()));
        assert!(events.iter().any(|e| matches!(e,
            AgentEvent::ToolUse { name, .. } if name == "mcp__infinabox__list_context_cards")));
        assert!(matches!(events.last(), Some(AgentEvent::TurnCompleted { is_error: false, .. })));
    }
}
