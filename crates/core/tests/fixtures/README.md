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

Things the parser must handle, all visible in these files:

- Unknown message types and `system` subtypes (`rate_limit_event`,
  `permission_denied`, `thinking_tokens`, ...) must be ignored, not errors.
- An `assistant` message can contain `thinking` blocks, which are not user
  text.
- Tool results arrive as `user` messages whose content holds `tool_result`
  blocks (with `tool_use_id` and optional `is_error`).
- `--allowedTools` does **not** remove other tools (the model tried `Bash`
  first in `b_edit_file`); limit built-in tools with `--tools`.
- MCP tools may be deferred behind a `ToolSearch` call first.
- The `result` message carries `usage` (`input_tokens`, `output_tokens`,
  cache fields), `duration_ms`, `is_error`, `subtype`, and `result` text.
