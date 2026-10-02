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

## `claude/` — recorded

- **Version:** Claude Code `2.1.283` (see `version.txt`).
- **Recorded:** 2026-09-26, with `node scripts/record-claude-fixtures.mjs`.
- **How:** in the cloud container that built Phase A, but run as close to a
  normal local install as possible: `env -i` with only `PATH`, a fresh empty
  `HOME` (so no user config, plugins or settings), and the proxy/API variables
  the container needs to reach the API. None of the session-specific variables
  that the earlier dry run showed changing the output were set, and the
  recordings contain none of those extra event types.
- **Scrubbing (done by the script):** project path, home directory, username
  and git email replaced with placeholders; the `init` message's machine
  lists (`tools`, `slash_commands`, `skills`, `plugins`, `agents`,
  `capabilities`, ...) emptied and local socket/memory paths removed;
  `rate_limit_event` reduced to `{status}`. Everything else is byte-for-byte.
- **Re-record on a real local install** (`node scripts/record-claude-fixtures.mjs`
  on a Mac with `claude` logged in) whenever the CLI version changes or to
  confirm these. If the output differs, the new recording wins; fix the
  parser, not the fixture.

| Scenario | Command (all with `--output-format stream-json --verbose`) | What it shows |
|---|---|---|
| `a_plain_text` | `-p "Reply with exactly: hello from the fixture"` | `system/init` → `assistant` (text) → `rate_limit_event` → `result` (success) |
| `b_edit_file` | `-p "Append the line 'world' to notes.txt..." --allowedTools Read,Edit,Write` | Thinking blocks, a `Bash` attempt that is **denied** (`system/permission_denied`), then `Read` and `Edit` tool uses with `user` tool results, then text |
| `c_resumed_turn` | `-p "..." --resume <a's session id>` | A resumed session |
| `d_mcp_tool` | `-p "Call the echo tool..." --mcp-config mcp.json --allowedTools mcp__fixture__echo` | MCP tool named `mcp__fixture__echo`, preceded by a `ToolSearch` tool use |
| `e_bad_resume` | `-p hi --resume 00000000-...` | Only a `result` with `is_error: true`, subtype `error_during_execution`; exit code 1; stderr `No conversation found with session ID: ...` |
| `f_propose_plan` | the runtime's own flags (`--setting-sources user --mcp-config <cfg> --strict-mcp-config --tools Read,Edit,Write,Glob,Grep --allowedTools ...,mcp__infinabox__* --permission-mode acceptEdits --append-system-prompt <Director + plan-always + explain + the project's AGENTS.md>`) and `"Add a double jump to my game..."`, in a copy of `templates/blank-2d`, with `scripts/fixtures/echo-mcp-server.mjs --propose-plan` registered as `infinabox` | A plan turn: `Read`/`Glob` of the game and its Context card, then `mcp__infinabox__propose_plan` (title + 5 steps) whose result is the real server's "plan is now shown" text, then one closing sentence; no file changes |

`f_propose_plan` was recorded 2026-09-28 (same CLI version) by the script
itself running the CLI in a clean environment: only `PATH`, a fresh empty
`HOME`, and the proxy/certificate variables. The CLI found its own sign-in;
nothing was read or passed for it. The script also scrubs the repo path,
the temp MCP config folder (`<CONFIG>`) and the model identifier
(`<MODEL>`, from the `init` message) — the Phase A recordings above predate
the model scrub. Record only some scenarios with
`node scripts/record-claude-fixtures.mjs f_propose_plan`.

Things the parser must handle, all visible in these files:

- Unknown message types and `system` subtypes (`rate_limit_event`,
  `permission_denied`, `thinking_tokens`, ...) must be ignored, not errors.
- An `assistant` message can contain `thinking` blocks, which are not user
  text.
- Tool results arrive as `user` messages whose content holds `tool_result`
  blocks (with `tool_use_id` and optional `is_error`).
- `--allowedTools` does **not** remove other tools (the model tried `Bash`
  first in `b_edit_file`); limit built-in tools with `--tools`.
- MCP tools may be deferred behind a `ToolSearch` call first (with
  `--tools` set, as in `f_propose_plan`, they are listed directly).
- A plan is an ordinary MCP `tool_use` named `mcp__infinabox__propose_plan`
  with `input: {title, steps}`; the parser turns a valid one into
  `PlanProposed` and drops its `tool_result`.
- The `result` message carries `usage` (`input_tokens`, `output_tokens`,
  cache fields), `duration_ms`, `is_error`, `subtype`, and `result` text.

## `codex/` — recorded

- **Version:** Codex CLI `0.157.1` (see `version.txt`; `help.txt` is
  `codex exec --help`, `resume-help.txt` is `codex exec resume --help`).
- **Recorded:** 2026-09-28, with
  `CODEX_BIN=<path to codex> node scripts/record-codex-fixtures.mjs`.
- **How:** the real `codex exec --json` CLI, with the same restriction flags
  the runtime passes (`build_args` in `crates/core/src/agent/codex.rs`;
  the script's `RESTRICTIONS` mirrors it), in a real git project with an
  `AGENTS.md`, and a throwaway `CODEX_HOME`. For every scenario but the
  last two, Codex is pointed at `scripts/fixtures/mock-responses-server.mjs`
  — a scripted local OpenAI-compatible Responses endpoint, configured with
  `-c model_provider=mock -c model_providers.mock={..., wire_api="responses",
  env_key=<fake key var>}` plus a `model_catalog_json` entry for the scripted
  model (so Codex offers its own freeform `apply_patch` tool, as it does for
  its real models). Only the model's side of the conversation is scripted
  (picked by a `SCENARIO=<name>` word in the message); every line here is
  the CLI's own output, and file edits, MCP calls and shell commands really
  happen. The MCP server is the same script run with `--mcp` (an `echo`
  tool that also reports whether the bridge token reached it, and
  `propose_plan`), registered as `infinabox`. The error scenarios use the
  real default provider with a fresh, signed-out `CODEX_HOME`.
- **Scrubbing (done by the script):** project path, `CODEX_HOME`, temp dir,
  repo path, home directory, username, the mock's port and the help text's example model name replaced with
  placeholders. Everything else is byte-for-byte. The script fails if the
  bridge token appears in the output or the arguments.
- **Re-record** whenever the CLI version changes. If the output differs, the
  new recording wins; fix the parser, not the fixture.

| Scenario | Message / how | What it shows |
|---|---|---|
| `a_plain_text` | `SCENARIO=text ...` | `thread.started` → `turn.started` → `item.completed` (`agent_message`) → `turn.completed` (`usage`, no duration) |
| `b_edit_file` | `SCENARIO=edit ...` | An `apply_patch` edit reported as a `file_change` item (`item.started` then `item.completed`, `changes: [{path: <absolute>, kind: "update"}]`); `notes.txt` really changed |
| `c_resumed_turn` | `resume <a's thread id> -- SCENARIO=resume ...` | The resumed turn reports the **same** `thread_id`; the mock's reply quotes (a)'s answer, so the history really came along |
| `d_mcp_tool` | `SCENARIO=mcp ...` | `mcp_tool_call` item (`server: "infinabox"`, `tool`, `arguments`, `result.content`, `error`); the result says the bridge token arrived through `env_vars` |
| `e_propose_plan` | `SCENARIO=plan ...` | `propose_plan` as an `mcp_tool_call` with `{title, steps: [..]}` |
| `f_bad_plan` | `SCENARIO=bad_plan ...` | `propose_plan` with `steps` as a string (Codex doesn't validate MCP input against the schema) |
| `g_shell_command` | `SCENARIO=shell ...` | `command_execution` item (`command`, `aggregated_output`, `exit_code`); `printenv` shows the bridge token is **not** visible to shell commands (`shell_environment_policy.exclude`) |
| `h_cancelled` | `SCENARIO=hang ...`, SIGTERM to the process group after the edit | An edit, then the stream just stops: no `turn.completed`/`turn.failed`, **exit code 0** |
| `i_unauthorized` | `SCENARIO=unauthorized` (the mock answers HTTP 401) | Top-level `error` lines `Reconnecting... n/5 (unexpected status 401 Unauthorized: ...)`, then `turn.failed` with `error.message`; exit 1 |
| `j_bad_resume` | `resume 00000000-... -- hi` | No JSON at all; stderr `Error: thread/resume: thread/resume failed: no rollout found for thread id 00000000-... (code -32600)` plus a backtrace; exit 1 |
| `k_not_logged_in` | `-- Reply with exactly: hello`, signed out, real provider | Codex doesn't check sign-in before sending. Here (no route to the API) it retried (`Reconnecting... n/5`, a non-fatal `error` *item* "Falling back from WebSockets to HTTPS transport", then `Reconnecting... waiting for network`) until the script stopped it (SIGTERM, exit 0). On a machine with network the real answer is an HTTP 401, as in `i_unauthorized` |
| `login_status_signed_out.txt` / `login_status_signed_in.txt` | `codex login status` (signed in with a fake API key; the command only reads stored credentials) | Exit 1 + `Not logged in` on stderr / exit 0 + `Logged in using an API key - sk-mock-***-0000` |

Things the parser must handle, all visible in these files:

- Every line is `{"type": ...}`: `thread.started`, `turn.started`,
  `item.started`/`item.completed` (each with an `item` whose `type` is
  `agent_message`, `file_change`, `mcp_tool_call`, `command_execution`, ...),
  top-level `error` (retry notices, not the end), `turn.completed`,
  `turn.failed`.
- `item` of type `error` is a non-fatal warning (e.g. unknown model
  metadata, the WebSockets fallback), not the turn's error.
- Paths in `file_change` are absolute; edits made through the shell tool
  are *not* reported as `file_change`s (only the command is).
- A cancelled turn and some failures end without a turn event, and a
  SIGTERM'd Codex exits 0 — the runtime can't rely on the exit code.
- Errors before a thread exists are only on stderr, as `Error: <message>`.
- `stderr` always starts with a `WARNING: proceeding, even though we could
  not create PATH aliases` line here (because `CODEX_HOME` is under `/tmp`),
  and `codex exec` (not `resume`) prints `Reading additional input from stdin...` even when stdin is `/dev/null`.

## `import/` — made with openpyxl

`balance.xlsx` is a small workbook (two sheets, a whole number, a decimal, a
cell with a `|` in it, and a formula that was never calculated) saved by the
real `openpyxl` library, for `doc_import`'s spreadsheet test.
