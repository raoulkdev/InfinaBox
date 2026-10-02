//! `CodexRuntime` tests. The unit tests run a stand-in `codex` (a small
//! shell script replaying a real recording) so the real process plumbing is
//! exercised — spawning, arguments, environment, streaming, cancel —
//! without a network or a signed-in CLI. The `#[ignore]`d test runs the
//! real CLI against the scripted Responses endpoint.

use super::*;
use crate::agent::McpLaunch;
use crate::agent::prompt::DIRECTOR_PROMPT;
use std::path::Path;

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/codex/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {path}: {e}"))
}

fn request(project: &Path, message: &str) -> TurnRequest {
    TurnRequest {
        thread_id: "thread-1".into(),
        project_path: project.to_path_buf(),
        message: message.into(),
        resume_provider_session_id: None,
        mcp: McpLaunch {
            command: PathBuf::from("/Applications/InfinaBox.app/Contents/MacOS/infinabox"),
            args: vec!["--mcp-server".into()],
            env: vec![
                ("INFINABOX_PROJECT".into(), project.display().to_string()),
                ("INFINABOX_BRIDGE_TOKEN".into(), "token-123".into()),
            ],
        },
        options: Default::default(),
    }
}

fn run(runtime: &CodexRuntime, req: TurnRequest) -> Vec<AgentEvent> {
    let mut events = Vec::new();
    runtime
        .run_turn(req, &mut |e| events.push(e))
        .expect("run_turn");
    events
}

#[test]
fn builds_the_expected_arguments() {
    let mut req = request(Path::new("/work/my game"), "-make it blue");
    req.resume_provider_session_id = Some("01a0e7bd-6cf8-7d72-be15-07e549057aec".into());
    let args: Vec<String> = build_args(&req, "Be \"nice\".\nThanks")
        .into_iter()
        .map(|a| a.into_string().unwrap())
        .collect();
    // A resumed thread keeps its first instructions, so this turn's travel
    // with the message.
    let wrapped = prompt::message_with_turn_instructions(&req.options, "-make it blue");
    let expected: Vec<&str> = vec![
        "exec",
        "--json",
        "--skip-git-repo-check",
        // Never the user's own config/MCP servers or a project's .codex/.
        "--ignore-user-config",
        "--ignore-rules",
        "--disable",
        "apps",
        "--disable",
        "plugins",
        "--disable",
        "hooks",
        "--sandbox",
        "workspace-write",
        "-C",
        "/work/my game",
        "-c",
        r#"mcp_servers.infinabox.command="/Applications/InfinaBox.app/Contents/MacOS/infinabox""#,
        "-c",
        r#"mcp_servers.infinabox.args=["--mcp-server"]"#,
        "-c",
        r#"mcp_servers.infinabox.env_vars=["INFINABOX_PROJECT","INFINABOX_BRIDGE_TOKEN"]"#,
        "-c",
        r#"mcp_servers.infinabox.default_tools_approval_mode="approve""#,
        "-c",
        r#"shell_environment_policy.exclude=["INFINABOX_PROJECT","INFINABOX_BRIDGE_TOKEN"]"#,
        "-c",
        r#"developer_instructions="Be \"nice\".\nThanks""#,
        "resume",
        "01a0e7bd-6cf8-7d72-be15-07e549057aec",
        "--",
        wrapped.as_str(),
    ];
    assert_eq!(args, expected);
    // The bridge token's value is never on the command line.
    assert!(!args.iter().any(|a| a.contains("token-123")));

    req.resume_provider_session_id = None;
    let args = build_args(&req, "P");
    assert!(!args.iter().any(|a| a == "resume"));
    assert_eq!(args[args.len() - 2], "--");
    assert_eq!(args[args.len() - 1], "-make it blue");
}

#[test]
fn toml_strings_escape_what_toml_needs() {
    assert_eq!(toml_string("plain"), r#""plain""#);
    assert_eq!(
        toml_string("a\"b\\c\nd\te\r\u{1}\u{7f}é"),
        r#""a\"b\\c\nd\te\r\u0001\u007Fé""#
    );
    assert_eq!(
        toml_string(r"C:\Program Files\x"),
        r#""C:\\Program Files\\x""#
    );
    assert_eq!(toml_list(&["a", "b\""]), r#"["a","b\""]"#);
    assert_eq!(toml_list::<&str>(&[]), "[]");
}

#[test]
fn parses_versions_and_login_status() {
    assert_eq!(
        parse_version(&fixture("version.txt")).as_deref(),
        Some("0.157.1")
    );
    assert_eq!(
        parse_version("WARNING: proceeding, even though...\ncodex-cli 0.157.1\n").as_deref(),
        Some("0.157.1")
    );
    assert_eq!(parse_version(" \n"), None);

    // The recorded `codex login status` outputs (exit code on the first line).
    let recorded = |name: &str| {
        let text = fixture(name);
        let code = text
            .lines()
            .next()
            .and_then(|l| l.strip_prefix("exit: "))
            .and_then(|c| c.trim().parse().ok())
            .unwrap();
        (code, text)
    };
    assert_eq!(
        parse_login_status(recorded("login_status_signed_out.txt")),
        Some(false)
    );
    assert_eq!(
        parse_login_status(recorded("login_status_signed_in.txt")),
        Some(true)
    );
    assert_eq!(parse_login_status((1, "Error: config broken".into())), None);
    assert_eq!(parse_login_status((2, "Not logged in".into())), None);
}

#[test]
fn missing_binary_on_path_is_not_installed() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = CodexRuntime::with_program("definitely-not-a-real-cli-tool-xyz123");
    let status = runtime.detect();
    assert_eq!(
        status,
        RuntimeStatus {
            name: "codex".into(),
            installed: false,
            version: None,
            logged_in: None,
        }
    );
    let events = run(&runtime, request(dir.path(), "hi"));
    assert!(matches!(
        events[0],
        AgentEvent::Error {
            kind: AgentErrorKind::NotInstalled,
            ..
        }
    ));
    assert_eq!(
        events[1],
        AgentEvent::TurnCompleted {
            is_error: true,
            duration_ms: None,
            usage: None
        }
    );
    assert_eq!(events.len(), 2);
}

#[test]
fn missing_binary_at_a_path_is_not_installed() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = CodexRuntime::with_program("/nonexistent/dir/codex");
    assert!(!runtime.detect().installed);
    let events = run(&runtime, request(dir.path(), "hi"));
    assert!(matches!(
        events[0],
        AgentEvent::Error {
            kind: AgentErrorKind::NotInstalled,
            ..
        }
    ));
}

#[test]
fn a_missing_project_folder_is_an_error_before_anything_runs() {
    let runtime = CodexRuntime::with_program("/nonexistent/dir/codex");
    let mut events = Vec::new();
    let err = runtime
        .run_turn(request(Path::new("/nonexistent/project"), "hi"), &mut |e| {
            events.push(e)
        })
        .unwrap_err();
    assert!(err.to_string().contains("doesn't exist"));
    assert!(events.is_empty());
}

#[test]
fn cancel_without_a_running_turn_does_nothing() {
    CodexRuntime::new().cancel("no-such-thread");
}

/// A stand-in `codex` in its own temp dir. `--version` and `login status`
/// answer as recorded (signed in unless `signed_out`); a turn records its
/// arguments (NUL-separated), working directory and the MCP variables it
/// was given next to itself, then runs `body`, which can read
/// `$dir/stream.jsonl`.
#[cfg(unix)]
struct FakeCodex {
    dir: tempfile::TempDir,
    project: tempfile::TempDir,
}

#[cfg(unix)]
impl FakeCodex {
    fn new(body: &str, stream: &str) -> Self {
        Self::with_login(body, stream, "echo 'Logged in using an API key - sk-mock-***-0000'; exit 0")
    }

    fn with_login(body: &str, stream: &str, login: &str) -> Self {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let script = format!(
            r#"#!/bin/sh
dir="$(dirname "$0")"
if [ "$1" = "--version" ]; then echo "codex-cli 0.157.1"; exit 0; fi
if [ "$1" = "login" ] && [ "$2" = "status" ]; then {login}; fi
: > "$dir/args.bin"
for a in "$@"; do printf '%s\0' "$a" >> "$dir/args.bin"; done
pwd -P > "$dir/cwd.txt"
printf '%s|%s' "$INFINABOX_BRIDGE_TOKEN" "$INFINABOX_PROJECT" > "$dir/env.txt"
{body}
"#
        );
        let project_path = project.path().to_string_lossy().into_owned();
        std::fs::write(
            dir.path().join("stream.jsonl"),
            stream.replace("<PROJECT>", &project_path),
        )
        .unwrap();
        let path = dir.path().join("codex");
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self { dir, project }
    }

    fn runtime(&self) -> CodexRuntime {
        CodexRuntime::with_program(self.dir.path().join("codex").to_string_lossy())
    }

    fn read(&self, name: &str) -> String {
        std::fs::read_to_string(self.dir.path().join(name)).unwrap()
    }
}

#[cfg(unix)]
#[test]
fn detect_reads_version_and_sign_in() {
    let fake = FakeCodex::new("exit 0", "");
    assert_eq!(
        fake.runtime().detect(),
        RuntimeStatus {
            name: "codex".into(),
            installed: true,
            version: Some("0.157.1".into()),
            logged_in: Some(true),
        }
    );
    let signed_out = FakeCodex::with_login("exit 0", "", "echo 'Not logged in' >&2; exit 1");
    assert_eq!(signed_out.runtime().detect().logged_in, Some(false));
    let broken = FakeCodex::with_login("exit 0", "", "echo 'Error: bad config' >&2; exit 1");
    assert_eq!(broken.runtime().detect().logged_in, None);
}

#[cfg(unix)]
#[test]
fn detect_gives_up_on_a_hanging_status_command() {
    let fake = FakeCodex::with_login("exit 0", "", "sleep 60");
    let started = Instant::now();
    let status = fake.runtime().detect();
    assert_eq!(status.logged_in, None);
    assert_eq!(status.version.as_deref(), Some("0.157.1"));
    assert!(started.elapsed() < DETECT_TIMEOUT + Duration::from_secs(5));
}

#[cfg(unix)]
#[test]
fn runs_the_cli_and_streams_its_events() {
    let fake = FakeCodex::new(r#"cat "$dir/stream.jsonl""#, &fixture("b_edit_file.jsonl"));
    let message = r#"-Add "world" to notes.txt; echo $HOME `id`"#;
    let events = run(&fake.runtime(), request(fake.project.path(), message));

    // The same events the parser gives for the recording, in order.
    assert!(
        matches!(&events[0], AgentEvent::SessionStarted { provider_session_id, .. }
        if provider_session_id == "01a0e7bd-712b-7950-a641-4b9860bf3ee5")
    );
    assert_eq!(
        &events[events.len() - 2..],
        &[
            AgentEvent::FilesChanged {
                paths: vec!["notes.txt".into()]
            },
            AgentEvent::TurnCompleted {
                is_error: false,
                duration_ms: None,
                usage: Some(crate::agent::Usage {
                    input_tokens: Some(240),
                    output_tokens: Some(24)
                }),
            },
        ]
    );
    assert_eq!(events.len(), 6);

    // Arguments arrive exactly as built (instructions from
    // `prompt::system_prompt`, without AGENTS.md), the message verbatim.
    let args: Vec<String> = fake
        .read("args.bin")
        .split('\0')
        .filter(|a| !a.is_empty())
        .map(String::from)
        .collect();
    let req = request(fake.project.path(), message);
    let expected: Vec<String> = build_args(&req, &system_prompt(&req.options, None))
        .into_iter()
        .map(|a| a.into_string().unwrap())
        .collect();
    assert_eq!(args, expected);
    assert!(args.iter().any(|a| a.contains("developer_instructions=")));
    assert_eq!(args.last().unwrap(), message);

    // Run in the project, with the MCP variables in its environment.
    assert_eq!(
        Path::new(fake.read("cwd.txt").trim()),
        fake.project.path().canonicalize().unwrap()
    );
    assert_eq!(
        fake.read("env.txt"),
        format!("token-123|{}", fake.project.path().display())
    );
}

#[cfg(unix)]
#[test]
fn the_director_prompt_goes_in_the_instructions() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("AGENTS.md"), "PROJECT-CODEWORD").unwrap();
    let req = request(dir.path(), "hi");
    let instructions = system_prompt(&req.options, None);
    assert!(instructions.contains(DIRECTOR_PROMPT.trim()));
    // Codex reads AGENTS.md itself; it isn't repeated here.
    assert!(!instructions.contains("PROJECT-CODEWORD"));
}

#[cfg(unix)]
#[test]
fn a_failing_cli_ends_the_turn_with_its_error_line() {
    let fake = FakeCodex::new(
        r#"cat "$dir/stream.jsonl" >&2; exit 1"#,
        &fixture("j_bad_resume.stderr.txt"),
    );
    let events = run(&fake.runtime(), request(fake.project.path(), "hi"));
    assert_eq!(events.len(), 2);
    assert!(
        matches!(&events[0], AgentEvent::Error { kind: AgentErrorKind::Other, message }
        if message.contains(crate::agent::codex_stream::BAD_RESUME_MARKER))
    );
    assert!(matches!(
        events[1],
        AgentEvent::TurnCompleted { is_error: true, .. }
    ));

    let fake = FakeCodex::new("echo 'boom: the CLI fell over' >&2\nexit 3", "");
    assert_eq!(
        run(&fake.runtime(), request(fake.project.path(), "hi"))[0],
        AgentEvent::Error {
            kind: AgentErrorKind::ProcessFailed,
            message: "boom: the CLI fell over".into(),
        }
    );
}

#[cfg(unix)]
#[test]
fn cancel_stops_the_cli_and_ends_the_turn() {
    // Prints the recorded lines up to the edit, then hangs (in a child
    // process, to check the whole process group is stopped).
    let fake = FakeCodex::new(
        r#"cat "$dir/stream.jsonl"; sleep 60"#,
        &fixture("h_cancelled.jsonl"),
    );
    let runtime = Arc::new(fake.runtime());
    let started = Instant::now();
    let mut events = Vec::new();
    let canceller = runtime.clone();
    runtime
        .run_turn(request(fake.project.path(), "hi"), &mut |e| {
            if matches!(e, AgentEvent::ToolResult { .. }) {
                // From another thread, as the app's cancel command would.
                let canceller = canceller.clone();
                std::thread::spawn(move || canceller.cancel("thread-1"));
            }
            events.push(e);
        })
        .unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "took {:?}",
        started.elapsed()
    );
    assert_eq!(
        &events[events.len() - 3..],
        &[
            AgentEvent::FilesChanged {
                paths: vec!["notes.txt".into()]
            },
            AgentEvent::Error {
                kind: AgentErrorKind::Cancelled,
                message: "Stopped.".into()
            },
            AgentEvent::TurnCompleted {
                is_error: true,
                duration_ms: None,
                usage: None
            },
        ]
    );
    assert!(runtime.running.lock().unwrap().is_empty());
}

#[cfg(unix)]
#[test]
fn a_second_turn_on_the_same_thread_is_refused_while_one_runs() {
    let fake = FakeCodex::new(
        r#"head -n 1 "$dir/stream.jsonl"; sleep 60"#,
        &fixture("a_plain_text.jsonl"),
    );
    let runtime = Arc::new(fake.runtime());
    let project = fake.project.path().to_path_buf();
    let mut refused = None;
    let other = runtime.clone();
    runtime
        .run_turn(request(&project, "hi"), &mut |e| {
            if matches!(e, AgentEvent::SessionStarted { .. }) {
                refused = Some(
                    other
                        .run_turn(request(&project, "again"), &mut |_| {})
                        .is_err(),
                );
                other.cancel("thread-1");
            }
        })
        .unwrap();
    assert_eq!(refused, Some(true));
    assert!(runtime.running.lock().unwrap().is_empty());
}

/// A cancel that arrives after the slot is reserved but before the process
/// is stored is applied as soon as it is.
#[cfg(unix)]
#[test]
fn cancel_before_the_child_is_registered_still_stops_it() {
    use std::os::unix::process::CommandExt;
    let runtime = CodexRuntime::new();
    let guard = RunningGuard::reserve(&runtime.running, "t").unwrap();
    assert!(RunningGuard::reserve(&runtime.running, "t").is_none());
    runtime.cancel("t");
    assert!(guard.was_cancelled());

    let child = Command::new("sleep")
        .arg("60")
        .process_group(0)
        .spawn()
        .unwrap();
    let started = Instant::now();
    guard.slot.set_child(child);
    let status = guard.wait().expect("exit status");
    assert!(!status.success());
    assert!(started.elapsed() < Duration::from_secs(10));
    drop(guard);
    assert!(runtime.running.lock().unwrap().is_empty());
}

/// The real CLI, pointed at `scripts/fixtures/mock-responses-server.mjs`:
/// an edit, a resumed turn, the MCP server getting the bridge token through
/// the environment, a plan, a cancel, and a bad resume id. Needs `codex`
/// (on PATH, or `INFINABOX_CODEX`) and `node`.
///
/// The runtime's own arguments are used unchanged; a small wrapper only adds
/// the mock provider's `-c` settings after `exec` (the runtime deliberately
/// ignores the user's config file, where they'd otherwise go).
#[cfg(unix)]
#[test]
#[ignore = "needs codex and node; run with --ignored"]
fn real_cli_against_the_mock_endpoint() {
    use std::os::unix::fs::PermissionsExt;
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mock = repo
        .join("scripts/fixtures/mock-responses-server.mjs")
        .canonicalize()
        .unwrap();
    let codex = std::env::var("INFINABOX_CODEX").unwrap_or_else(|_| "codex".into());
    let node = find_on_path("node", login_shell_path()).expect("node not found");

    let scratch = tempfile::tempdir().unwrap();
    let log = scratch.path().join("requests.log");
    let mut server = Command::new(&node)
        .arg(&mock)
        .env("MOCK_RESPONSES_LOG", &log)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut first = String::new();
    BufReader::new(server.stdout.take().unwrap())
        .read_line(&mut first)
        .unwrap();
    let port = serde_json::from_str::<serde_json::Value>(&first).unwrap()["port"]
        .as_u64()
        .unwrap();

    // The model catalog entry for the scripted model (as in
    // scripts/record-codex-fixtures.mjs), so Codex offers apply_patch.
    let catalog = scratch.path().join("catalog.json");
    std::fs::write(
        &catalog,
        serde_json::json!({"models": [{
            "slug": "mock-model", "display_name": "Mock", "description": "Scripted",
            "apply_patch_tool_type": "freeform", "shell_type": "shell_command",
            "supported_in_api": true, "visibility": "list", "priority": 1,
            "supported_reasoning_levels": [], "default_reasoning_level": null,
            "base_instructions": "You are a scripted test model.", "support_verbosity": false,
            "truncation_policy": {"mode": "tokens", "limit": 10000},
            "experimental_supported_tools": []
        }]})
        .to_string(),
    )
    .unwrap();
    let home = scratch.path().join("codex-home");
    std::fs::create_dir(&home).unwrap();
    let wrapper = scratch.path().join("codex");
    std::fs::write(
        &wrapper,
        format!(
            r#"#!/bin/sh
[ "$1" = "exec" ] || exec '{codex}' "$@"
shift
export CODEX_HOME='{home}' MOCK_API_KEY=sk-mock NO_PROXY="127.0.0.1,localhost${{NO_PROXY:+,$NO_PROXY}}"
exec '{codex}' exec -c model_provider=mock \
  -c 'model_providers.mock={{name="mock",base_url="http://127.0.0.1:{port}/v1",env_key="MOCK_API_KEY",wire_api="responses"}}' \
  -c 'model_catalog_json="{catalog}"' -m mock-model "$@"
"#,
            home = home.display(),
            catalog = catalog.display(),
        ),
    )
    .unwrap();
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
    let runtime = Arc::new(CodexRuntime::with_program(wrapper.to_string_lossy()));

    // A git project with an AGENTS.md (read by Codex itself) and a
    // `.codex/config.toml` that must not load.
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("notes.txt"), "hello\n").unwrap();
    std::fs::write(
        project.path().join("AGENTS.md"),
        "AGENTS-CODEWORD-PELICAN\n",
    )
    .unwrap();
    std::fs::create_dir(project.path().join(".codex")).unwrap();
    std::fs::write(
        project.path().join(".codex/config.toml"),
        "developer_instructions = \"PROJECT-CONFIG-LOADED\"\n",
    )
    .unwrap();
    git2::Repository::init(project.path()).unwrap();

    let mut req = request(project.path(), "SCENARIO=edit Append the line 'world'.");
    req.mcp = McpLaunch {
        command: node.clone(),
        args: vec![mock.to_string_lossy().into_owned(), "--mcp".into()],
        env: vec![("INFINABOX_BRIDGE_TOKEN".into(), "real-test-token".into())],
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // An edit with Codex's own apply_patch.
        let events = run(&runtime, req.clone());
        eprintln!("{events:#?}");
        let Some(AgentEvent::SessionStarted {
            provider_session_id: thread,
            ..
        }) = events.first().cloned()
        else {
            panic!("first event should be SessionStarted: {events:?}");
        };
        assert_eq!(
            &events[events.len() - 2],
            &AgentEvent::FilesChanged {
                paths: vec!["notes.txt".into()]
            }
        );
        assert!(matches!(
            events.last(),
            Some(AgentEvent::TurnCompleted {
                is_error: false,
                ..
            })
        ));
        assert_eq!(
            std::fs::read_to_string(project.path().join("notes.txt")).unwrap(),
            "hello\nworld\n"
        );
        let sent = std::fs::read_to_string(&log).unwrap();
        assert!(
            sent.contains("AGENTS-CODEWORD-PELICAN"),
            "AGENTS.md not sent"
        );
        assert!(
            !sent.contains("PROJECT-CONFIG-LOADED"),
            "project config loaded"
        );
        let director_line = DIRECTOR_PROMPT
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap();
        let director_json = serde_json::to_string(director_line).unwrap();
        assert!(
            sent.contains(director_json.trim_matches('"')),
            "Director prompt not sent"
        );
        assert!(!sent.contains("real-test-token"));

        // Resumed: same thread, and the earlier reply is in its history.
        let mut resumed = req.clone();
        resumed.message = "SCENARIO=resume What did you say?".into();
        resumed.resume_provider_session_id = Some(thread.clone());
        let events = run(&runtime, resumed);
        eprintln!("{events:#?}");
        assert!(
            matches!(&events[0], AgentEvent::SessionStarted { provider_session_id, .. } if *provider_session_id == thread)
        );
        assert!(events.contains(&AgentEvent::AssistantText {
            text: "Last time I said: I added the line \"world\" to notes.txt.".into()
        }));

        // The MCP server got the bridge token through the environment.
        let mut mcp = req.clone();
        mcp.message = "SCENARIO=mcp".into();
        let events = run(&runtime, mcp);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, AgentEvent::AssistantText { text }
            if text.contains("echo: ping (bridge token received: yes)"))),
            "{events:#?}"
        );

        // A plan.
        let mut plan = req.clone();
        plan.message = "SCENARIO=plan".into();
        let events = run(&runtime, plan);
        assert!(
            events.contains(&AgentEvent::PlanProposed {
                title: "Add a double jump".into(),
                steps: vec![
                    "Let the player jump a second time in mid-air".into(),
                    "Play a small puff effect on the second jump".into(),
                ],
            }),
            "{events:#?}"
        );

        // Stopped mid-stream, after an edit.
        let mut hang = req.clone();
        hang.message = "SCENARIO=hang".into();
        let canceller = runtime.clone();
        let mut events = Vec::new();
        let started = Instant::now();
        runtime
            .run_turn(hang, &mut |e| {
                if matches!(e, AgentEvent::ToolResult { .. }) {
                    let canceller = canceller.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_millis(500));
                        canceller.cancel("thread-1");
                    });
                }
                events.push(e);
            })
            .unwrap();
        eprintln!("{events:#?} after {:?}", started.elapsed());
        assert!(started.elapsed() < Duration::from_secs(30));
        assert_eq!(
            &events[events.len() - 2],
            &AgentEvent::Error {
                kind: AgentErrorKind::Cancelled,
                message: "Stopped.".into()
            }
        );

        // A resume id Codex doesn't know.
        let mut bad = req.clone();
        bad.message = "SCENARIO=text".into();
        bad.resume_provider_session_id = Some("00000000-0000-0000-0000-000000000000".into());
        let events = run(&runtime, bad);
        eprintln!("{events:#?}");
        assert!(
            matches!(&events[0], AgentEvent::Error { kind: AgentErrorKind::Other, message }
            if message.contains(crate::agent::codex_stream::BAD_RESUME_MARKER))
        );
        assert_eq!(events.len(), 2);
    }));
    let _ = server.kill();
    let _ = server.wait();
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

#[test]
fn a_chosen_model_and_effort_reach_codex() {
    use crate::agent::Effort;
    let dir = tempfile::tempdir().unwrap();
    let mut req = request(dir.path(), "hi");
    req.options.model = Some("some-model".into());
    req.options.effort = Some(Effort::Max);
    let args: Vec<String> = build_args(&req, "I").iter().map(|a| a.to_string_lossy().into_owned()).collect();
    let m = args.iter().position(|a| a == "-m").unwrap();
    assert_eq!(args[m + 1], "some-model");
    assert!(args.iter().any(|a| a == "model_reasoning_effort=\"xhigh\""), "{args:?}");
}

#[test]
fn an_ask_turn_runs_in_a_read_only_sandbox() {
    use crate::agent::{TurnMode, TurnOptions};
    let dir = tempfile::tempdir().unwrap();
    let mut req = request(dir.path(), "how does saving work?");
    req.options = TurnOptions { mode: TurnMode::Ask, ..Default::default() };
    let args: Vec<String> = build_args(&req, "INSTRUCTIONS")
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let i = args.iter().position(|a| a == "--sandbox").unwrap();
    assert_eq!(args[i + 1], "read-only");
}
