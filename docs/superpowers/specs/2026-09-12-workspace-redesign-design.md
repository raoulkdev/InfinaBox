# InfinaBox workspace redesign: Agent panel + Project window

**Status:** approved for spec, pending user review of this document
**Date:** 2026-09-12
**Supersedes:** the "cockpit" layout (file browser + Build/Design/QA/Business/Live Ops sections + agent chat + console)

## Context

Phase 1 built a fixed four/five-panel "cockpit": a permanent file browser, a
Build viewport, a "Project" dropdown switching in Design/QA/Business/Live Ops
overlays, an agent chat panel, and a console log dock. Two things changed
since:

1. The agent chat panel was replaced with a real embedded terminal (a Rust
   PTY via `portable-pty` + `xterm.js`) — the actual AI interaction is now
   the user's own Claude Code/Codex CLI, not a custom chat UI InfinaBox
   brokers. See `agent_auth_model` project memory.
2. The user pointed at two reference products (Ship Studio's Agent+Preview
   session layout, and a generic workspace app with per-project sessions)
   and said the product doesn't need "a bunch of agents with central
   memory" — the user's own coding agent already sees the whole project.
   InfinaBox's job is to be a game-dev-focused workspace around that agent,
   not a bespoke multi-agent orchestration system.

This spec redesigns the interface around that: a two-pane session (Agent +
Project window) replacing the fixed cockpit.

## Goals

- Every project session is one Agent panel (the real terminal, unchanged)
  paired with one Project window that gives both the human and the agent a
  shared, real, glanceable view of the project's state.
- The Project window covers what the old file browser + Design section +
  (the honest, mostly-empty) Build viewport used to, consolidated into one
  place with three tabs: **Overview**, **Changes**, **Files**.
- Reuse everything from Phase 0/1 that's still genuinely useful — the Git
  indexer, the CodeMirror markdown editor, the file-listing command — rather
  than rebuilding them. This redesign is a recomposition, not a rewrite.
- Every number or status shown is real. No fabricated activity, no invented
  stats — the same discipline the project has held since Phase 0.

## Non-goals (explicitly deferred, not part of this design)

- **A live, playable game preview.** Considered and rejected for this pass
  — the user clarified the Project window is an overview/changes/inspector
  surface, not a game viewport. "Open in Godot" remains the way to actually
  see the game; that handoff is unrelated to this redesign and still isn't
  built.
- **A richer, structured Agent panel** (shelling out to `claude --print
  --output-format=stream-json` and rendering it natively instead of a raw
  terminal). The plain terminal stays. Named as a future follow-up.
- **Multiple sessions per project** (a session list/history, switching
  between past sessions). This design assumes one implicit session per open
  project — no new persistence or session-switching UI. Named as a future
  follow-up.
- **Editing arbitrary file types** in the Files tab. Only `.md` files get
  the full CodeMirror edit/save experience (unchanged from the old Design
  section). Every other file type is read-only in this pass — the user's
  own coding agent already edits code well via the terminal; InfinaBox's
  Files tab doesn't need to duplicate a general-purpose code editor.
- **Deleting the graph/agent backend infrastructure** (`infinabox_core::graph`,
  `ask_question`, `refresh_project_graph`). Nothing calls them from the UI
  after this redesign, but they stay in the codebase as tested, working
  infrastructure — a candidate to expose as an MCP server to the user's own
  CLI agent later, per the `agent_auth_model` memory. Not dead code to rip
  out, just currently unused by the UI.

## Architecture

```
App
├── TopBar (simplified: logo, Open Project, New task — no Project-menu dropdown)
└── main row (flex, min-h-0, flex-1)
    ├── ProjectWindow   (flex-1 — was FileBrowserPanel + ViewportPanel/SectionOverlay)
    │   ├── tab bar: Overview · Changes · Files
    │   ├── OverviewTab   (real project stats)
    │   ├── ChangesTab    (real commit history + diffs)
    │   └── FilesTab      (file tree + inspector; .md gets the existing editor)
    └── TerminalPanel   (flex-1 — unchanged, this is the Agent panel)
```

The console dock and the four section-overlay components
(`SectionOverlay`, `DesignSection`, `QaSection`, `BusinessSection`,
`LiveOpsSection`) are removed. `FileTree.tsx` and `MarkdownEditor.tsx` /
`MarkdownPreview.tsx` are kept and reused inside the new `FilesTab`.

### Removed files
`ConsoleDock.tsx`, `SectionOverlay.tsx`, `DesignSection.tsx`,
`QaSection.tsx`, `BusinessSection.tsx`, `LiveOpsSection.tsx`,
`FileBrowserPanel.tsx`, `ViewportPanel.tsx` (their logic is absorbed into
the new tabs, not carried over as separate components).

### New files
`src/components/cockpit/ProjectWindow.tsx` (tab shell + state for which
tab/file is active), `OverviewTab.tsx`, `ChangesTab.tsx`, `FilesTab.tsx`.

### Unchanged
`TerminalPanel.tsx`, `TopBar.tsx` (simplified, see below), `FileTree.tsx`,
`MarkdownEditor.tsx`, `MarkdownPreview.tsx`, all of `crates/core` and the
existing `list_directory` / `read_file` / `write_file` / terminal commands.

## Components

### TopBar (simplified)

Drops the "Project" section-switcher dropdown entirely — there are no more
sections to switch between, the tabs live inside `ProjectWindow` now. Keeps:
logo, the "Open Project" folder picker (unchanged), "New task" button
(still a placeholder — no behavior change, out of scope here).

### ProjectWindow

Owns which of the three tabs is active (`"overview" | "changes" | "files"`,
default `"overview"`) and, for the Files tab, which file is currently
selected (this replaces `DesignSection`'s `selectedDoc` state, generalized
to any file, not just `docs/gdd/*.md`). Renders the tab bar plus whichever
tab is active. Receives `projectPath` from `App`, same as today.

### OverviewTab

Real, computed stats — no invented numbers:
- Current branch name (new: a `current_branch` Tauri command using
  `git2`'s `repo.head()?.shorthand()`).
- Last commit's summary + author + relative time (first entry of the
  existing `walk_commits` result).
- Total commit count (`walk_commits(...).len()`).
- Total tracked file count (existing `list_directory`, counted recursively
  client-side, excluding directories).

Handles the "not a Git repo" case the same honest way `AgentPanel` used to
(before it was removed) — branch/commit stats show "not a Git repository"
rather than an error, while the file count still works (it has no Git
dependency).

### ChangesTab

A real commit history list — this is almost entirely reuse. New backend:
`list_recent_commits(project_path: String) -> Result<Vec<CommitInfo>, String>`,
a thin wrapper around the already-tested `infinabox_core::git_indexer::walk_commits`
(no new Git logic — `CommitInfo`/`FileChange` are already `Serialize`).
Frontend renders each commit (sha, summary, author, timestamp) in a list;
selecting one expands to show its `files_changed` (added/modified/deleted/
renamed — the same rename-aware data Phase 0 proved correct against the
`hollow-meridian-test` fixture).

Empty/error states: no project open → "no project open"; not a Git
repository → the same honest message already used elsewhere ("this folder
isn't a Git repository yet").

### FilesTab

The old `FileBrowserPanel` + `DesignSection` merged and generalized:
- Left column: the existing `FileTree` component, unchanged, now showing
  the whole project (not scoped to `docs/gdd`).
- Right side: a file inspector.
  - `.md` files: the existing `MarkdownEditor`/`MarkdownPreview` Edit/Preview
    tabs, full read/write/save — this is `DesignSection`'s `DocEditor`
    logic, moved here unchanged in behavior.
  - Every other text file: read-only view of `read_file`'s content in a
    plain monospace block (no CodeMirror language mode, no editing).
  - Binary files: `read_file` uses `std::fs::read_to_string`, which
    already fails on non-UTF-8 content — catch that and show "can't
    preview this file" rather than garbled bytes or a crash. No new
    backend command needed for this; it's the existing error path.
  - No file selected: "Select a file to inspect" empty state.

## Backend changes

Two new, small, thin Tauri commands in `src-tauri/src/commands/fs.rs`
(or a new `overview.rs` if `fs.rs` starts feeling crowded — implementer's
call):

```rust
#[tauri::command]
pub fn current_branch(path: String) -> Result<String, String> { ... }
// git2::Repository::open(path)?.head()?.shorthand() — mirrors the existing
// "not a git repo" error shape already used by refresh_project_graph.

#[tauri::command]
pub fn list_recent_commits(path: String) -> Result<Vec<CommitInfo>, String> {
    infinabox_core::git_indexer::walk_commits(&path).map_err(|e| e.to_string())
}
```

Both need real tests against the `hollow-meridian-test` fixture (current
branch name, commit count/order), following the same pattern as every
other command in this codebase — a real assertion against real fixture
data, not a mock.

## Testing

- Rust: unit tests for `current_branch` and `list_recent_commits` against
  the real fixture (branch name is deterministic once set; commit list
  reuses the already-proven `walk_commits`, so the new test only needs to
  confirm the thin wrapper passes through correctly).
- Frontend: `npm run build` clean, plus the same code-review-against-tested-
  contract discipline used throughout this project (this environment can't
  get a live screenshot of the running Tauri window — no interactive
  display session — so verification leans on real backend tests + careful
  review of the exact `invoke()` call shapes against them).
- No regression risk to `crates/core` or the terminal feature — this
  redesign only touches the frontend cockpit components and adds two
  read-only Tauri commands.

## Open questions for implementation (not blocking this spec)

None — the scope above is fully decided. Naming nit for whoever implements:
`TerminalPanel.tsx` keeps its name (it's an accurate description of what it
is); the "Agent panel" language in this document refers to its *role*, not
a rename.
