//! Parser tests over the real Claude Code 2.1.283 recordings in
//! `crates/core/tests/fixtures/claude/` (plus one real auth-failure
//! recording in `agent/testdata/`), asserting exact event sequences.

use super::*;
use AgentEvent::*;

/// Stands in for the real project path the recordings were scrubbed to
/// `<PROJECT>` from.
const PROJECT: &str = "/work/my-game";

fn fixture(name: &str) -> String {
    let path = format!(
        "{}/tests/fixtures/claude/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {path}: {e}"))
}

/// Every event a whole recording produces, with the project restored to
/// `PROJECT`.
fn parse(text: &str) -> (Vec<AgentEvent>, ClaudeStream) {
    let mut stream = ClaudeStream::new(vec![PathBuf::from(PROJECT)]);
    let events = text
        .replace("<PROJECT>", PROJECT)
        .lines()
        .flat_map(|line| stream.parse_line(line))
        .collect();
    (events, stream)
}

fn usage(input: u64, output: u64) -> Option<Usage> {
    Some(Usage {
        input_tokens: Some(input),
        output_tokens: Some(output),
    })
}

fn session(id: &str) -> AgentEvent {
    SessionStarted {
        provider_session_id: id.into(),
        model: Some("claude-sonnet-5".into()),
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

#[test]
fn a_plain_text() {
    let (events, stream) = parse(&fixture("a_plain_text.jsonl"));
    assert_eq!(
        events,
        vec![
            session("347f9f83-2e94-48b0-b592-32f6aeeff4b9"),
            text("hello from the fixture"),
            TurnCompleted {
                is_error: false,
                duration_ms: Some(1760),
                usage: usage(2, 9)
            },
        ]
    );
    assert!(stream.saw_result());
}

#[test]
fn b_edit_file() {
    let (events, _) = parse(&fixture("b_edit_file.jsonl"));
    assert_eq!(
        events,
        vec![
            session("74eec361-ca75-4d9f-82ed-7b0b6aa6a735"),
            tool(
                "toolu_01CkW7tFZY6HS5eFPWenmGXM",
                "Bash",
                "Check if notes.txt exists"
            ),
            result("toolu_01CkW7tFZY6HS5eFPWenmGXM", true, "Done"),
            tool(
                "toolu_016FxYPApVcSvAK44DvoxKRr",
                "Bash",
                "Append 'world' line to notes.txt"
            ),
            result(
                "toolu_016FxYPApVcSvAK44DvoxKRr",
                false,
                // The project's absolute path is taken out of the real text.
                "Output redirection to 'notes.txt' needs approval. The path is inside the \
working directories for this session ('my-game'), and Claude Code asks before a shell command \
creates, changes or removes file…",
            ),
            tool(
                "toolu_01SAmyqSjZ2VxSz5Tq9wg17Q",
                "Read",
                "Reading notes.txt"
            ),
            result("toolu_01SAmyqSjZ2VxSz5Tq9wg17Q", true, "Done"),
            tool(
                "toolu_013ytHZAFUEjTq9EdmAt1JVr",
                "Edit",
                "Editing notes.txt"
            ),
            result("toolu_013ytHZAFUEjTq9EdmAt1JVr", true, "Done"),
            text("Appended \"world\" to notes.txt."),
            FilesChanged {
                paths: vec!["notes.txt".into()]
            },
            TurnCompleted {
                is_error: false,
                duration_ms: Some(6986),
                usage: usage(10, 524)
            },
        ]
    );
}

#[test]
fn c_resumed_turn_keeps_the_session_id() {
    let (events, _) = parse(&fixture("c_resumed_turn.jsonl"));
    assert_eq!(
        events,
        vec![
            // Same id as a_plain_text: the resumed conversation.
            session("347f9f83-2e94-48b0-b592-32f6aeeff4b9"),
            text("I said \"hello from the fixture.\""),
            TurnCompleted {
                is_error: false,
                duration_ms: Some(1309),
                usage: usage(2, 13)
            },
        ]
    );
}

#[test]
fn d_mcp_tool_hides_tool_search() {
    let (events, _) = parse(&fixture("d_mcp_tool.jsonl"));
    assert_eq!(
        events,
        vec![
            session("a303ea73-981e-4915-97ec-4a3199415d81"),
            tool(
                "toolu_01XLpeKgS5yrxFdBV1RM1Ztk",
                "mcp__fixture__echo",
                "Using echo"
            ),
            result("toolu_01XLpeKgS5yrxFdBV1RM1Ztk", true, "Done"),
            text("The echo tool returned: `echo: ping`"),
            TurnCompleted {
                is_error: false,
                duration_ms: Some(7372),
                usage: usage(6, 180)
            },
        ]
    );
}

#[test]
fn e_bad_resume_is_an_other_error_with_the_real_text() {
    let (events, mut stream) = parse(&fixture("e_bad_resume.jsonl"));
    assert_eq!(
        events,
        vec![
            Error {
                kind: AgentErrorKind::Other,
                message: "No conversation found with session ID: \
00000000-0000-0000-0000-000000000000"
                    .into(),
            },
            TurnCompleted {
                is_error: true,
                duration_ms: Some(0),
                usage: usage(0, 0)
            },
        ]
    );
    // The CLI exited 1 but did send a result: nothing more to add.
    let stderr = fixture("e_bad_resume.stderr.txt");
    assert_eq!(
        stream.finish(StreamEnd::Exited {
            code: Some(1),
            stderr
        }),
        vec![]
    );
}

#[test]
fn e_bad_resume_stderr_classifies_as_other() {
    assert_eq!(
        classify_error(&fixture("e_bad_resume.stderr.txt"), None),
        AgentErrorKind::Other
    );
}

/// Recorded from the real CLI (2.1.283) with an invalid API key: ten
/// `api_retry`s with `error: authentication_failed`, then an assistant
/// message flagged `error`/`is_api_error_message`, then an error result
/// with `api_error_status: 401`. The first nine retries are trimmed.
#[test]
fn auth_failure_is_not_authenticated_and_not_shown_twice() {
    let (events, _) = parse(include_str!("../testdata/auth_failure_tail.jsonl"));
    assert_eq!(
        events,
        vec![
            Error {
                kind: AgentErrorKind::NotAuthenticated,
                message: "Failed to authenticate. API Error: 401 API key is invalid.".into(),
            },
            TurnCompleted {
                is_error: true,
                duration_ms: Some(175111),
                usage: usage(0, 0)
            },
        ]
    );
}

#[test]
fn every_fixture_ends_with_exactly_one_turn_completed() {
    for name in [
        "a_plain_text",
        "b_edit_file",
        "c_resumed_turn",
        "d_mcp_tool",
        "e_bad_resume",
    ] {
        let (events, _) = parse(&fixture(&format!("{name}.jsonl")));
        let completed = events
            .iter()
            .filter(|e| matches!(e, TurnCompleted { .. }))
            .count();
        assert_eq!(completed, 1, "{name}");
        assert!(
            matches!(events.last(), Some(TurnCompleted { .. })),
            "{name}"
        );
    }
}

#[test]
fn nothing_after_the_result_is_emitted() {
    let (_, mut stream) = parse(&fixture("a_plain_text.jsonl"));
    let text_line = fixture("a_plain_text.jsonl")
        .lines()
        .nth(1)
        .unwrap()
        .to_string();
    assert_eq!(stream.parse_line(&text_line), vec![]);
}

#[test]
fn failure_summaries_drop_the_project_path_longest_root_first() {
    let mut stream = ClaudeStream::new(vec![
        PathBuf::from("/tmp/game"),
        PathBuf::from("/private/tmp/game"),
    ]);
    stream.parse_line(&edit_line("1", "Edit", r#"{"file_path":"/tmp/game/a.gd"}"#));
    let line = r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"1","is_error":true,"content":[{"type":"text","text":"<tool_use_error>File /private/tmp/game/a.gd not found in /private/tmp/game</tool_use_error>"}]}]}}"#;
    assert_eq!(
        stream.parse_line(line),
        vec![result("1", false, "File a.gd not found in game")]
    );
}

#[test]
fn ignores_unknown_and_malformed_lines() {
    let mut stream = ClaudeStream::new(vec![PathBuf::from(PROJECT)]);
    for line in [
        "",
        "   ",
        "not json at all",
        r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed"}}"#,
        r#"{"type":"system","subtype":"thinking_tokens","estimated_tokens":50}"#,
        r#"{"type":"system","subtype":"something_new"}"#,
        r#"{"type":"brand_new_type","data":1}"#,
        r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"hmm"}]}}"#,
        r#"{"type":"user","message":{"content":"a plain string"}}"#,
    ] {
        assert_eq!(stream.parse_line(line), vec![], "{line}");
    }
}

// --- Files changed ---

fn edit_line(id: &str, name: &str, input: &str) -> String {
    format!(
        r#"{{"type":"assistant","message":{{"content":[{{"type":"tool_use","id":"{id}","name":"{name}","input":{input}}}]}}}}"#
    )
}

fn result_line(id: &str, is_error: bool) -> String {
    format!(
        r#"{{"type":"user","message":{{"content":[{{"type":"tool_result","tool_use_id":"{id}","content":"x","is_error":{is_error}}}]}}}}"#
    )
}

const RESULT_LINE: &str = r#"{"type":"result","is_error":false,"duration_ms":5}"#;

#[test]
fn files_changed_lists_successful_writes_once_relative_to_the_project() {
    let mut stream = ClaudeStream::new(vec![
        PathBuf::from("/tmp/game"),
        PathBuf::from("/private/tmp/game"),
    ]);
    let lines = [
        edit_line(
            "1",
            "Write",
            r#"{"file_path":"/tmp/game/scripts/player.gd"}"#,
        ),
        result_line("1", false),
        // Reported under the canonical form of the project path.
        edit_line(
            "2",
            "Edit",
            r#"{"file_path":"/private/tmp/game/main.tscn"}"#,
        ),
        result_line("2", false),
        // Same file again, via a `./` path: listed once.
        edit_line("3", "MultiEdit", r#"{"file_path":"/tmp/game/./main.tscn"}"#),
        result_line("3", false),
        // A failed edit changed nothing.
        edit_line("4", "Edit", r#"{"file_path":"/tmp/game/broken.gd"}"#),
        result_line("4", true),
        // Outside the project: not part of it.
        edit_line("5", "Write", r#"{"file_path":"/etc/hosts"}"#),
        result_line("5", false),
        edit_line("6", "Edit", r#"{"file_path":"/tmp/game/../other/x.gd"}"#),
        result_line("6", false),
        // Reads don't change anything.
        edit_line("7", "Read", r#"{"file_path":"/tmp/game/readme.md"}"#),
        result_line("7", false),
        edit_line(
            "8",
            "NotebookEdit",
            r#"{"notebook_path":"/tmp/game/notes.ipynb"}"#,
        ),
        result_line("8", false),
        // The InfinaBox Context tool writes under .ibproject/context/.
        edit_line(
            "9",
            "mcp__infinabox__write_context_card",
            r#"{"path":"characters/boss.md","markdown":"x"}"#,
        ),
        result_line("9", false),
    ];
    let mut events: Vec<AgentEvent> = lines.iter().flat_map(|l| stream.parse_line(l)).collect();
    assert!(
        !events.iter().any(|e| matches!(e, FilesChanged { .. })),
        "only at the end"
    );
    events.extend(stream.parse_line(RESULT_LINE));
    let tail = &events[events.len() - 2..];
    assert_eq!(
        tail,
        &[
            FilesChanged {
                paths: vec![
                    "scripts/player.gd".into(),
                    "main.tscn".into(),
                    "notes.ipynb".into(),
                    ".ibproject/context/characters/boss.md".into(),
                ]
            },
            TurnCompleted {
                is_error: false,
                duration_ms: Some(5),
                usage: None
            },
        ]
    );
}

#[test]
fn no_files_changed_event_when_nothing_was_written() {
    let (events, _) = parse(&fixture("a_plain_text.jsonl"));
    assert!(!events.iter().any(|e| matches!(e, FilesChanged { .. })));
}

// --- Turns that end without a result ---

/// `b_edit_file` cut off right after the successful Edit's result.
fn b_up_to_the_edit() -> ClaudeStream {
    let full = fixture("b_edit_file.jsonl");
    let edit_result = full
        .lines()
        .position(|l| l.contains(r#""tool_use_id":"toolu_013ytHZAFUEjTq9EdmAt1JVr""#))
        .expect("the Edit's result line");
    let cut: Vec<&str> = full.lines().take(edit_result + 1).collect();
    let (events, stream) = parse(&cut.join("\n"));
    assert!(matches!(events.last(), Some(ToolResult { ok: true, .. })));
    stream
}

#[test]
fn cancel_ends_the_turn_and_still_reports_the_edits_made() {
    let mut stream = b_up_to_the_edit();
    assert_eq!(
        stream.finish(StreamEnd::Cancelled),
        vec![
            FilesChanged {
                paths: vec!["notes.txt".into()]
            },
            Error {
                kind: AgentErrorKind::Other,
                message: STOPPED_MESSAGE.into()
            },
            TurnCompleted {
                is_error: true,
                duration_ms: None,
                usage: None
            },
        ]
    );
    // Only once.
    assert_eq!(stream.finish(StreamEnd::Cancelled), vec![]);
}

#[test]
fn nonzero_exit_without_result_is_process_failed_with_the_real_stderr() {
    let mut stream = b_up_to_the_edit();
    let events = stream.finish(StreamEnd::Exited {
        code: Some(1),
        stderr: "\nError: something broke inside the CLI\n".into(),
    });
    assert_eq!(
        &events[1..],
        &[
            Error {
                kind: AgentErrorKind::ProcessFailed,
                message: "Error: something broke inside the CLI".into(),
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
fn exit_without_result_or_stderr_says_so_with_the_exit_code() {
    let mut stream = ClaudeStream::new(vec![]);
    assert_eq!(
        stream.finish(StreamEnd::Exited {
            code: Some(137),
            stderr: String::new()
        })[0],
        Error {
            kind: AgentErrorKind::ProcessFailed,
            message: "Claude Code stopped before finishing the turn (exit code 137).".into(),
        }
    );
}

#[test]
fn exit_after_auth_retries_uses_the_recorded_error_kind() {
    let mut stream = ClaudeStream::new(vec![]);
    let retry = include_str!("../testdata/auth_failure_tail.jsonl")
        .lines()
        .next()
        .unwrap();
    assert_eq!(stream.parse_line(retry), vec![]);
    let events = stream.finish(StreamEnd::Exited {
        code: Some(1),
        stderr: String::new(),
    });
    assert!(matches!(
        events[0],
        Error {
            kind: AgentErrorKind::NotAuthenticated,
            ..
        }
    ));
}

// --- Error classification ---

#[test]
fn classifies_auth_and_rate_limit_conservatively() {
    use AgentErrorKind::*;
    assert_eq!(
        classify_error(
            "Failed to authenticate. API Error: 401 API key is invalid.",
            None
        ),
        NotAuthenticated
    );
    assert_eq!(
        classify_error("Not logged in · Please run /login", None),
        NotAuthenticated
    );
    assert_eq!(classify_error("anything", Some(401)), NotAuthenticated);
    assert_eq!(
        classify_error("Claude AI usage limit reached", None),
        RateLimited
    );
    assert_eq!(classify_error("anything", Some(429)), RateLimited);
    assert_eq!(
        classify_error("API Error: 500 Internal server error", Some(500)),
        Other
    );
    assert_eq!(
        classify_error("No conversation found with session ID: x", None),
        Other
    );
}

// --- Summaries ---

#[test]
fn summaries_are_plain_and_never_dump_input() {
    let stream = ClaudeStream::new(vec![PathBuf::from(PROJECT)]);
    let s = |name: &str, input: serde_json::Value| stream.summarize(name, &input);
    use serde_json::json;
    assert_eq!(
        s(
            "Write",
            json!({"file_path": "/work/my-game/scenes/level.tscn", "content": "secret"})
        ),
        "Writing scenes/level.tscn"
    );
    assert_eq!(
        s("Read", json!({"file_path": "/elsewhere/deep/file.txt"})),
        "Reading file.txt"
    );
    assert_eq!(
        s("Glob", json!({"pattern": "**/*.gd"})),
        "Looking for files"
    );
    assert_eq!(
        s("Grep", json!({"pattern": "velocity"})),
        "Searching the project"
    );
    assert_eq!(
        s("Bash", json!({"command": "rm -rf x"})),
        "Running a command"
    );
    assert_eq!(s("mcp__infinabox__run_game", json!({})), "Running the game");
    assert_eq!(
        s("mcp__infinabox__get_game_errors", json!({"limit": 5})),
        "Checking the game for errors"
    );
    assert_eq!(
        s(
            "mcp__infinabox__write_context_card",
            json!({"path": "characters/boss.md", "markdown": "x"})
        ),
        "Updating the characters/boss Context card"
    );
    assert_eq!(s("SomethingNew", json!({"x": 1})), "Using SomethingNew");
}
