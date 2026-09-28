//! Parser tests over the real Codex CLI 0.157.1 recordings in
//! `crates/core/tests/fixtures/codex/`, asserting exact event sequences.

use super::*;
use AgentEvent::*;

/// Stands in for the real project path the recordings were scrubbed to
/// `<PROJECT>` from.
const PROJECT: &str = "/work/my-game";

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/codex/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {path}: {e}"))
}

/// Every event a whole recording produces, with the project restored to
/// `PROJECT`.
fn parse(text: &str) -> (Vec<AgentEvent>, CodexStream) {
    let mut stream = CodexStream::new(vec![PathBuf::from(PROJECT)]);
    let events = text
        .replace("<PROJECT>", PROJECT)
        .lines()
        .flat_map(|line| stream.parse_line(line))
        .collect();
    (events, stream)
}

/// A recording parsed as the runtime would: stdout, then `finish` with the
/// recorded stderr and exit code.
fn parse_with_exit(name: &str) -> Vec<AgentEvent> {
    let (mut events, mut stream) = parse(&fixture(&format!("{name}.jsonl")));
    let code = fixture(&format!("{name}.exit.txt")).trim().parse().ok();
    events.extend(stream.finish(StreamEnd::Exited {
        code,
        stderr: fixture(&format!("{name}.stderr.txt")),
    }));
    events
}

fn session(id: &str) -> AgentEvent {
    SessionStarted {
        provider_session_id: id.into(),
        model: None,
    }
}

fn text(t: &str) -> AgentEvent {
    AssistantText { text: t.into() }
}

fn tool(id: &str, name: &str, summary: &str) -> AgentEvent {
    ToolUse {
        id: id.into(),
        name: name.into(),
        summary: summary.into(),
    }
}

fn result(id: &str, ok: bool, summary: &str) -> AgentEvent {
    ToolResult {
        id: id.into(),
        ok,
        summary: summary.into(),
    }
}

fn completed(input: u64, output: u64) -> AgentEvent {
    TurnCompleted {
        is_error: false,
        duration_ms: None,
        usage: Some(Usage {
            input_tokens: Some(input),
            output_tokens: Some(output),
        }),
    }
}

#[test]
fn a_plain_text() {
    let (events, stream) = parse(&fixture("a_plain_text.jsonl"));
    assert_eq!(
        events,
        vec![
            session("01a0e7bd-6cf8-7d72-be15-07e549057aec"),
            text("hello from the fixture"),
            completed(120, 12),
        ]
    );
    assert!(stream.saw_end());
}

#[test]
fn b_edit_file() {
    let (events, _) = parse(&fixture("b_edit_file.jsonl"));
    assert_eq!(
        events,
        vec![
            session("01a0e7bd-712b-7950-a641-4b9860bf3ee5"),
            tool("item_0", "file_change", "Editing notes.txt"),
            result("item_0", true, "Done"),
            text("I added the line \"world\" to notes.txt."),
            FilesChanged {
                paths: vec!["notes.txt".into()]
            },
            completed(240, 24),
        ]
    );
}

#[test]
fn c_resumed_turn_keeps_the_thread_id() {
    let (events, _) = parse(&fixture("c_resumed_turn.jsonl"));
    assert_eq!(
        events,
        vec![
            // The same thread as a_plain_text: a resumed turn reports the
            // id it resumed.
            session("01a0e7bd-6cf8-7d72-be15-07e549057aec"),
            text("Last time I said: hello from the fixture"),
            completed(240, 24),
        ]
    );
}

#[test]
fn d_mcp_tool() {
    let (events, _) = parse(&fixture("d_mcp_tool.jsonl"));
    assert_eq!(
        &events[..3],
        &[
            session("01a0e7bd-7932-75f0-bad7-64abdccacf07"),
            tool("item_0", "mcp__infinabox__echo", "Using echo"),
            result("item_0", true, "Done"),
        ]
    );
    // The mock repeats the tool output Codex handed back, which reached
    // the MCP server's env-forwarded bridge token.
    assert!(matches!(&events[3], AssistantText { text }
        if text.ends_with("Output:echo: ping (bridge token received: yes)")));
    assert_eq!(events[4], completed(240, 24));
    assert_eq!(events.len(), 5);
}

#[test]
fn e_propose_plan_becomes_plan_proposed() {
    let (events, _) = parse(&fixture("e_propose_plan.jsonl"));
    assert_eq!(
        &events[1..],
        &[
            PlanProposed {
                title: "Add a double jump".into(),
                steps: vec![
                    "Let the player jump a second time in mid-air".into(),
                    "Play a small puff effect on the second jump".into(),
                ],
            },
            text("I've shown you the plan. Approve it and I'll build it."),
            completed(240, 24),
        ]
    );
}

#[test]
fn f_malformed_plan_stays_a_tool_use() {
    let (events, _) = parse(&fixture("f_bad_plan.jsonl"));
    assert_eq!(
        &events[1..],
        &[
            tool("item_0", "mcp__infinabox__propose_plan", "Proposing a plan"),
            result("item_0", true, "Done"),
            text("That plan didn't go through."),
            completed(240, 24),
        ]
    );
}

#[test]
fn g_shell_command() {
    let (events, _) = parse(&fixture("g_shell_command.jsonl"));
    assert_eq!(
        &events[1],
        &tool("item_0", "command_execution", "Running a command")
    );
    assert_eq!(&events[2], &result("item_0", true, "Done"));
    assert!(matches!(&events[3], AssistantText { text } if text.ends_with("token-not-visible\n")));
    assert_eq!(events.last(), Some(&completed(240, 24)));
    assert_eq!(events.len(), 5);
}

#[test]
fn h_cancelled_keeps_the_edit_made_before_the_stop() {
    let (mut events, mut stream) = parse(&fixture("h_cancelled.jsonl"));
    assert!(!stream.saw_end());
    events.extend(stream.finish(StreamEnd::Cancelled));
    assert_eq!(
        events,
        vec![
            session("01a0e7bd-890f-72c3-93e6-bdac723de3fd"),
            tool("item_0", "file_change", "Editing notes.txt"),
            result("item_0", true, "Done"),
            FilesChanged {
                paths: vec!["notes.txt".into()]
            },
            Error {
                kind: AgentErrorKind::Cancelled,
                message: "Stopped.".into()
            },
            TurnCompleted {
                is_error: true,
                duration_ms: None,
                usage: None
            },
        ]
    );
    // Nothing more once the turn is closed.
    assert!(stream.finish(StreamEnd::Cancelled).is_empty());
}

#[test]
fn h_cancelled_exits_zero_without_ending_the_turn() {
    // SIGTERM makes Codex exit 0 without a turn.completed, so a stop that
    // wasn't ours still ends as a failed turn, not a silent success.
    let events = parse_with_exit("h_cancelled");
    assert!(matches!(
        &events[events.len() - 2],
        Error { kind: AgentErrorKind::ProcessFailed, message }
        if message == "Codex stopped before finishing the turn (exit code 0)."
    ));
}

#[test]
fn i_unauthorized_is_not_authenticated() {
    let events = parse_with_exit("i_unauthorized");
    assert_eq!(
        &events[1..],
        &[
            Error {
                kind: AgentErrorKind::NotAuthenticated,
                message: "unexpected status 401 Unauthorized: Mock server: no valid credentials \
were sent., url: http://127.0.0.1:<PORT>/v1/responses"
                    .into()
            },
            TurnCompleted {
                is_error: true,
                duration_ms: None,
                usage: None
            },
        ]
    );
}

#[test]
fn j_bad_resume_is_reported_with_its_marker() {
    let events = parse_with_exit("j_bad_resume");
    assert_eq!(
        events,
        vec![
            Error {
                kind: AgentErrorKind::Other,
                message: "thread/resume: thread/resume failed: no rollout found for thread id \
00000000-0000-0000-0000-000000000000 (code -32600)"
                    .into()
            },
            TurnCompleted {
                is_error: true,
                duration_ms: None,
                usage: None
            },
        ]
    );
    let Error { message, .. } = &events[0] else {
        unreachable!()
    };
    assert!(message.contains(BAD_RESUME_MARKER));
}

#[test]
fn k_not_logged_in_ends_with_the_last_retry_notice() {
    // No network here, so the signed-out CLI retried until it was stopped;
    // the non-fatal `error` item and retry notices produce no events.
    let (events, mut stream) = parse(&fixture("k_not_logged_in.jsonl"));
    assert_eq!(events.len(), 1);
    assert!(matches!(&events[0], SessionStarted { .. }));
    let end = stream.finish(StreamEnd::Exited {
        code: Some(0),
        stderr: String::new(),
    });
    assert!(matches!(
        &end[0],
        Error { kind: AgentErrorKind::ProcessFailed, message }
        if message.starts_with("Reconnecting... waiting for network")
    ));
}

#[test]
fn every_recording_ends_with_turn_completed() {
    for name in [
        "a_plain_text",
        "b_edit_file",
        "c_resumed_turn",
        "d_mcp_tool",
        "e_propose_plan",
        "f_bad_plan",
        "g_shell_command",
        "h_cancelled",
        "i_unauthorized",
        "j_bad_resume",
        "k_not_logged_in",
    ] {
        let events = parse_with_exit(name);
        assert!(
            matches!(events.last(), Some(TurnCompleted { .. })),
            "{name}: {events:?}"
        );
        let completions = events
            .iter()
            .filter(|e| matches!(e, TurnCompleted { .. }))
            .count();
        assert_eq!(completions, 1, "{name}");
    }
}

#[test]
fn lines_after_the_end_are_ignored() {
    let mut stream = CodexStream::new(vec![PathBuf::from(PROJECT)]);
    let lines = fixture("a_plain_text.jsonl");
    for line in lines.lines() {
        stream.parse_line(line);
    }
    assert!(stream.parse_line(lines.lines().nth(2).unwrap()).is_empty());
    assert!(
        stream
            .finish(StreamEnd::Exited {
                code: Some(1),
                stderr: "boom".into()
            })
            .is_empty()
    );
}

/// A plan the server refused is shown as the failed tool call it was.
/// (Built from the e_propose_plan recording's item, with the error Codex
/// reports for a failed MCP call, as in the d_mcp_tool-shaped refusal
/// "MCP tool call requires approval, but approval policy is never" seen
/// while checking the approval flags.)
#[test]
fn a_refused_plan_is_a_failed_tool_call() {
    let recording = fixture("e_propose_plan.jsonl");
    let completed_line = recording
        .lines()
        .find(|l| l.contains("item.completed") && l.contains("propose_plan"))
        .unwrap();
    let mut msg: Value = serde_json::from_str(completed_line).unwrap();
    msg["item"]["status"] = "failed".into();
    msg["item"]["result"] = Value::Null;
    msg["item"]["error"] = serde_json::json!({"message": "MCP tool call requires approval, but approval policy is never"});
    let mut stream = CodexStream::new(vec![PathBuf::from(PROJECT)]);
    let started = recording
        .lines()
        .find(|l| l.contains("item.started"))
        .unwrap();
    assert!(stream.parse_line(started).is_empty());
    assert_eq!(
        stream.parse_line(&msg.to_string()),
        vec![
            tool("item_0", "mcp__infinabox__propose_plan", "Proposing a plan"),
            result(
                "item_0",
                false,
                "MCP tool call requires approval, but approval policy is never"
            ),
        ]
    );
}

#[test]
fn plan_input_is_validated() {
    let item = |args: Value| {
        serde_json::json!({"type": "mcp_tool_call", "server": "infinabox",
            "tool": "propose_plan", "arguments": args})
    };
    assert_eq!(
        plan_call(&item(
            serde_json::json!({"title": " Jump ", "steps": ["a", " b "]})
        )),
        Some(("Jump".into(), vec!["a".into(), "b".into()]))
    );
    for bad in [
        serde_json::json!({"title": "Jump", "steps": "a"}),
        serde_json::json!({"title": "Jump", "steps": []}),
        serde_json::json!({"title": "Jump", "steps": ["a", ""]}),
        serde_json::json!({"title": "Jump", "steps": ["a", 3]}),
        serde_json::json!({"title": "", "steps": ["a"]}),
        serde_json::json!({"steps": ["a"]}),
    ] {
        assert_eq!(plan_call(&item(bad.clone())), None, "{bad}");
    }
    // Only the InfinaBox server's tool.
    let mut other = item(serde_json::json!({"title": "Jump", "steps": ["a"]}));
    other["server"] = "fixture".into();
    assert_eq!(plan_call(&other), None);
}

#[test]
fn paths_outside_the_project_are_not_counted() {
    let stream = CodexStream::new(vec![PathBuf::from(PROJECT)]);
    let item = serde_json::json!({"type": "file_change", "changes": [
        {"path": "/work/my-game/scenes/main.tscn", "kind": "add"},
        {"path": "/etc/passwd", "kind": "update"},
        {"path": "/work/my-game/../other/x", "kind": "update"},
    ]});
    assert_eq!(
        stream.written_paths("file_change", &item),
        vec!["scenes/main.tscn".to_string()]
    );
    assert_eq!(stream.summarize("file_change", &item), "Editing 3 files");
}

#[test]
fn context_card_writes_count_as_changed_files() {
    let stream = CodexStream::new(vec![PathBuf::from(PROJECT)]);
    let item = serde_json::json!({"type": "mcp_tool_call", "server": "infinabox",
        "tool": "write_context_card", "arguments": {"path": "mechanics/jump.md"}});
    assert_eq!(
        stream.written_paths("mcp_tool_call", &item),
        vec![".ibproject/context/mechanics/jump.md".to_string()]
    );
    assert_eq!(
        stream.summarize("mcp_tool_call", &item),
        "Updating the mechanics/jump Context card"
    );
}

#[test]
fn classifies_errors() {
    assert_eq!(
        classify_error("unexpected status 401 Unauthorized: x"),
        AgentErrorKind::NotAuthenticated
    );
    assert_eq!(
        classify_error("unexpected status 429 Too Many Requests: slow down"),
        AgentErrorKind::RateLimited
    );
    assert_eq!(
        classify_error("thread/resume failed: no rollout found for thread id x"),
        AgentErrorKind::Other
    );
}

#[test]
fn stderr_error_finds_the_clis_error_line() {
    assert_eq!(
        stderr_error(&fixture("j_bad_resume.stderr.txt")).as_deref(),
        Some(
            "thread/resume: thread/resume failed: no rollout found for thread id \
00000000-0000-0000-0000-000000000000 (code -32600)"
        )
    );
    // Only routine notices: nothing to show.
    assert_eq!(stderr_error(&fixture("a_plain_text.stderr.txt")), None);
    assert_eq!(stderr_error("boom\n").as_deref(), Some("boom"));
}
