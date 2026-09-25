# Real tool output fixtures

Recorded from the real tools, never hand-written. The Phase A parsers (agent
runtime, Godot errors) are tested against these files; if a parser and a
fixture disagree, fix the parser. See
`docs/superpowers/plans/2026-09-25-phase-a-foundations.md`, Task 0.2.

## `godot/` — recorded

- **Version:** `4.7.2.stable.official.ed1daf0bf` (Linux x86_64 official build,
  SHA-512 verified against the release's `SHA512-SUMS.txt`). This is
  `infinabox_core::godot::PINNED_VERSION`.
- **Recorded:** 2026-09-25.
- **Command, per scenario:**
  `godot --headless --path projects/<scenario> --quit-after 30`
  with stdout and stderr piped separately (as the app will run it — piped
  output has no ANSI colour codes).
- **Files:** `<scenario>.stdout.txt`, `<scenario>.stderr.txt`, the exact
  project that produced them in `projects/<scenario>/`, plus `version.txt`
  and `help.txt` (colour codes and the binary's location stripped).

| Scenario | What the project does |
|---|---|
| `clean` | Boots and prints `[infinabox] ready 1` (the addon's ready line format) |
| `parse_error` | GDScript syntax error in `main.gd` |
| `runtime_error` | Calls a method on `null` in `_ready` |
| `missing_resource` | `load()`s a `res://` path that doesn't exist |

Things the parser must handle, all visible in these files:

- **Godot exits 0 even when scripts fail.** Exit code is not an error signal;
  stderr content is.
- Errors are **multi-line blocks** on stderr: a `SCRIPT ERROR: ...` or
  `ERROR: ...` line, then an indented `at: <function> (<location>)` line,
  sometimes followed by `GDScript backtrace (most recent call first):` and
  indented `[n] <function> (res://file.gd:line)` frames.
- For `SCRIPT ERROR`s the `at:` location is the user's `res://` file and line.
  For engine `ERROR`s (e.g. a missing resource) the `at:` location is engine
  C++ source (`core/io/resource_loader.cpp:325`); **the user's location is in
  the first backtrace frame** instead.
- A parse error produces two blocks: the `SCRIPT ERROR: Parse Error` (with
  the user's file and line) and a follow-up engine `ERROR: Failed to load
  script` about the same file. Both are real; the UI may choose to group them.
- Window flags for later tasks, from `help.txt`: `--position X,Y`,
  `--resolution WxH`, `--windowed`, and `--wid <window_id>` ("Request
  parented to window"), which is worth trying first when embedding the game
  window is researched.

## `claude/` — to be recorded on a real local install

Not recorded yet. These must come from a normal, locally installed and
logged-in Claude Code CLI (the one InfinaBox users will have). On a machine
with that, from the repo root:

```
node scripts/record-claude-fixtures.mjs
```

It records five scenarios (plain answer, file edit, resumed turn, MCP tool
call via `scripts/fixtures/echo-mcp-server.mjs`, bad `--resume` id) plus
`--version` and `--help`, scrubbing home path, username, git email, and the
temp project path. Review the files, then commit them.

**Why not recorded in the cloud session that wrote Wave 0:** the only
`claude` available there was a cloud-hosted build running under that
session's managed configuration. Its output included extra message types and
partial-message stream events that environment variables were turning on,
and it resumed the *host* session's id. That output doesn't represent what
users' installs print, so it must not become the parser's source of truth.

Observations from that dry run of the script (CLI 2.1.282), useful for Task
A but **to be confirmed against the real recording**:

- Headless streaming works as `claude -p "<prompt>" --output-format
  stream-json --verbose`. Each line is one JSON object with a `type`.
- A `system`/`init` message carries `session_id`. Assistant turns are
  `assistant` messages whose `message.content` is a list of blocks (`text`,
  `thinking`, `tool_use` with `name`/`input`). Tool results come back as
  `user` messages. The turn ends with a `result` message carrying
  `is_error` and `subtype` (`success`, `error_during_execution`).
- MCP tools are named `mcp__<server>__<tool>` (e.g. `mcp__fixture__echo`).
- A bad `--resume` id produces a single `result` with `is_error: true`,
  subtype `error_during_execution`, exit code 1, and stderr
  `No conversation found with session ID: <id>`.
- **`--allowedTools` does not remove other tools**: in the file-edit
  scenario the model tried `Bash` first and got a `permission_denied`. To
  actually limit the built-in tool set, use `--tools "Read,Edit,Write,Glob,Grep"`.
  `--strict-mcp-config` ignores the user's other MCP servers.
  `--session-id <uuid>` lets InfinaBox choose the session id up front.
  `--include-partial-messages` is opt-in; without it there should be no
  `stream_event` lines.
- Unknown message types appeared (`rate_limit_event`, `system` subtypes
  such as `status`, `task_summary`, `post_turn_summary`); the parser must
  ignore types it doesn't know.
