# InfinaBox

A Tauri v2 + React 19 + TypeScript desktop app for indie game developers. This file orients a fresh session fast — grounded in the current code, not the project's history of plans.

## What this is

InfinaBox is becoming an **AI game studio for people with little or no game-dev knowledge** — see `docs/superpowers/specs/2026-09-25-ai-game-studio-product-spec.md` (the product direction) and `docs/superpowers/plans/2026-09-25-phase-a-foundations.md` (Phase A, the current build). The user's **own** AI (their installed, logged-in Claude Code CLI; later Codex, API keys, local models) does the work; InfinaBox never sells or proxies AI access and stores no AI credentials.

Phase A delivers the core loop in a new **Studio** section: a **chat panel** (this is now the primary interface — the older "no chat panel" stance and the `agent_auth_model` memory are superseded by the spec), a **Play** panel running the user's Godot game, and a **History** panel of snapshots with undo. The pre-Phase-A workspace (Build's terminal + code editor, and the docs-style discipline sections) still exists and will move into an "Advanced" area in Phase B.

The stated product discipline, from `docs/superpowers/specs/2026-09-12-workspace-redesign-design.md`:

> Every number or status shown is real. No fabricated activity, no invented stats.

This is not just doc language — it's load-bearing throughout the real implementation: CLI tool detection is a real `PATH` lookup (`src-tauri/src/commands/environment.rs`), git stats come from real `git2` calls, and sections with no real backing data render an honest `NotBuiltYetSection` rather than placeholder numbers (see "What's real vs. placeholder" below).

## Architecture

Cargo workspace (`Cargo.toml` at repo root, which also holds the release profile — member crates can't set profiles) with four members:

- **`crates/core`** (`infinabox-core`) — the real logic, unit-tested without Tauri:
  - `agent/` — the provider-independent `AgentRuntime` (`types.rs`, a frozen contract) and `ClaudeCodeRuntime` (`claude.rs`), which drives the user's `claude` CLI headless (`-p --output-format stream-json`) with a restricted tool set, `--setting-sources user` (a project's own `.claude/settings.json` never runs), the InfinaBox MCP server, and the Director prompt (`prompts/director.md`) plus the project's `AGENTS.md`. `claude_stream.rs` turns the CLI's JSON lines into `AgentEvent`s; `path.rs` resolves the login-shell PATH (GUI apps on macOS don't inherit it).
  - `godot/` — managed Godot install (pinned `4.7.2-stable`, SHA-512 verified, into the app data dir), `locate` (managed install, else `INFINABOX_GODOT`), `run` (game process in its own process group, output capture), `errors` (parser for real Godot error blocks), `validate` (headless import/boot).
  - `snapshot.rs` — snapshots are git commits with `InfinaBox-*` trailers; undo/go back are always new commits; restores never rewind `.ibproject/chat/` or remove `.ibproject/.ibx`; a process-wide lock serializes all history operations.
  - `chat_store.rs` — chat threads as `.ibproject/chat/<id>.jsonl` (committed with the project), every string passed through `redact.rs` first.
  - `scaffold.rs` — new projects from `templates/blank-2d/` plus the auto-installed runtime addon from `godot-addon/infinabox/`.
  - Older pieces: filesystem watcher, git indexer, SQLite project graph + deterministic `ask.rs` Q&A, and an `mcp_client.rs` spike.
- **`crates/mcp-server`** (`infinabox-mcp-server`) — the MCP server the user's agent gets (10 tools: Context cards, run/stop the game, game status/errors/output, snapshot list). It is **not a separate binary**: the app executable runs it when launched as `<app exe> --mcp-server` (checked in `src-tauri/src/main.rs` before Tauri starts). Game tools reach the running app over a loopback **bridge** (`bridge_protocol.rs`: token-authenticated, newline-delimited JSON).
- **`crates/cli`** — dev binary: spike subcommands plus `infinabox-cli mcp-server` (same server, for testing); `mock_godot_mcp` is an old spike.
- **`src-tauri`** — the Tauri v2 app. Commands live under `src-tauri/src/commands/`: `agent.rs` (chat turns: runs the agent on a background thread, persists every event, snapshots after file-changing turns, always emits `agent-turn-finished`), `godot.rs` (install/status/run/stop, events), `bridge.rs` (the MCP bridge server), `snapshot.rs`, `scaffold.rs`, plus the older `fs.rs`, `project.rs`, `terminal.rs`, `overview.rs`, `watcher.rs`, `environment.rs`. Each is re-exported through `commands/mod.rs` and registered in `src-tauri/src/lib.rs`'s `tauri::generate_handler![...]` list. Adding a new command means: write the `#[tauri::command]` fn, export it from `mod.rs`, add it to the `invoke_handler!` list — all three steps, or the frontend's `invoke()` call fails silently at runtime with no type error. **A command doing real work (git, files, child processes) must be `#[tauri::command(async)]`**: in Tauri v2 a plain sync command runs on the main thread and freezes the window.

Frontend: `src/App.tsx` is the app shell; `src/components/studio/` holds Studio (`chat/`, `play/`, `history/`, `StudioSection.tsx`); `src/components/cockpit/` holds the older panels/sections; `src/lib/` holds shared utilities. **Studio calls the backend only through `src/lib/studio-api.ts`** (typed wrappers for every Phase A command and event) with types in `src/lib/studio-types.ts`, which mirror the Rust serde types — change both sides together.

Tauri events used by Studio: `agent-event`, `agent-turn-finished`, `godot-install-progress`, `game-state`, `game-output`, `game-error`, `snapshots-changed` (see `studio-api.ts`).

### `src/App.tsx`'s persistent-tab crossfade

Read this file in full before touching navigation — it has a genuinely unusual mechanism. Four sections (**Studio**, **Build**, **Design/Documents**, **Graphs**) must never unmount once opened: Studio holds live chat turns and game state, Build owns the live terminal session (respawning it would kill whatever the user is doing in their shell), and Design/Graphs can each hold unsaved drafts. Opening a project from Home lands on Studio. Every other section (`art`, `audio`, `uiux`, `qa`, `release`, `business`, `marketing`, `community`, `liveops`) mounts only while selected and has nothing worth preserving.

Because `AnimatePresence` (Motion's crossfade helper) only animates real mount/unmount, and these four panels never unmount, they can't use it directly. Instead all four sit **absolutely stacked in one slot, permanently mounted**, with only `opacity`/`pointer-events` toggling per the active `section`. Each inactive one also carries the HTML `inert` attribute — not just `pointer-events: none` — which additionally pulls it out of the tab order and accessibility tree, specifically because a prior bug in this codebase's history came from a descendant re-adding its own `pointer-events-auto` inside a "hidden" panel.

Everything else (Home, and the 9 "simple" sections) goes through ordinary `AnimatePresence mode="wait"` mount/unmount, driven by a data-driven `simpleSections: Record<SimpleSection, () => ReactNode>` render map rather than a growing `{section === "x" && ...}` if-chain.

## Key established patterns

Each of these was verified against the current source — cite the same file if you need to check for drift.

- **`data-tauri-drag-region` must go on the exact draggable element, not an ancestor.** It's purely a data attribute Tauri's JS layer reads on `pointerdown`; it never touches CSS `pointer-events`, and nothing "inherits" it. See `src/App.tsx`'s top comment (explicitly calls out "despite an earlier, disproven theory in this codebase's history") and `Sidebar.tsx`'s own comment on the same point. Every block's empty header strip (the "Files"/"Agent" label rows) carries this attribute directly so it's draggable while the buttons inside stay clickable for free.

- **React 18/19 StrictMode's double-invoke-on-mount gotcha → compare against a content-based baseline, not an invocation-count flag.** `GraphEditor.tsx`'s `baselineRef` holds a `JSON.stringify` snapshot of the just-parsed initial doc, and the dirty-check effect only fires `onChange` when the current serialization differs from that baseline — not from a "first render" ref, which StrictMode's mount→cleanup→mount cycle would flip after the first of two invocations and then miss on the second. `MarkdownEditor.tsx` gets the same effect a different way: MDXEditor itself reports an `initialMarkdownNormalize` flag on its first `onChange`, which is ignored rather than tracked with a ref.

- **`ResizablePanelGroup` (`src/components/cockpit/ResizablePanelGroup.tsx`)** is the one system every multi-block row uses for resize + drag-to-reorder, persisted to `localStorage` under `infinabox.layout.<storageKey>`. Used by Build's Agent/Code split (`storageKey="build"`) and every `FileBrowser` `variant="list"` instance's list-vs-inspector split (`storageKey="filebrowser.<label>"`). Percentage flex-basis is deliberately resolved to real pixel widths (tracked via a debounced `ResizeObserver`) instead of CSS percentage flex-basis, because this rendering engine adds `gap` on top of a 100%-summing row instead of subtracting it first — see the file's `GAP` comment.

- **Live filesystem watching**: `src-tauri/src/commands/watcher.rs` (a debounced `notify` watcher, `WatcherState` holding at most one active watch, replaced wholesale on every `watch_project_path` call) emits a payload-less `project-fs-changed` event, subscribed to via `src/lib/fs-watch.ts`'s `onProjectFilesChanged`. Every disk-reading panel (file trees, open file content) just re-fetches whatever it's currently showing — deliberately coarse-grained, no per-path diffing.

- **`.ibproject/.ibx`** (`src/lib/project-picker.ts`) is how InfinaBox tells a real project apart from an arbitrary folder — `isInfinaBoxProject` just tries to read that file and treats any failure as "no". `.ibproject/` itself is hidden from every general file browser by the same dotfile rule that hides `.git` (`src-tauri/src/commands/fs.rs`'s `list_directory`), while docs-style sections read from inside it directly by path (the dotfile filter only applies to entries *discovered* while listing, not paths given directly).

- **Motion (`motion/react`) conventions** (`src/lib/motion.ts`): `springTransition` is the app-wide default for anything that moves; `fadeTransition` for opacity-only appear/disappear; `widthTransition` (a plain tween, not a spring) is a deliberate, documented exception for layout-affecting properties (`width`/`height`) — a spring's physics ticks aren't frame-budgeted and can overshoot before settling, which is expensive when something downstream (the Sidebar's width animation, in particular) is also reacting via `ResizeObserver` on every tick. `src/lib/debounce.ts`'s leading+trailing debounce exists for exactly that downstream cost: `ResizablePanelGroup` and `TerminalPanel` both use it to collapse a burst of intermediate `ResizeObserver` firings (from an animating ancestor) into one immediate call plus one settled call, instead of a real IPC round-trip (`resize_terminal`) on every frame.

- **`FileBrowser` (`src/components/cockpit/FileBrowser.tsx`)** is the one generic engine behind Documents, Business, Marketing, Community, Release, Graphs, and Build's Code panel. Mode flags change behavior, not identity:
  - `forceMdExtension` — "New Document" with no extension typed defaults to `.md` (docs-style sections).
  - `graphExtension` — defaults to `.graph.json`, which always opens in `GraphEditor` regardless of which `FileBrowser` instance found it.
  - `codeEditor` — Build's "mini VS Code": every readable text file opens in the CodeMirror-backed `CodeEditor`, not the markdown/graph editors (though `.md` still always gets `MarkdownEditor` even here — that rich editor isn't replaced by the general-purpose one).
  - `variant="list"` vs `"grid"` — list (Documents/Business/Graphs/Build) puts the file list and inspector side by side via `ResizablePanelGroup`; grid (Files-style) stacks a Finder-style icon grid above the inspector in one card.

- **Editor contract**: `MarkdownEditor`, `GraphEditor`, and `CodeEditor` are all uncontrolled — `initialValue` is read once on mount. Callers must pass `key={path}` (or `${path}:${version}` when an external fs change should force a remount) to switch files correctly; see `FileInspector` inside `FileBrowser.tsx`.

- **Delete is the one irreversible FS action** — always confirmed via `DeleteEntryDialog.tsx` in `FileGrid`/`FileList` before calling `delete_path`, per `src-tauri/src/commands/fs.rs`'s own doc comment on that command.

- **Per-project graph DB storage**: `refresh_project_graph`/`ask_question` (`src-tauri/src/commands/project.rs`) resolve a SQLite DB path under the Tauri app data directory, hashed (FNV-1a) from the canonicalized project path — never inside the user's own git repo. This infrastructure is real and tested but currently has **zero frontend callers** (see "What's real vs. placeholder").

## Tech stack specifics

- **MDXEditor** (`@mdxeditor/editor`) — WYSIWYG markdown, real inline rendering (not a hide/reveal CodeMirror trick). Themed via `markdown-editor-dark.css`.
- **`@xyflow/react`** (ReactFlow) — backs `GraphEditor.tsx` for the Graphs discipline; custom node shapes (label/diamond/circle) with inline-editable labels, a custom labeled bezier edge type, and a sanitize step that strips React Flow's own runtime bookkeeping (`measured`, `selected`, `dragging`) before persisting to `.graph.json`.
- **`@uiw/react-codemirror`** — Build tab's general-purpose source editor (`CodeEditor.tsx`), language-detected by extension; GDScript (`.gd`) deliberately reuses the Python highlighter since its syntax is close enough to be genuinely useful.
- **`motion`** (not `framer-motion`) — see conventions above. `MotionConfig reducedMotion="user"` wraps the whole app in `App.tsx`, so every animation respects the OS reduce-motion setting for free.
- **shadcn/ui** — see `components.json`: style `radix-nova`, base color `neutral`, CSS variables on, no class prefix, icon library `lucide`.

## How to build / verify

From repo root unless noted:

```
npx tsc --noEmit                          # type-check the frontend
npm run build                             # tsc + vite build (currently clean; one chunk-size warning, ~2MB main bundle from MDXEditor+CodeMirror+ReactFlow+xterm all in one chunk — not yet code-split)
cd src-tauri && cargo check --no-default-features   # established Rust check command
cargo test --workspace --no-default-features        # from repo root
INFINABOX_GODOT=/path/to/godot cargo test --workspace --no-default-features -- --ignored   # tests needing a real Godot (and some a logged-in claude)
```

- **Parser tests run against real recorded tool output** in `crates/core/tests/fixtures/` (`godot/`, `claude/`; see its README for how each was recorded). If a parser and a fixture disagree, fix the parser. Re-record Claude fixtures with `node scripts/record-claude-fixtures.mjs` when the CLI changes.
- **End-to-end tests drive the real app** via `e2e/` (tauri-driver + WebKitWebDriver on Linux under Xvfb; see `e2e/README.md`): create a project, Play with real Godot, errors, undo/go back, managed Godot install, and optionally a real AI turn.

`cargo check` (default features, no flag) currently also passes clean locally — there's no CI config in this repo (no `.github/`) codifying why `--no-default-features` specifically is the convention, so treat it as the established local habit rather than a documented hard requirement.

**Known environment-dependent test failures**: several Rust tests (`src-tauri/src/commands/fs.rs`, `src-tauri/src/commands/project.rs`, `src-tauri/src/commands/overview.rs`) assert against a real fixture repo hardcoded at `/Users/raoulkaleba/Developer/hollow-meridian-test` (a genuine Godot-shaped git repo with a tracked file rename, used to prove the graph's rename-aware file identity tracking). Most of these pass on this machine because that fixture exists. One test currently fails here regardless — `commands::fs::tests::reads_a_real_gdd_doc_from_the_fixture` — because the fixture's `.ibproject/docs/gdd/sector-3-verticality.md` file isn't present on disk right now (the fixture repo exists but is missing that specific doc). This is a stale/incomplete local fixture, not a code regression — don't "fix" it by touching `fs.rs`. On any machine without that fixture (e.g. cloud sessions), all 7 fixture-dependent tests fail (`git_indexer` ×2, `fs` ×2, `overview` ×2, `project` ×1); that's expected.

## What's real vs. placeholder right now

From `src/App.tsx`'s `simpleSections` map — deliberately honest placeholders, not oversights:

| Section | State | Why |
|---|---|---|
| Studio | real (Phase A) | chat with the user's own Claude Code (streamed events, threads, stop), Play panel (managed Godot install, run/stop, output, parsed errors, "Ask AI to fix"), History panel (snapshots, undo, go back) |
| Build | real | terminal + git-backed project data + `FileBrowser` code editor |
| Documents (Design) | real | `FileBrowser` over `.ibproject/docs`, `.md`-only editing |
| Graphs | real | `FileBrowser` over `.ibproject/graphs`, ReactFlow canvas for `.graph.json` |
| Business / Marketing / Community / Release | real | `FileBrowser` clones over `.ibproject/{business,marketing,community,release}` — docs-only; Release has no real ship-state dashboard or tool detection yet |
| Art / UI-UX | placeholder | needs image preview in the file browser, not markdown standing in for it |
| Audio | placeholder | needs audio playback in the file browser |
| QA | placeholder | bug tracking + grounded commit lookups — waiting on the core agent loop (i.e. the graph/`ask_question` pair) to get a real frontend caller |
| Live Ops | placeholder | analytics/crash triage need a shipped game generating real data — inherently post-ship |

Home/Dashboard is real: recent-projects list (`localStorage`), real CLI PATH detection (`check_cli_tools`), real New/Open Project flows — New Project now scaffolds a real blank 2D Godot project (`project_create`) with the addon, `.ibx` v2 and a first snapshot. Its own "Preferences" sub-panel is a placeholder too.

## Orientation for anything not covered here

- For the product direction, read `docs/superpowers/specs/2026-09-25-ai-game-studio-product-spec.md`; for what Phase A built, the decisions made along the way and its status, read `docs/superpowers/plans/2026-09-25-phase-a-foundations.md` (its status notes are the most current record). `docs/superpowers/plans/2026-09-22-project-status-and-roadmap.md` describes the pre-Phase-A state.
- The `agent_auth_model` project memory's "no chat panel" rule is superseded; its credential rule (InfinaBox stores no AI credentials for CLI-based agents) still holds.
- `crates/core/src/ask.rs` is intentionally a rule-based, deterministic Q&A agent over the local graph — not an LLM call. Its own doc comment explains why (proving the grounding/retrieval loop matters more right now than model fluency) and how a real model call would slot in later.
