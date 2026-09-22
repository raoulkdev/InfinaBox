# InfinaBox: project status and roadmap

**Date:** 2026-09-22
**Type:** status report, not an implementation plan — there's no single next feature queued up right now. This document exists so a future session (human or agent) can read it and know where things actually stand, without re-deriving it from git history or re-trusting a stale plan.

## How this document was produced

By reading the real code as of this date: `src/App.tsx` and every component it wires up, `src-tauri/src/commands/*`, `crates/core/*`, `git log`/`git status`, both existing `docs/superpowers/` documents, and the older `wobbly-brewing-turtle` planning doc (kept outside the repo, in the user's global Claude config — historical context only, not current fact). Where this document disagrees with an older plan, the older plan is wrong about the present, not this one.

## Current state, by discipline

Reality as wired in `src/App.tsx` today:

| Section | State | What's actually there |
|---|---|---|
| **Home** | real | Recent-projects grid (`localStorage`), real `check_cli_tools` PATH detection for `claude`/`codex`/`git`/`gh`, New/Open Project flows gated on `.ibproject/.ibx` |
| **Build** | real | Permanently-mounted terminal (real PTY via `portable-pty`, never respawns/re-cwds) + `FileBrowser` in `codeEditor` mode (CodeMirror "mini VS Code") over the whole project, via `ResizablePanelGroup` |
| **Documents** (labeled "Documents", internally `design`) | real | `FileBrowser` over `.ibproject/docs`, `.md`-only WYSIWYG editing (MDXEditor) |
| **Graphs** | real | `FileBrowser` over `.ibproject/graphs`, ReactFlow canvas editor for `.graph.json` (custom node shapes, labeled edges) |
| **Business** | real, docs-only | `FileBrowser` clone over `.ibproject/business` |
| **Marketing** | real, docs-only | `FileBrowser` clone over `.ibproject/marketing` |
| **Community** | real, docs-only | `FileBrowser` clone over `.ibproject/community` |
| **Release** | real, docs-only | `FileBrowser` clone over `.ibproject/release` — no ship-state dashboard, no engine/release tool detection yet (both still candidates, see below) |
| **Art** | honest placeholder | `NotBuiltYetSection` — needs image preview, not a markdown stand-in |
| **Audio** | honest placeholder | `NotBuiltYetSection` — needs audio playback |
| **UI/UX** | honest placeholder | `NotBuiltYetSection` — same image-preview dependency as Art |
| **QA/Testing** | honest placeholder | `NotBuiltYetSection` — waiting on the graph/`ask_question` backend to get a real UI caller |
| **Live Ops** | honest placeholder | `NotBuiltYetSection` — inherently needs a shipped game generating real telemetry |

Backend infrastructure that's real, tested, and currently **unused by any frontend caller**: `crates/core/src/graph/*` (SQLite-backed project graph: commits + file identities that survive renames) and `crates/core/src/ask.rs` (deterministic, citation-only Q&A over that graph), exposed as the `refresh_project_graph`/`ask_question` Tauri commands. Confirmed via `grep` — no `.tsx`/`.ts` file under `src/` invokes either. This matches the `agent_auth_model` project memory's framing: the intended next step for this pair is exposure as an MCP server for the user's own `claude`/`codex` session to call directly, not a new in-app "ask" widget.

## What changed from the original plans

Two things are worth separating clearly, because they're easy to conflate:

### 1. An entire architecture was built, then replaced, before the current work started

`docs/superpowers/specs/2026-09-12-workspace-redesign-design.md` and its paired plan (`docs/superpowers/plans/2026-09-12-workspace-redesign.md`) are **not** stale documents describing something that never happened — they describe a real redesign that was fully implemented and committed (commits `f8fe0a7` through `139fe13` in `git log`): a `ProjectWindow` with three tabs (Overview/Changes/Files), replacing an earlier per-discipline sidebar. That design itself explicitly superseded the chat-panel-based cockpit that came before it.

The **current uncommitted working tree** replaces that ProjectWindow design again, with the sidebar/`FileBrowser`-per-discipline approach described in "Current state" above. Concretely:

- `ProjectWindow.tsx`, `OverviewTab.tsx`, `ChangesTab.tsx` (477 lines combined) are still present in the tree and still compile, but **nothing imports them anymore** — confirmed by grep, the only remaining references are in comments (`BuildRow.tsx`'s own comment names them explicitly as "still exist... just unused here"). `FileTree.tsx`, `FilesTab.tsx`, `MarkdownPreview.tsx`, and `TopBar.tsx` were deleted outright in this same uncommitted change.
- So there have now been **three** distinct top-level UI architectures for this app across its history: chat-panel cockpit → ProjectWindow (Overview/Changes/Files tabs) → sidebar + per-discipline `FileBrowser` (current). Anyone reading the 2026-09-12 spec should know it describes the middle one, already superseded.

### 2. The sidebar/discipline redesign matches — and has already gone further than — the `wobbly-brewing-turtle` plan

That plan (13 game-dev disciplines, grouped Make/Ship/Grow sidebar, `FileBrowser`-rooted-at-a-folder as the cheap real slice) is **largely already built**, not just planned:

- Phase 0 (sidebar rewrite: pinned Home/Build, grouped labeled `Make`/`Ship`/`Grow` sections, `ScrollArea`) — done, matches `Sidebar.tsx` almost exactly, including the group labels and pinned-Home/Build positioning.
- Phase 1 (pure markdown-docs clones: Business, Marketing, Community, Release docs-half) — done.
- The plan's explicit exclusion of Art/Audio/UI-UX from a markdown-only phase, on the grounds that a markdown stand-in for image/audio content would be "a weak, arguably dishonest substitute" — **honored**: those three are still `NotBuiltYetSection`, not markdown clones.

Two real divergences from that plan, worth flagging so nobody assumes 1:1 fidelity:

- **"Direction & Production" and "Level/World Development"** (2 of the plan's original 13 disciplines) **don't exist** in the current sidebar at all — there's no `direction` or `level` section anywhere in `Sidebar.tsx`'s `Section` union.
- **"Graphs"** exists and is fully real — and it isn't in the old plan's 13 disciplines at all. It appears to have been added independently as a general-purpose canvas/diagramming primitive (flowcharts, dependency maps), not tied to any one discipline. Net effect: the current sidebar has 13 entries again (Home + Build + 11 disciplines), but it's not the *same* 13 the old plan enumerated.

### 3. Build's Files panel became something different than either old plan expected

Neither the ProjectWindow spec's `FilesTab` (tree + read-only-except-`.md` inspector) nor the turtle plan's assumed Finder-style grid is what Build has today. `BuildRow.tsx`'s own comment is explicit: the Files panel became `FileBrowser`'s `codeEditor` mode — a CodeMirror-backed "mini VS Code" that opens *any* readable text file for editing, not just `.md`. And Overview/Changes (git branch/commit data) was **removed from Build "for now"** per the same comment — the components (`ProjectWindow`/`OverviewTab`/`ChangesTab`) are kept around unused rather than deleted, specifically so that decision is reversible.

## What's still genuinely unbuilt — candidate next initiatives

These are presented as candidates, not commitments. Confirmed unbuilt by reading the current code, not by trusting the old plan's claims about it:

- **A real MCP server exposing the graph/`ask_question` pair.** `crates/core/src/mcp_client.rs` + `crates/cli/src/bin/mock_godot_mcp.rs` still only prove a one-shot stdio JSON round-trip against a mock server — not a real MCP handshake/capability-negotiation/tool-discovery lifecycle, and not a real Godot integration. No `crates/mcp-server` exists. This is the most-referenced "next" direction across both the design spec and the `agent_auth_model` memory — expose `ask_question`/`refresh_project_graph` as MCP tools for the user's own terminal-based agent to call, rather than building any new in-app UI for them.
- **QA's real content.** `ask_question`/`refresh_project_graph` remain fully unwired from any frontend caller. Note this is a slight update to the old plan's framing: the old plan proposed a first in-app "ask" widget as QA's cheap win; the `agent_auth_model` memory (written after that plan) argues against reviving any in-app agent-interaction UI, favoring the MCP-server direction instead. These two ideas are in tension — worth a real decision before building either.
- **Release's real tooling.** `check_cli_tools`/`is_on_path` (`src-tauri/src/commands/environment.rs`) only checks `claude`/`codex`/`git`/`gh` today (used by the Home dashboard). Extending that pattern to engine/release tools (`godot`, `butler`, `steamcmd`, etc.) plus a real branch/last-commit "ship state" card on the Release section — still just an idea, `ReleaseSection.tsx` is docs-only.
- **Image/audio asset preview.** Confirmed by reading `FileGrid.tsx`/`FileList.tsx`: no image rendering, no `<audio>` element, no `convertFileSrc` usage anywhere. This is the real blocker for Art, Audio, and UI/UX moving off `NotBuiltYetSection` — all three need the same underlying capability (Tauri asset-protocol preview in the inspector) rather than three separate builds.
- **"Direction & Production" and "Level/World Development"** as disciplines — not present at all, not even as placeholders. Unclear whether this is a deliberate scope-narrowing or just not gotten to yet; worth asking rather than assuming.

## Known rough edges / things worth attention

- **The working tree has a large amount of uncommitted work right now** (`git status` at the time of writing: 21 modified files, 3 deletions, 31 untracked files, including a 9,206-line `package-lock.json` diff). Read closely, this reads as **one coherent, internally-consistent feature** — the sidebar/discipline redesign described above, fully wired end-to-end (`Sidebar.tsx`, `App.tsx`, every `*Section.tsx`, `FileBrowser.tsx` and its supporting `FileGrid`/`FileList`/`CodeEditor`/`GraphEditor`, the fs-watcher pair, several new `ui/` primitives) — not a grab-bag of unrelated changes. `npx tsc --noEmit`, `npm run build`, and `cargo check`/`cargo test --workspace` (both `--no-default-features`) all pass cleanly against it (one pre-existing fixture-data test failure, unrelated — see below). That said, it's a genuinely large amount of uncommitted surface area to be sitting in a working tree rather than history — worth committing (likely as more than one commit, given it both adds the new architecture and deletes the old one) sooner rather than later, if only so `git log`/`git blame` stay useful and a crash or a bad `git clean` doesn't lose it.
- **`ProjectWindow.tsx`, `OverviewTab.tsx`, `ChangesTab.tsx` are dead code as of the uncommitted changes** — they compile, but nothing imports them (verified by grep across `src/`). They're kept intentionally per `BuildRow.tsx`'s comment (Overview/Changes might come back to Build), but a future session encountering them via search should know they're currently inert, not a hidden second UI.
- **One Rust test fails locally**, environment-dependent, not a code bug: `commands::fs::tests::reads_a_real_gdd_doc_from_the_fixture` expects `/Users/raoulkaleba/Developer/hollow-meridian-test/.ibproject/docs/gdd/sector-3-verticality.md` to exist; the fixture repo is present on this machine but that specific file isn't. Every other fixture-dependent test (git branch/commits, graph ingest/ask, directory listing) passes fine against the same fixture.
- **`get_default_project_path`** (`src-tauri/src/commands/fs.rs`) is still registered as a Tauri command and hardcodes the same personal fixture path, but has zero frontend callers anymore (superseded by the real Open/New Project flow with `.ibx` validation). Harmless, but it's a leftover worth removing whenever that file is next touched.
- **No CI configuration exists** (`.github/` doesn't exist in this repo) — every "build passes" claim in this document and in `CLAUDE.md` is from a manual local run, not an enforced gate. `cargo check --no-default-features` is the established local convention, but nothing currently codifies *why* versus plain `cargo check` (both pass locally as of this date).
- **Frontend bundle size**: `npm run build` succeeds but warns about a ~2MB main chunk (gzipped ~655KB) — MDXEditor, CodeMirror (with a large set of language packages), ReactFlow, and xterm.js all currently ship in one chunk with no code-splitting. Not urgent, but will only get more noticeable as more sections gain real editors.
- **No LLM integration exists anywhere in the codebase.** `crates/core/src/ask.rs` is deliberately a rule-based, deterministic lookup — its own doc comment explains this is by design (proving the grounding/citation loop, not model fluency) and sketches how a real model call would slot in later behind the same evidence-gathering. Worth remembering next time someone assumes "ask" means an LLM is already wired up somewhere.
