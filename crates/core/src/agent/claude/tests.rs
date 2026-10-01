//! `ClaudeCodeRuntime` tests. The unit tests run a stand-in `claude` (a
//! small shell script replaying a real recording) so the real process
//! plumbing is exercised — spawning, arguments, streaming, cancel — without
//! a network or a signed-in CLI. The `#[ignore]`d test runs the real one.

use super::*;
use crate::agent::Role;
use std::path::Path;

fn fixture(name: &str) -> String {
    let path = format!(
        "{}/tests/fixtures/claude/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
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

fn run(runtime: &ClaudeCodeRuntime, req: TurnRequest) -> Vec<AgentEvent> {
    let mut events = Vec::new();
    runtime
        .run_turn(req, &mut |e| events.push(e))
        .expect("run_turn");
    events
}

#[test]
fn builds_the_expected_arguments() {
    let dir = tempfile::tempdir().unwrap();
    let mut req = request(dir.path(), "-make it blue");
    req.resume_provider_session_id = Some("sess-1".into());
    let args: Vec<String> = build_args(&req, Path::new("/tmp/mcp.json"), "PROMPT")
        .into_iter()
        .map(|a| a.into_string().unwrap())
        .collect();
    let wrapped = prompt::message_with_turn_instructions(&req.options, "-make it blue");
    let expected: Vec<String> = [
        "-p",
        "--output-format",
        "stream-json",
        "--verbose",
        // Never load the project's own settings (hooks, env, apiKeyHelper).
        "--setting-sources",
        "user",
        "--resume",
        "sess-1",
        "--mcp-config",
        "/tmp/mcp.json",
        "--strict-mcp-config",
        "--tools",
        "Read,Edit,Write,Glob,Grep",
        "--allowedTools",
        "Read,Edit,Write,Glob,Grep,mcp__infinabox__*",
        "--permission-mode",
        "acceptEdits",
        "--append-system-prompt",
        "PROMPT",
        "--",
        wrapped.as_str(),
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert_eq!(args, expected);

    req.resume_provider_session_id = None;
    let args = build_args(&req, Path::new("/tmp/mcp.json"), "PROMPT");
    assert!(!args.iter().any(|a| a == "--resume"));
    assert_eq!(args.last().unwrap(), "-make it blue");
}

#[test]
fn a_resumed_turn_carries_its_own_instructions_in_the_message() {
    use crate::agent::{MessageOrigin, PlanPolicy, TurnOptions};
    // The CLI ignores `--append-system-prompt` on `--resume` (checked with
    // the real CLI: a resumed conversation kept its first prompt), so a
    // first build's "no plan" would otherwise rule every later turn.
    let dir = tempfile::tempdir().unwrap();
    let mut req = request(dir.path(), "add a double jump");
    req.resume_provider_session_id = Some("sess-1".into());
    req.options = TurnOptions {
        role: Role::Director,
        plan_policy: PlanPolicy::AlwaysPlan,
        teach: true,
        origin: MessageOrigin::User,
        model: None,
        effort: None,
        technical_level: Default::default(),
    };
    let args = build_args(&req, Path::new("/tmp/mcp.json"), "PROMPT");
    let message = args.last().unwrap().to_str().unwrap();
    assert!(message.starts_with("<infinabox-instructions>\n"), "{message}");
    assert!(message.contains("## Plans come first"), "{message}");
    assert!(message.contains("## Teach me"), "{message}");
    assert!(!message.contains(prompt::DIRECTOR_PROMPT.trim_end()));
    assert!(message.ends_with("</infinabox-instructions>\n\nadd a double jump"), "{message}");
    assert_eq!(
        prompt::system_prompt(&req.options, None),
        format!("{}\n\n{}", prompt::DIRECTOR_PROMPT.trim_end(), prompt::turn_instructions(&req.options))
    );
}

#[test]
fn system_prompt_follows_the_turn_options_and_appends_the_projects_agents_md() {
    use crate::agent::{MessageOrigin, PlanPolicy, TurnOptions};
    let dir = tempfile::tempdir().unwrap();
    let mut req = request(dir.path(), "hi");
    assert_eq!(
        turn_system_prompt(&req),
        prompt::system_prompt(&TurnOptions::default(), None)
    );
    std::fs::write(dir.path().join("AGENTS.md"), "# My game\nUse tabs.\n").unwrap();
    req.options = TurnOptions {
        role: Role::Director,
        plan_policy: PlanPolicy::SmallChangesDirect,
        teach: true,
        origin: MessageOrigin::AutoFix,
        model: None,
        effort: None,
        technical_level: Default::default(),
    };
    let prompt = turn_system_prompt(&req);
    assert_eq!(
        prompt,
        prompt::system_prompt(&req.options, Some("# My game\nUse tabs.\n"))
    );
    assert!(prompt.starts_with(prompt::DIRECTOR_PROMPT.trim_end()));
    assert!(prompt.contains("## This message: the game hit errors"));
    assert!(prompt.contains("## Teach me"));
    assert!(prompt.ends_with("# This project's AGENTS.md\n\n# My game\nUse tabs.\n"));
}

#[test]
fn sweeps_only_old_files_with_our_exact_name_pattern() {
    let dir = tempfile::tempdir().unwrap();
    let old = std::time::SystemTime::now() - Duration::from_secs(2 * 24 * 60 * 60);
    let ours_old = format!("infinabox-mcp-{}.json", uuid::Uuid::new_v4());
    let ours_new = format!("infinabox-mcp-{}.json", uuid::Uuid::new_v4());
    let names = [
        ours_old.as_str(),
        ours_new.as_str(),
        "infinabox-mcp-not-a-uuid.json",
        "someone-elses.json",
    ];
    for name in names {
        let file = std::fs::File::create(dir.path().join(name)).unwrap();
        if name != ours_new {
            file.set_modified(old).unwrap();
        }
    }
    sweep_stale_configs(dir.path(), STALE_TEMP_CONFIG_AGE);
    for name in names {
        assert_eq!(dir.path().join(name).exists(), name != ours_old, "{name}");
    }
}

#[test]
fn mcp_config_registers_the_infinabox_server() {
    let dir = tempfile::tempdir().unwrap();
    let req = request(dir.path(), "hi");
    let config = mcp_config_json(&req.mcp);
    let server = &config["mcpServers"]["infinabox"];
    assert_eq!(
        server["command"],
        "/Applications/InfinaBox.app/Contents/MacOS/infinabox"
    );
    assert_eq!(server["args"], serde_json::json!(["--mcp-server"]));
    assert_eq!(server["env"]["INFINABOX_BRIDGE_TOKEN"], "token-123");
}

#[test]
fn parses_versions() {
    assert_eq!(
        parse_version("2.1.283 (Claude Code)\n").as_deref(),
        Some("2.1.283")
    );
    assert_eq!(
        parse_version("\nclaude dev build\n").as_deref(),
        Some("claude dev build")
    );
    assert_eq!(parse_version("  \n"), None);
}

#[test]
fn parses_the_auth_status_json() {
    // The real 2.1.283 output (directories shortened).
    let real = r#"{
  "loggedIn": true,
  "authMethod": "oauth_token",
  "apiProvider": "firstParty",
  "analyticsDisabled": false,
  "projectsDirectory": "<HOME>/.claude/projects",
  "configDirectory": "<HOME>/.claude"
}
"#;
    assert_eq!(parse_logged_in(real), Some(true));
    assert_eq!(parse_logged_in(r#"{"loggedIn": false}"#), Some(false));
    for unknown in [
        "",
        "Login method: Claude API account",
        r#"{"loggedIn": "yes"}"#,
        r#"{"authMethod": "none"}"#,
        "[true]",
    ] {
        assert_eq!(parse_logged_in(unknown), None, "{unknown}");
    }
}

#[test]
fn missing_binary_on_path_is_not_installed() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = ClaudeCodeRuntime::with_program("definitely-not-a-real-cli-tool-xyz123");
    let status = runtime.detect();
    assert!(!status.installed);
    assert_eq!(status.version, None);
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
    let runtime = ClaudeCodeRuntime::with_program("/nonexistent/dir/claude");
    assert!(!runtime.detect().installed);
    let events = run(&runtime, request(dir.path(), "hi"));
    assert!(matches!(
        events[0],
        AgentEvent::Error {
            kind: AgentErrorKind::NotInstalled,
            ..
        }
    ));
    assert!(matches!(
        events[1],
        AgentEvent::TurnCompleted { is_error: true, .. }
    ));
}

#[test]
fn a_missing_project_folder_is_an_error_before_anything_runs() {
    let runtime = ClaudeCodeRuntime::with_program("/nonexistent/dir/claude");
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
    ClaudeCodeRuntime::new().cancel("no-such-thread");
}

/// A stand-in `claude` in its own temp dir. It records its arguments
/// (NUL-separated), working directory and MCP config next to itself, then
/// runs `body`, which can read `$dir/stream.jsonl`. `claude auth ...` runs
/// `$dir/auth.sh` if a test wrote one, else fails with no output.
#[cfg(unix)]
struct FakeClaude {
    dir: tempfile::TempDir,
    project: tempfile::TempDir,
}

#[cfg(unix)]
impl FakeClaude {
    fn new(body: &str, stream: &str) -> Self {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let script = format!(
            r#"#!/bin/sh
dir="$(dirname "$0")"
if [ "$1" = "--version" ]; then echo "2.1.283 (Claude Code)"; exit 0; fi
if [ "$1" = "auth" ]; then
  printf '%s ' "$@" > "$dir/auth_args.txt"
  if [ -f "$dir/auth.sh" ]; then . "$dir/auth.sh"; fi
  exit 1
fi
: > "$dir/args.bin"
prev=""
for a in "$@"; do
  printf '%s\0' "$a" >> "$dir/args.bin"
  if [ "$prev" = "--mcp-config" ]; then cp "$a" "$dir/mcp.json"; printf '%s' "$a" > "$dir/mcp_path.txt"; fi
  prev="$a"
done
pwd -P > "$dir/cwd.txt"
{body}
"#
        );
        let project_path = project.path().to_string_lossy().into_owned();
        std::fs::write(
            dir.path().join("stream.jsonl"),
            stream.replace("<PROJECT>", &project_path),
        )
        .unwrap();
        let path = dir.path().join("claude");
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self { dir, project }
    }

    fn runtime(&self) -> ClaudeCodeRuntime {
        ClaudeCodeRuntime::with_program(self.dir.path().join("claude").to_string_lossy())
    }

    fn read(&self, name: &str) -> String {
        std::fs::read_to_string(self.dir.path().join(name)).unwrap()
    }
}

#[cfg(unix)]
#[test]
fn detect_runs_the_version_and_auth_status_commands() {
    let fake = FakeClaude::new("exit 0", "");
    // No answer from `auth status`: can't tell.
    let status = fake.runtime().detect();
    assert_eq!(
        status,
        RuntimeStatus {
            name: "claude".into(),
            installed: true,
            version: Some("2.1.283".into()),
            logged_in: None,
        }
    );
    assert_eq!(fake.read("auth_args.txt"), "auth status --json ");

    let auth = |script: &str| {
        std::fs::write(fake.dir.path().join("auth.sh"), script).unwrap();
        fake.runtime().detect().logged_in
    };
    assert_eq!(
        auth(
            "printf '{\\n  \"loggedIn\": true,\\n  \"authMethod\": \"oauth_token\"\\n}\\n'; exit 0"
        ),
        Some(true)
    );
    // The field counts, whatever the exit code.
    assert_eq!(auth(r#"echo '{"loggedIn": false}'; exit 1"#), Some(false));
    assert_eq!(auth("echo 'Not logged in'; exit 1"), None);
}

#[cfg(unix)]
#[test]
fn a_command_that_hangs_is_given_up_on() {
    let started = Instant::now();
    assert_eq!(
        capture(
            Path::new("/bin/sh"),
            &["-c", "exec sleep 30"],
            Duration::from_millis(300)
        ),
        None
    );
    assert!(started.elapsed() < Duration::from_secs(10));
    let (status, out) = capture(
        Path::new("/bin/sh"),
        &["-c", "echo hi; exit 2"],
        Duration::from_secs(10),
    )
    .unwrap();
    assert_eq!((status.code(), out.as_str()), (Some(2), "hi\n"));
}

#[test]
fn a_missing_cli_is_not_logged_in_as_far_as_anyone_can_tell() {
    let runtime = ClaudeCodeRuntime::with_program("/nonexistent/dir/claude");
    assert_eq!(runtime.detect().logged_in, None);
}

#[cfg(unix)]
#[test]
fn runs_the_cli_and_streams_its_events() {
    let fake = FakeClaude::new(r#"cat "$dir/stream.jsonl""#, &fixture("b_edit_file.jsonl"));
    let message = r#"-Add "world" to notes.txt; echo $HOME `id`"#;
    let events = run(&fake.runtime(), request(fake.project.path(), message));

    // The same events the parser gives for the recording, in order.
    assert!(
        matches!(&events[0], AgentEvent::SessionStarted { provider_session_id, .. }
        if provider_session_id == "74eec361-ca75-4d9f-82ed-7b0b6aa6a735")
    );
    assert_eq!(
        &events[events.len() - 2..],
        &[
            AgentEvent::FilesChanged {
                paths: vec!["notes.txt".into()]
            },
            AgentEvent::TurnCompleted {
                is_error: false,
                duration_ms: Some(6986),
                usage: Some(crate::agent::Usage {
                    input_tokens: Some(10),
                    output_tokens: Some(524)
                }),
            },
        ]
    );
    assert_eq!(events.len(), 12);

    // Arguments arrive exactly as built, the message verbatim (no shell).
    let args: Vec<String> = fake
        .read("args.bin")
        .split('\0')
        .filter(|a| !a.is_empty())
        .map(String::from)
        .collect();
    let mcp_path = fake.read("mcp_path.txt");
    let expected: Vec<String> = build_args(
        &request(fake.project.path(), message),
        Path::new(&mcp_path),
        &turn_system_prompt(&request(fake.project.path(), message)),
    )
    .into_iter()
    .map(|a| a.into_string().unwrap())
    .collect();
    assert_eq!(args, expected);
    assert_eq!(args.last().unwrap(), message);

    // Run in the project, with the MCP config it was given, which is
    // deleted once the turn is over.
    assert_eq!(
        Path::new(fake.read("cwd.txt").trim()),
        fake.project.path().canonicalize().unwrap()
    );
    let config: serde_json::Value = serde_json::from_str(&fake.read("mcp.json")).unwrap();
    assert_eq!(
        config["mcpServers"]["infinabox"]["args"],
        serde_json::json!(["--mcp-server"])
    );
    assert!(
        !Path::new(&mcp_path).exists(),
        "temp MCP config should be deleted"
    );
}

#[cfg(unix)]
#[test]
fn a_failing_cli_ends_the_turn_with_its_stderr() {
    let fake = FakeClaude::new("echo 'boom: the CLI fell over' >&2\nexit 3", "");
    let events = run(&fake.runtime(), request(fake.project.path(), "hi"));
    assert_eq!(
        events,
        vec![
            AgentEvent::Error {
                kind: AgentErrorKind::ProcessFailed,
                message: "boom: the CLI fell over".into(),
            },
            AgentEvent::TurnCompleted {
                is_error: true,
                duration_ms: None,
                usage: None
            },
        ]
    );
}

#[cfg(unix)]
#[test]
fn the_real_bad_resume_output_ends_as_other() {
    let fake = FakeClaude::new(
        r#"cat "$dir/stream.jsonl"; echo 'No conversation found with session ID: 00000000-0000-0000-0000-000000000000' >&2; exit 1"#,
        &fixture("e_bad_resume.jsonl"),
    );
    let events = run(&fake.runtime(), request(fake.project.path(), "hi"));
    assert!(
        matches!(&events[0], AgentEvent::Error { kind: AgentErrorKind::Other, message }
        if message.starts_with("No conversation found"))
    );
    assert!(matches!(
        events[1],
        AgentEvent::TurnCompleted { is_error: true, .. }
    ));
    assert_eq!(events.len(), 2);
}

#[cfg(unix)]
#[test]
fn cancel_stops_the_cli_and_ends_the_turn() {
    // Prints the init line, then hangs (in a child process, to check the
    // whole process group is stopped).
    let fake = FakeClaude::new(
        r#"head -n 1 "$dir/stream.jsonl"; sleep 60"#,
        &fixture("a_plain_text.jsonl"),
    );
    let runtime = Arc::new(fake.runtime());
    let started = Instant::now();
    let mut events = Vec::new();
    let canceller = runtime.clone();
    runtime
        .run_turn(request(fake.project.path(), "hi"), &mut |e| {
            if matches!(e, AgentEvent::SessionStarted { .. }) {
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
        &events[1..],
        &[
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
    let fake = FakeClaude::new(
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
}

/// Several `run_turn`s on one thread started at the same moment: exactly
/// one runs, the rest are refused, and the map ends up empty.
#[cfg(unix)]
#[test]
fn concurrent_turns_on_one_thread_only_one_runs() {
    const N: usize = 4;
    let fake = FakeClaude::new(
        r#"head -n 1 "$dir/stream.jsonl"; sleep 60"#,
        &fixture("a_plain_text.jsonl"),
    );
    let runtime = Arc::new(fake.runtime());
    let barrier = Arc::new(std::sync::Barrier::new(N));
    let (tx, rx) = std::sync::mpsc::channel::<bool>();
    let handles: Vec<_> = (0..N)
        .map(|_| {
            let (runtime, barrier, tx) = (runtime.clone(), barrier.clone(), tx.clone());
            let project = fake.project.path().to_path_buf();
            std::thread::spawn(move || {
                barrier.wait();
                let mut events = Vec::new();
                let result = runtime.run_turn(request(&project, "hi"), &mut |e| events.push(e));
                let _ = tx.send(result.is_ok());
                (result.is_ok(), events)
            })
        })
        .collect();
    // The N-1 refusals come back straight away; then stop the one that runs.
    for _ in 0..N - 1 {
        assert!(!rx.recv_timeout(Duration::from_secs(10)).unwrap());
    }
    // Its slot is reserved before the spawn, so the cancel always lands.
    runtime.cancel("thread-1");
    let results: Vec<(bool, Vec<AgentEvent>)> =
        handles.into_iter().map(|h| h.join().unwrap()).collect();
    let ran: Vec<_> = results.iter().filter(|(ok, _)| *ok).collect();
    assert_eq!(ran.len(), 1);
    assert!(matches!(
        ran[0].1.last(),
        Some(AgentEvent::TurnCompleted { is_error: true, .. })
    ));
    assert!(runtime.running.lock().unwrap().is_empty());
}

/// A cancel that arrives after the slot is reserved but before the process
/// is stored is applied as soon as it is.
#[cfg(unix)]
#[test]
fn cancel_before_the_child_is_registered_still_stops_it() {
    use std::os::unix::process::CommandExt;
    let runtime = ClaudeCodeRuntime::new();
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

/// A guard only removes its own entry, never a later turn's.
#[test]
fn a_guard_only_removes_its_own_entry() {
    let runtime = ClaudeCodeRuntime::new();
    let first = RunningGuard::reserve(&runtime.running, "t").unwrap();
    // Simulate the slot having been replaced by another turn's.
    let other = Arc::new(TurnSlot::new());
    runtime
        .running
        .lock()
        .unwrap()
        .insert("t".into(), other.clone());
    drop(first);
    let map = runtime.running.lock().unwrap();
    assert!(Arc::ptr_eq(map.get("t").unwrap(), &other));
}

/// Runs one real turn, then resumes it. Needs a signed-in `claude`.
#[test]
#[ignore = "needs a logged-in claude CLI; run with --ignored"]
fn real_turn_edits_a_file_and_resumes() {
    let project = tempfile::tempdir().unwrap();
    std::fs::write(project.path().join("notes.txt"), "hello\n").unwrap();
    // The project's AGENTS.md reaches the agent (via the system prompt)...
    std::fs::write(
        project.path().join("AGENTS.md"),
        "The project codeword is PELICAN-42.\n",
    )
    .unwrap();
    // ...but its own settings (here a Stop hook) never run.
    let hook_marker = project.path().join("hook-ran");
    std::fs::create_dir(project.path().join(".claude")).unwrap();
    std::fs::write(
        project.path().join(".claude/settings.json"),
        serde_json::json!({"hooks": {"Stop": [{"hooks": [{"type": "command",
            "command": format!("touch '{}'", hook_marker.display())}]}]}})
        .to_string(),
    )
    .unwrap();
    let runtime = ClaudeCodeRuntime::new();
    let status = runtime.detect();
    assert!(status.installed, "claude not found");
    // The real `claude auth status --json` of a signed-in CLI.
    assert_eq!(status.logged_in, Some(true), "{status:?}");
    eprintln!("detected: {status:?}");

    let mut req = request(
        project.path(),
        "Append the line 'world' to notes.txt. This is a test \
project: the InfinaBox tools aren't available, so don't try to run the game.",
    );
    // No MCP server in this test; the CLI reports it as failed and goes on.
    req.mcp.command = PathBuf::from("infinabox-test-no-such-mcp-server");
    let events = run(&runtime, req.clone());
    eprintln!("{events:#?}");
    let Some(AgentEvent::SessionStarted {
        provider_session_id,
        ..
    }) = events.first()
    else {
        panic!("first event should be SessionStarted: {events:?}");
    };
    assert!(matches!(
        events.last(),
        Some(AgentEvent::TurnCompleted {
            is_error: false,
            ..
        })
    ));
    assert!(events.iter().any(|e| matches!(e, AgentEvent::FilesChanged { paths } if paths == &vec!["notes.txt".to_string()])));
    assert!(
        std::fs::read_to_string(project.path().join("notes.txt"))
            .unwrap()
            .contains("world")
    );

    assert!(!hook_marker.exists(), "the project's own hook ran");

    req.message = "Which word did you just add, and what is the project codeword? Reply with \
only those two, separated by a space."
        .into();
    req.resume_provider_session_id = Some(provider_session_id.clone());
    let resumed = run(&runtime, req);
    eprintln!("{resumed:#?}");
    assert!(
        matches!(resumed.first(), Some(AgentEvent::SessionStarted { provider_session_id: id, .. }) if id == provider_session_id)
    );
    assert!(matches!(
        resumed.last(),
        Some(AgentEvent::TurnCompleted {
            is_error: false,
            ..
        })
    ));
    let answer: String = resumed
        .iter()
        .filter_map(|e| match e {
            AgentEvent::AssistantText { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        answer.contains("world") && answer.contains("PELICAN-42"),
        "{answer}"
    );
    assert!(!hook_marker.exists(), "the project's own hook ran");
}

/// Cancels a real turn as soon as it starts. Needs a signed-in `claude`.
#[test]
#[ignore = "needs a logged-in claude CLI; run with --ignored"]
fn real_turn_can_be_cancelled() {
    let project = tempfile::tempdir().unwrap();
    let runtime = Arc::new(ClaudeCodeRuntime::new());
    let mut req = request(
        project.path(),
        "Write a 500-word story about a lighthouse into story.txt.",
    );
    req.mcp.command = PathBuf::from("infinabox-test-no-such-mcp-server");
    let canceller = runtime.clone();
    let started = Instant::now();
    let mut events = Vec::new();
    runtime
        .run_turn(req, &mut |e| {
            if matches!(e, AgentEvent::SessionStarted { .. }) {
                let canceller = canceller.clone();
                std::thread::spawn(move || canceller.cancel("thread-1"));
            }
            events.push(e);
        })
        .unwrap();
    eprintln!("{events:#?} after {:?}", started.elapsed());
    assert!(matches!(
        events.first(),
        Some(AgentEvent::SessionStarted { .. })
    ));
    assert_eq!(
        &events[events.len() - 2..],
        &[
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
}

#[test]
fn a_chosen_model_and_effort_reach_the_cli_and_a_bad_model_name_does_not() {
    use crate::agent::Effort;
    let dir = tempfile::tempdir().unwrap();
    let mut req = request(dir.path(), "hi");
    let plain = build_args(&req, Path::new("/tmp/mcp.json"), "P");
    assert!(!plain.iter().any(|a| a == "--model" || a == "--effort"));

    req.options.model = Some("sonnet".into());
    req.options.effort = Some(Effort::High);
    let args = build_args(&req, Path::new("/tmp/mcp.json"), "P");
    let at = |flag: &str| args.iter().position(|a| a == flag).unwrap();
    assert_eq!(args[at("--model") + 1], "sonnet");
    assert_eq!(args[at("--effort") + 1], "high");

    req.options.model = Some("--dangerous x".into());
    let args = build_args(&req, Path::new("/tmp/mcp.json"), "P");
    assert!(!args.iter().any(|a| a == "--model"));
}
