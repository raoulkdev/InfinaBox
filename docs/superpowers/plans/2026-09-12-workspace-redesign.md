# Agent Panel + Project Window Workspace Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace InfinaBox's fixed cockpit (file browser + Build/Design/QA/Business/Live Ops sections + console) with a two-pane session: the existing terminal (Agent panel, unchanged) paired with a new Project window (Overview / Changes / Files tabs).

**Architecture:** `App` renders `TopBar` (simplified — no more section dropdown) and a row of `ProjectWindow` + `TerminalPanel`, each `flex-1` so they split evenly. `ProjectWindow` is a tab shell around three new components (`OverviewTab`, `ChangesTab`, `FilesTab`) that recompose already-tested logic (the Git indexer, the CodeMirror GDD editor, the file tree) rather than rebuilding it. Two new thin Tauri commands (`current_branch`, `list_recent_commits`) back Overview/Changes; both delegate to already-tested `infinabox_core` functions.

**Tech Stack:** Rust (Tauri v2, `git2`), React 19 + TypeScript, shadcn/ui `Tabs`, existing `@xterm/xterm` terminal (untouched by this plan).

**Spec:** `docs/superpowers/specs/2026-09-12-workspace-redesign-design.md`

## Global Constraints

- Every stat/status shown must be real — no fabricated data, ever (spec: Goals).
- Only `.md` files get full edit/save in the Files tab; every other text file is read-only; binary files show an honest "can't preview" message (spec: Non-goals, FilesTab).
- `infinabox_core::graph`, `ask_question`, and `refresh_project_graph` are NOT touched or deleted by this plan — they stay as unused-by-UI-but-tested infrastructure (spec: Non-goals).
- `TerminalPanel.tsx` keeps its name and is not modified by this plan.
- This codebase has no frontend test runner (no Vitest/RTL installed) — frontend tasks are verified via `npm run build` (zero type errors) plus careful review of `invoke()` call shapes against the Rust commands' real, tested signatures, matching how every prior frontend feature in this project was verified. Backend tasks get real Rust `#[test]`s, per this codebase's established practice throughout Phase 0/1 (every command has been backed by a test against the real `hollow-meridian-test` fixture, not a mock).
- Fixture repo for all backend tests: `/Users/raoulkaleba/Developer/hollow-meridian-test` (a real Git repo with real, evolving history — do not hardcode its exact commit count in a new test; if you need a count, compute it from a live call the same way `crates/core`'s existing tests do).

---

## Task 1: `current_branch` in the core Git indexer

**Files:**
- Modify: `crates/core/src/git_indexer.rs` (add a new public function + test; do not touch existing functions)

**Interfaces:**
- Produces: `pub fn current_branch(path: &str) -> anyhow::Result<String>` — used by Task 3.

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` block at the bottom of `crates/core/src/git_indexer.rs`:

```rust
    #[test]
    fn current_branch_matches_real_git() {
        let path = "/Users/raoulkaleba/Developer/hollow-meridian-test";
        let branch = current_branch(path).expect("fixture should have a valid current branch");

        let output = std::process::Command::new("git")
            .args(["-C", path, "branch", "--show-current"])
            .output()
            .expect("git should be runnable on this machine");
        let expected = String::from_utf8_lossy(&output.stdout).trim().to_string();

        assert_eq!(
            branch, expected,
            "current_branch should match `git branch --show-current`"
        );
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd /Users/raoulkaleba/Developer/InfinaBox && cargo test -p infinabox-core current_branch_matches_real_git`
Expected: FAIL to compile — `current_branch` is not defined yet.

- [ ] **Step 3: Write minimal implementation**

Add this function above the `#[cfg(test)]` block in `crates/core/src/git_indexer.rs` (near `walk_commits`, since it uses the same `Repository::open` pattern):

```rust
/// Returns the short name of the repository's current branch (e.g. "main"),
/// resolved the same way `git branch --show-current` does.
pub fn current_branch(path: &str) -> Result<String> {
    let repo = Repository::open(path)
        .with_context(|| format!("failed to open Git repository at '{path}'"))?;
    let head = repo.head().context("failed to read repository HEAD")?;
    head.shorthand()
        .map(|s| s.to_string())
        .context("HEAD's branch name is not valid UTF-8")
}
```

This uses `Repository`, `Result`, and `Context`/`context` — all already imported at the top of this file for `walk_commits`. Do not add new imports.

- [ ] **Step 4: Run test to verify it passes**

Run: `cd /Users/raoulkaleba/Developer/InfinaBox && cargo test -p infinabox-core current_branch_matches_real_git`
Expected: PASS (1 passed)

- [ ] **Step 5: Run the full core test suite to confirm nothing broke**

Run: `cd /Users/raoulkaleba/Developer/InfinaBox && cargo test -p infinabox-core`
Expected: all tests pass (the pre-existing git indexer/graph/ask tests plus this new one).

- [ ] **Step 6: Commit**

```bash
cd /Users/raoulkaleba/Developer/InfinaBox
git add crates/core/src/git_indexer.rs
git commit -m "core: add current_branch, tested against real git branch --show-current"
```

---

## Task 2: Lock down the binary-file error message

**Why:** the Files tab (Task 7) needs to detect "this is a binary file" by matching on `read_file`'s error text. That text comes from Rust's standard library (`std::fs::read_to_string`), not from code we wrote — this task proves the exact string today so the frontend's detection is built on a verified fact, not an assumption.

**Files:**
- Modify: `src-tauri/src/commands/fs.rs` (add one test to the existing `#[cfg(test)] mod tests` block; do not touch `read_file`, `write_file`, `list_directory`, or `get_default_project_path`)

**Interfaces:**
- Produces: confirms `read_file`'s error text for non-UTF-8 content contains the substring `"stream did not contain valid utf-8"` (case-insensitive) — Task 7's frontend code relies on this exact substring.

- [ ] **Step 1: Write the test**

Add to the `#[cfg(test)] mod tests` block in `src-tauri/src/commands/fs.rs`:

```rust
    #[test]
    fn read_file_error_message_for_binary_content_contains_expected_substring() {
        let dir = std::env::temp_dir().join(format!(
            "infinabox-fs-binary-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("not_text.bin").to_string_lossy().into_owned();

        // Invalid UTF-8 byte sequence — 0xFF is never valid as a UTF-8
        // continuation or lead byte.
        std::fs::write(&path, [0xFFu8, 0xFE, 0xFD, 0x00]).unwrap();

        let err = read_file(path).expect_err("reading binary content as a string should fail");
        assert!(
            err.to_lowercase().contains("stream did not contain valid utf-8"),
            "expected the standard library's UTF-8 error text, got: {err}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
```

- [ ] **Step 2: Run the test**

Run: `cd /Users/raoulkaleba/Developer/InfinaBox && cargo test -p tauri-app read_file_error_message_for_binary_content`
Expected: PASS. If it fails because the substring doesn't match, read the actual printed error text from the assertion failure and use that exact substring in this test AND note the real substring in your Task 7 report — do not force a match by lying about what the string contains.

- [ ] **Step 3: Run the full src-tauri test suite to confirm nothing broke**

Run: `cd /Users/raoulkaleba/Developer/InfinaBox && cargo test -p tauri-app`
Expected: all existing tests still pass, plus this new one.

- [ ] **Step 4: Commit**

```bash
cd /Users/raoulkaleba/Developer/InfinaBox
git add src-tauri/src/commands/fs.rs
git commit -m "fs: lock down read_file's binary-content error text with a real test"
```

---

## Task 3: `current_branch` + `list_recent_commits` Tauri commands

**Files:**
- Create: `src-tauri/src/commands/overview.rs`
- Modify: `src-tauri/src/commands/mod.rs` (add `pub mod overview;`)
- Modify: `src-tauri/src/lib.rs` (import and register the two new commands)

**Interfaces:**
- Consumes: `infinabox_core::git_indexer::current_branch(path: &str) -> anyhow::Result<String>` (Task 1), `infinabox_core::git_indexer::walk_commits(path: &str) -> anyhow::Result<Vec<CommitInfo>>` (already exists, unchanged), `infinabox_core::git_indexer::CommitInfo` (already `#[derive(Serialize, Clone, Debug)]`, unchanged).
- Produces: Tauri commands `current_branch(path: String) -> Result<String, String>` and `list_recent_commits(path: String) -> Result<Vec<CommitInfo>, String>`, both registered in `invoke_handler` — consumed by Tasks 5 and 6.

- [ ] **Step 1: Write the failing tests**

Create `src-tauri/src/commands/overview.rs` with this full content (implementation + tests together, since these are thin wrappers — writing the test first here means writing an empty/stub function first):

```rust
//! Thin Tauri commands backing the Project window's Overview and Changes
//! tabs. All the real Git logic already lives in infinabox_core and is
//! tested there — these commands only convert its `anyhow::Result` into
//! the `Result<T, String>` shape Tauri commands need.

use infinabox_core::git_indexer::{self, CommitInfo};

#[tauri::command]
pub fn current_branch(path: String) -> Result<String, String> {
    todo!("delegate to infinabox_core::git_indexer::current_branch, map the error to String")
}

#[tauri::command]
pub fn list_recent_commits(path: String) -> Result<Vec<CommitInfo>, String> {
    todo!("delegate to infinabox_core::git_indexer::walk_commits, map the error to String")
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE_REPO: &str = "/Users/raoulkaleba/Developer/hollow-meridian-test";

    #[test]
    fn current_branch_command_delegates_correctly() {
        let branch = current_branch(FIXTURE_REPO.to_string()).expect("should resolve a branch");
        assert!(!branch.is_empty(), "branch name should not be empty");
    }

    #[test]
    fn list_recent_commits_command_delegates_correctly() {
        let commits =
            list_recent_commits(FIXTURE_REPO.to_string()).expect("should list real commits");
        assert!(!commits.is_empty(), "the fixture has real commit history");
    }

    #[test]
    fn both_commands_fail_cleanly_against_a_non_git_folder() {
        let dir = std::env::temp_dir().join(format!(
            "infinabox-overview-non-git-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let branch_result = current_branch(dir.to_string_lossy().into_owned());
        assert!(branch_result.is_err(), "a non-Git folder must not resolve a branch");

        let commits_result = list_recent_commits(dir.to_string_lossy().into_owned());
        assert!(commits_result.is_err(), "a non-Git folder must not list commits");

        std::fs::remove_dir_all(&dir).ok();
    }
}
```

- [ ] **Step 2: Wire the new module in, then run the tests to see them fail on `todo!()`**

Edit `src-tauri/src/commands/mod.rs` — it currently reads:
```rust
pub mod fs;
pub mod project;
pub mod terminal;
```
Change it to:
```rust
pub mod fs;
pub mod overview;
pub mod project;
pub mod terminal;
```

Run: `cd /Users/raoulkaleba/Developer/InfinaBox && cargo test -p tauri-app overview::tests`
Expected: compiles, then FAILS at runtime with a `not yet implemented` panic from the `todo!()` calls.

- [ ] **Step 3: Implement the two commands for real**

Replace the two `todo!()` function bodies in `src-tauri/src/commands/overview.rs` with:

```rust
#[tauri::command]
pub fn current_branch(path: String) -> Result<String, String> {
    git_indexer::current_branch(&path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_recent_commits(path: String) -> Result<Vec<CommitInfo>, String> {
    git_indexer::walk_commits(&path).map_err(|e| e.to_string())
}
```

(Leave the `use infinabox_core::git_indexer::{self, CommitInfo};` import line and all three `#[test]` functions exactly as they are.)

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd /Users/raoulkaleba/Developer/InfinaBox && cargo test -p tauri-app overview::tests`
Expected: `test result: ok. 3 passed; 0 failed`

- [ ] **Step 5: Register both commands in the app**

Edit `src-tauri/src/lib.rs`. It currently reads:

```rust
mod commands;

use commands::fs::{get_default_project_path, list_directory, read_file, write_file};
use commands::project::{ask_question, refresh_project_graph};
use commands::terminal::{resize_terminal, spawn_terminal, write_to_terminal, TerminalState};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(TerminalState::default())
        .invoke_handler(tauri::generate_handler![
            list_directory,
            get_default_project_path,
            read_file,
            write_file,
            refresh_project_graph,
            ask_question,
            spawn_terminal,
            write_to_terminal,
            resize_terminal,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

Change it to (adding the `overview` import and the two commands in the handler list):

```rust
mod commands;

use commands::fs::{get_default_project_path, list_directory, read_file, write_file};
use commands::overview::{current_branch, list_recent_commits};
use commands::project::{ask_question, refresh_project_graph};
use commands::terminal::{resize_terminal, spawn_terminal, write_to_terminal, TerminalState};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(TerminalState::default())
        .invoke_handler(tauri::generate_handler![
            list_directory,
            get_default_project_path,
            read_file,
            write_file,
            refresh_project_graph,
            ask_question,
            spawn_terminal,
            write_to_terminal,
            resize_terminal,
            current_branch,
            list_recent_commits,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 6: Run the full workspace build and test suite**

Run: `cd /Users/raoulkaleba/Developer/InfinaBox && cargo build --workspace && cargo test --workspace`
Expected: builds with no errors, all tests pass (this now includes Task 1's, Task 2's, and this task's new tests).

- [ ] **Step 7: Commit**

```bash
cd /Users/raoulkaleba/Developer/InfinaBox
git add src-tauri/src/commands/overview.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git commit -m "Add current_branch and list_recent_commits Tauri commands"
```

---

## Task 4: Frontend `CommitInfo`/`FileChange` types

**Files:**
- Create: `src/types/git.ts`

**Interfaces:**
- Produces: `CommitInfo`, `FileChange` TypeScript types, matching the JSON shape `infinabox_core::git_indexer::CommitInfo`/`FileChange` actually serialize to (already proven correct by Phase 0's CLI — the shapes below are not new, just typed for the frontend for the first time). Consumed by Tasks 5 and 6.

- [ ] **Step 1: Write the file**

```typescript
export interface CommitInfo {
  sha: string;
  short_sha: string;
  author_name: string;
  author_email: string;
  summary: string;
  message: string;
  timestamp: string;
  files_changed: FileChange[];
}

export type FileChange =
  | { status: "added"; path: string }
  | { status: "deleted"; path: string }
  | { status: "modified"; path: string }
  | { status: "renamed"; from: string; to: string }
  | { status: "copied"; from: string; to: string }
  | { status: "typechange"; path: string }
  | { status: "other"; path: string; kind: string };
```

- [ ] **Step 2: Verify it compiles on its own**

Run: `cd /Users/raoulkaleba/Developer/InfinaBox && npx tsc --noEmit src/types/git.ts`
Expected: no output (no errors). This file isn't imported anywhere yet, so this only checks its own syntax — the real cross-check against the Rust JSON shape happens when Tasks 5/6 actually deserialize real data through it.

- [ ] **Step 3: Commit**

```bash
cd /Users/raoulkaleba/Developer/InfinaBox
git add src/types/git.ts
git commit -m "Add CommitInfo/FileChange frontend types"
```

---

## Task 5: `OverviewTab` component

**Files:**
- Create: `src/components/cockpit/OverviewTab.tsx`

**Interfaces:**
- Consumes: `invoke<string>("current_branch", { path })`, `invoke<CommitInfo[]>("list_recent_commits", { path })`, `invoke<FileEntry[]>("list_directory", { path })` (all from Task 3 and pre-existing commands); `CommitInfo` from `@/types/git` (Task 4); `FileEntry` from `@/types/fs` (pre-existing).
- Produces: `export function OverviewTab({ projectPath }: { projectPath: string | null })` — consumed by Task 8 (`ProjectWindow`).

- [ ] **Step 1: Write the component**

```tsx
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { CommitInfo } from "@/types/git";
import type { FileEntry } from "@/types/fs";

interface OverviewTabProps {
  projectPath: string | null;
}

type GitState =
  | { status: "loading" }
  | { status: "not-git" }
  | { status: "ready"; branch: string; commitCount: number; lastCommit: CommitInfo | null };

type FileCountState =
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "ready"; count: number };

function countFiles(entries: FileEntry[]): number {
  let count = 0;
  for (const entry of entries) {
    count += entry.is_dir ? countFiles(entry.children ?? []) : 1;
  }
  return count;
}

function relativeTime(iso: string): string {
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return iso;
  const diffMin = Math.round((Date.now() - then) / 60000);
  if (diffMin < 1) return "just now";
  if (diffMin < 60) return `${diffMin}m ago`;
  const diffHr = Math.round(diffMin / 60);
  if (diffHr < 24) return `${diffHr}h ago`;
  return `${Math.round(diffHr / 24)}d ago`;
}

function StatRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between border-b border-border px-3 py-2 last:border-b-0">
      <span className="text-sm text-muted-foreground">{label}</span>
      <span className="max-w-[60%] truncate text-sm text-foreground/90" title={value}>
        {value}
      </span>
    </div>
  );
}

export function OverviewTab({ projectPath }: OverviewTabProps) {
  const [git, setGit] = useState<GitState>({ status: "loading" });
  const [files, setFiles] = useState<FileCountState>({ status: "loading" });

  useEffect(() => {
    if (!projectPath) {
      setGit({ status: "not-git" });
      setFiles({ status: "error", message: "No project open." });
      return;
    }

    let cancelled = false;
    setGit({ status: "loading" });
    setFiles({ status: "loading" });

    async function loadGit() {
      try {
        const [branch, commits] = await Promise.all([
          invoke<string>("current_branch", { path: projectPath }),
          invoke<CommitInfo[]>("list_recent_commits", { path: projectPath }),
        ]);
        if (!cancelled) {
          setGit({
            status: "ready",
            branch,
            commitCount: commits.length,
            lastCommit: commits[0] ?? null,
          });
        }
      } catch {
        // Either command fails the same way for a non-Git folder — a real,
        // expected case now that any folder can be opened.
        if (!cancelled) setGit({ status: "not-git" });
      }
    }

    async function loadFiles() {
      try {
        const entries = await invoke<FileEntry[]>("list_directory", { path: projectPath });
        if (!cancelled) setFiles({ status: "ready", count: countFiles(entries) });
      } catch (err) {
        if (!cancelled) {
          setFiles({
            status: "error",
            message: err instanceof Error ? err.message : String(err),
          });
        }
      }
    }

    void loadGit();
    void loadFiles();

    return () => {
      cancelled = true;
    };
  }, [projectPath]);

  return (
    <div className="flex flex-1 flex-col overflow-y-auto">
      <div className="border-b border-border">
        {git.status === "loading" && (
          <p className="px-3 py-2 text-sm text-muted-foreground">loading…</p>
        )}
        {git.status === "not-git" && (
          <p className="px-3 py-2 text-sm text-muted-foreground">
            This folder isn't a Git repository yet.
          </p>
        )}
        {git.status === "ready" && (
          <>
            <StatRow label="Branch" value={git.branch} />
            <StatRow label="Commits" value={String(git.commitCount)} />
            {git.lastCommit && (
              <StatRow
                label="Last commit"
                value={`${git.lastCommit.summary} — ${git.lastCommit.author_name}, ${relativeTime(git.lastCommit.timestamp)}`}
              />
            )}
          </>
        )}
      </div>
      <div>
        {files.status === "loading" && (
          <p className="px-3 py-2 text-sm text-muted-foreground">loading…</p>
        )}
        {files.status === "error" && (
          <p className="px-3 py-2 text-sm text-destructive">{files.message}</p>
        )}
        {files.status === "ready" && <StatRow label="Files" value={String(files.count)} />}
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cd /Users/raoulkaleba/Developer/InfinaBox && npx tsc --noEmit`
Expected: no new errors attributable to this file (it isn't imported by `App.tsx` yet, so this only checks the file's own correctness in isolation — ignore pre-existing unrelated errors, if any, but there should be none at this point in the plan).

- [ ] **Step 3: Commit**

```bash
cd /Users/raoulkaleba/Developer/InfinaBox
git add src/components/cockpit/OverviewTab.tsx
git commit -m "Add OverviewTab: real branch/commit/file stats"
```

---

## Task 6: `ChangesTab` component

**Files:**
- Create: `src/components/cockpit/ChangesTab.tsx`

**Interfaces:**
- Consumes: `invoke<CommitInfo[]>("list_recent_commits", { path })` (Task 3); `CommitInfo`, `FileChange` from `@/types/git` (Task 4); shadcn `ScrollArea` (`@/components/ui/scroll-area`, pre-existing); `cn` from `@/lib/utils` (pre-existing).
- Produces: `export function ChangesTab({ projectPath }: { projectPath: string | null })` — consumed by Task 8.

- [ ] **Step 1: Write the component**

```tsx
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ChevronRight } from "lucide-react";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "@/lib/utils";
import type { CommitInfo, FileChange } from "@/types/git";

interface ChangesTabProps {
  projectPath: string | null;
}

type ChangesState =
  | { status: "empty" }
  | { status: "loading" }
  | { status: "not-git" }
  | { status: "ready"; commits: CommitInfo[] };

function fileChangeLabel(change: FileChange): string {
  if (change.status === "renamed" || change.status === "copied") {
    return `${change.from} → ${change.to}`;
  }
  return change.path;
}

function CommitRow({ commit }: { commit: CommitInfo }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="border-b border-border">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center gap-2 px-3 py-2 text-left hover:bg-accent"
      >
        <ChevronRight
          className={cn(
            "size-3.5 shrink-0 text-muted-foreground transition-transform",
            open && "rotate-90",
          )}
        />
        <span className="shrink-0 font-mono text-xs text-muted-foreground">
          {commit.short_sha}
        </span>
        <span className="truncate text-sm text-foreground/90">{commit.summary}</span>
        <span className="ml-auto shrink-0 text-xs text-muted-foreground">
          {commit.author_name}
        </span>
      </button>
      {open && (
        <div className="flex flex-col gap-1 border-t border-border bg-background px-3 py-2 pl-9">
          {commit.files_changed.length === 0 && (
            <span className="text-xs text-muted-foreground">No file changes recorded.</span>
          )}
          {commit.files_changed.map((change, i) => (
            <span key={i} className="font-mono text-xs text-muted-foreground">
              [{change.status}] {fileChangeLabel(change)}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

export function ChangesTab({ projectPath }: ChangesTabProps) {
  const [state, setState] = useState<ChangesState>({ status: "empty" });

  useEffect(() => {
    if (!projectPath) {
      setState({ status: "empty" });
      return;
    }

    let cancelled = false;
    setState({ status: "loading" });

    async function load() {
      try {
        const commits = await invoke<CommitInfo[]>("list_recent_commits", {
          path: projectPath,
        });
        if (!cancelled) setState({ status: "ready", commits });
      } catch {
        if (!cancelled) setState({ status: "not-git" });
      }
    }

    void load();
    return () => {
      cancelled = true;
    };
  }, [projectPath]);

  return (
    <ScrollArea className="min-h-0 flex-1">
      {state.status === "empty" && (
        <p className="px-3 py-2 text-sm text-muted-foreground">No project open.</p>
      )}
      {state.status === "loading" && (
        <p className="px-3 py-2 text-sm text-muted-foreground">loading…</p>
      )}
      {state.status === "not-git" && (
        <p className="px-3 py-2 text-sm text-muted-foreground">
          This folder isn't a Git repository yet.
        </p>
      )}
      {state.status === "ready" && state.commits.length === 0 && (
        <p className="px-3 py-2 text-sm text-muted-foreground">No commits yet.</p>
      )}
      {state.status === "ready" &&
        state.commits.map((c) => <CommitRow key={c.sha} commit={c} />)}
    </ScrollArea>
  );
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cd /Users/raoulkaleba/Developer/InfinaBox && npx tsc --noEmit`
Expected: no new errors attributable to this file.

- [ ] **Step 3: Commit**

```bash
cd /Users/raoulkaleba/Developer/InfinaBox
git add src/components/cockpit/ChangesTab.tsx
git commit -m "Add ChangesTab: real commit history with expandable diffs"
```

---

## Task 7: `FilesTab` component (file tree + inspector)

**Files:**
- Create: `src/components/cockpit/FilesTab.tsx`

**Interfaces:**
- Consumes: `FileTree` from `@/components/cockpit/FileTree` (pre-existing, already supports optional `selectedPath`/`onSelectFile` props — do not modify it), `MarkdownEditor` from `@/components/cockpit/MarkdownEditor` (pre-existing, unchanged), `MarkdownPreview` from `@/components/cockpit/MarkdownPreview` (pre-existing, unchanged), `invoke<FileEntry[]>("list_directory", { path })`, `invoke<string>("read_file", { path })`, `invoke("write_file", { path, contents })` (all pre-existing), `FileEntry` from `@/types/fs` (pre-existing), the exact substring `"stream did not contain valid utf-8"` locked down by Task 2's test (case-insensitive match).
- Produces: `export function FilesTab({ projectPath }: { projectPath: string | null })` — consumed by Task 8.

- [ ] **Step 1: Write the component**

```tsx
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { FolderOpen } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { FileTree } from "@/components/cockpit/FileTree";
import { MarkdownEditor } from "@/components/cockpit/MarkdownEditor";
import { MarkdownPreview } from "@/components/cockpit/MarkdownPreview";
import type { FileEntry } from "@/types/fs";

interface FilesTabProps {
  projectPath: string | null;
}

type TreeState =
  | { status: "empty" }
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "ready"; entries: FileEntry[] };

type InspectorState =
  | { status: "loading" }
  | { status: "binary" }
  | { status: "error"; message: string }
  | { status: "ready"; savedContent: string };

function isMarkdown(path: string): boolean {
  return /\.md$/i.test(path);
}

function fileName(path: string): string {
  return path.split("/").pop() ?? path;
}

// Locked down by a real Rust test (see src-tauri/src/commands/fs.rs,
// read_file_error_message_for_binary_content_contains_expected_substring) —
// this is std::fs::read_to_string's actual, real error text for non-UTF-8
// content, not a guess.
function isLikelyBinaryError(message: string): boolean {
  return message.toLowerCase().includes("stream did not contain valid utf-8");
}

function FileInspector({ path }: { path: string }) {
  const [state, setState] = useState<InspectorState>({ status: "loading" });
  const [draft, setDraft] = useState("");
  const [saveError, setSaveError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setState({ status: "loading" });
    setSaveError(null);

    async function load() {
      try {
        const contents = await invoke<string>("read_file", { path });
        if (!cancelled) {
          setState({ status: "ready", savedContent: contents });
          setDraft(contents);
        }
      } catch (err) {
        if (!cancelled) {
          const message = err instanceof Error ? err.message : String(err);
          setState(
            isLikelyBinaryError(message)
              ? { status: "binary" }
              : { status: "error", message },
          );
        }
      }
    }

    void load();
    return () => {
      cancelled = true;
    };
  }, [path]);

  const editable = isMarkdown(path);
  const dirty = editable && state.status === "ready" && draft !== state.savedContent;

  async function save() {
    if (state.status !== "ready" || saving) return;
    setSaving(true);
    setSaveError(null);
    try {
      await invoke("write_file", { path, contents: draft });
      setState({ status: "ready", savedContent: draft });
    } catch (err) {
      setSaveError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="flex h-full min-w-0 flex-1 flex-col">
      <div className="flex h-8 shrink-0 items-center justify-between border-b border-border px-3">
        <div className="flex min-w-0 items-center gap-2">
          <span className="truncate text-sm text-foreground/90" title={path}>
            {fileName(path)}
          </span>
          {dirty && <Badge variant="outline">unsaved</Badge>}
        </div>
        {editable && (
          <Button
            size="xs"
            variant="secondary"
            onClick={() => void save()}
            disabled={state.status !== "ready" || !dirty || saving}
          >
            {saving ? "Saving…" : "Save"}
          </Button>
        )}
      </div>

      {saveError && (
        <p className="shrink-0 border-b border-border bg-destructive/10 px-3 py-1.5 text-xs text-destructive">
          Save failed: {saveError}
        </p>
      )}

      {state.status === "loading" && (
        <div className="flex flex-1 items-center justify-center">
          <p className="text-sm text-muted-foreground">loading…</p>
        </div>
      )}

      {state.status === "binary" && (
        <div className="flex flex-1 items-center justify-center">
          <p className="text-sm text-muted-foreground">
            Can't preview this file — it looks like binary content.
          </p>
        </div>
      )}

      {state.status === "error" && (
        <div className="flex flex-1 items-center justify-center">
          <p className="text-sm text-destructive">{state.message}</p>
        </div>
      )}

      {state.status === "ready" && editable && (
        <Tabs defaultValue="edit" className="min-h-0 flex-1 flex-col gap-0">
          <TabsList variant="line" className="mx-2 mt-1 h-7 w-fit self-start">
            <TabsTrigger value="edit">Edit</TabsTrigger>
            <TabsTrigger value="preview">Preview</TabsTrigger>
          </TabsList>
          <TabsContent
            value="edit"
            forceMount
            className="min-h-0 flex-1 overflow-hidden data-[state=inactive]:hidden"
          >
            <MarkdownEditor
              key={path}
              initialValue={state.savedContent}
              onChange={setDraft}
              onSave={() => void save()}
            />
          </TabsContent>
          <TabsContent
            value="preview"
            forceMount
            className="min-h-0 flex-1 overflow-auto data-[state=inactive]:hidden"
          >
            <MarkdownPreview content={draft} />
          </TabsContent>
        </Tabs>
      )}

      {state.status === "ready" && !editable && (
        <ScrollArea className="min-h-0 flex-1">
          <pre className="whitespace-pre-wrap p-3 font-mono text-xs text-foreground/90">
            {state.savedContent}
          </pre>
        </ScrollArea>
      )}
    </div>
  );
}

export function FilesTab({ projectPath }: FilesTabProps) {
  const [tree, setTree] = useState<TreeState>({ status: "empty" });
  const [selectedPath, setSelectedPath] = useState<string | null>(null);

  useEffect(() => {
    setSelectedPath(null);

    if (!projectPath) {
      setTree({ status: "empty" });
      return;
    }

    let cancelled = false;
    setTree({ status: "loading" });

    async function load() {
      try {
        const entries = await invoke<FileEntry[]>("list_directory", { path: projectPath });
        if (!cancelled) setTree({ status: "ready", entries });
      } catch (err) {
        if (!cancelled) {
          setTree({
            status: "error",
            message: err instanceof Error ? err.message : String(err),
          });
        }
      }
    }

    void load();
    return () => {
      cancelled = true;
    };
  }, [projectPath]);

  return (
    <div className="flex min-h-0 flex-1">
      <div className="flex h-full w-[220px] shrink-0 flex-col border-r border-border">
        <ScrollArea className="min-h-0 flex-1">
          <div className="py-1">
            {tree.status === "empty" && (
              <p className="px-3 py-2 text-sm text-muted-foreground">No project open.</p>
            )}
            {tree.status === "loading" && (
              <p className="px-3 py-2 text-sm text-muted-foreground">loading…</p>
            )}
            {tree.status === "error" && (
              <p className="px-3 py-2 text-sm text-destructive">{tree.message}</p>
            )}
            {tree.status === "ready" && (
              <FileTree
                entries={tree.entries}
                selectedPath={selectedPath}
                onSelectFile={(entry) => setSelectedPath(entry.path)}
              />
            )}
          </div>
        </ScrollArea>
      </div>
      {selectedPath ? (
        <FileInspector key={selectedPath} path={selectedPath} />
      ) : (
        <div className="flex flex-1 flex-col items-center justify-center gap-2 text-center">
          <div className="flex size-10 items-center justify-center border border-border">
            <FolderOpen className="size-5 text-muted-foreground" />
          </div>
          <p className="max-w-xs text-sm text-muted-foreground">Select a file to inspect.</p>
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cd /Users/raoulkaleba/Developer/InfinaBox && npx tsc --noEmit`
Expected: no new errors attributable to this file.

- [ ] **Step 3: Commit**

```bash
cd /Users/raoulkaleba/Developer/InfinaBox
git add src/components/cockpit/FilesTab.tsx
git commit -m "Add FilesTab: file tree + inspector (edit for .md, read-only otherwise)"
```

---

## Task 8: `ProjectWindow` tab shell

**Files:**
- Create: `src/components/cockpit/ProjectWindow.tsx`

**Interfaces:**
- Consumes: `OverviewTab` (Task 5), `ChangesTab` (Task 6), `FilesTab` (Task 7); shadcn `Tabs`/`TabsList`/`TabsTrigger`/`TabsContent` (`@/components/ui/tabs`, pre-existing).
- Produces: `export function ProjectWindow({ projectPath }: { projectPath: string | null })` — consumed by Task 9.

- [ ] **Step 1: Write the component**

```tsx
import { useState } from "react";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { OverviewTab } from "@/components/cockpit/OverviewTab";
import { ChangesTab } from "@/components/cockpit/ChangesTab";
import { FilesTab } from "@/components/cockpit/FilesTab";

interface ProjectWindowProps {
  projectPath: string | null;
}

export function ProjectWindow({ projectPath }: ProjectWindowProps) {
  const [tab, setTab] = useState("overview");

  return (
    <div className="flex h-full min-w-0 flex-1 flex-col border border-border bg-card">
      <Tabs
        value={tab}
        onValueChange={setTab}
        className="flex min-h-0 flex-1 flex-col gap-0"
      >
        <TabsList
          variant="line"
          className="h-8 w-full shrink-0 justify-start rounded-none border-b border-border px-2"
        >
          <TabsTrigger value="overview">Overview</TabsTrigger>
          <TabsTrigger value="changes">Changes</TabsTrigger>
          <TabsTrigger value="files">Files</TabsTrigger>
        </TabsList>
        <TabsContent value="overview" className="flex min-h-0 flex-1 flex-col">
          <OverviewTab projectPath={projectPath} />
        </TabsContent>
        <TabsContent value="changes" className="flex min-h-0 flex-1 flex-col">
          <ChangesTab projectPath={projectPath} />
        </TabsContent>
        <TabsContent value="files" className="flex min-h-0 flex-1">
          <FilesTab projectPath={projectPath} />
        </TabsContent>
      </Tabs>
    </div>
  );
}
```

Note: unlike `FilesTab`'s internal Edit/Preview toggle (Task 7), these three top-level tabs deliberately do NOT use `forceMount` — each tab's `useEffect` re-fetches on mount, so letting Radix's default behavior unmount inactive tabs is correct here (only the active tab's data loads) rather than a bug to fix.

- [ ] **Step 2: Verify it compiles**

Run: `cd /Users/raoulkaleba/Developer/InfinaBox && npx tsc --noEmit`
Expected: no new errors attributable to this file.

- [ ] **Step 3: Commit**

```bash
cd /Users/raoulkaleba/Developer/InfinaBox
git add src/components/cockpit/ProjectWindow.tsx
git commit -m "Add ProjectWindow: tab shell for Overview/Changes/Files"
```

---

## Task 9: Wire `ProjectWindow` into `App`, simplify `TopBar`

**Files:**
- Modify: `src/App.tsx` (replace the old panel composition)
- Modify: `src/components/cockpit/TopBar.tsx` (remove the Project-menu dropdown)

**Interfaces:**
- Consumes: `ProjectWindow` (Task 8), `TerminalPanel` (pre-existing, unchanged), simplified `TopBar` props.
- Produces: the app's real, final layout for this redesign.

- [ ] **Step 1: Rewrite `TopBar.tsx`**

Replace the entire contents of `src/components/cockpit/TopBar.tsx` with:

```tsx
import { Plus, Box, FolderOpen } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";

interface TopBarProps {
  projectPath: string | null;
  onOpenProject: (path: string) => void;
}

function folderName(path: string): string {
  return path.split("/").filter(Boolean).pop() ?? path;
}

export function TopBar({ projectPath, onOpenProject }: TopBarProps) {
  async function handleOpenProject() {
    // `open` resolves to `null` when the user cancels the dialog — that's
    // a normal outcome, not an error, so there's nothing to catch/report.
    const folder = await open({ directory: true, multiple: false });
    if (typeof folder === "string") {
      onOpenProject(folder);
    }
  }

  return (
    <header className="flex h-12 shrink-0 items-center justify-between border border-border bg-card px-3">
      <div className="flex items-center gap-3">
        <div className="flex size-6 items-center justify-center border border-border bg-accent">
          <Box className="size-3.5" />
        </div>
        <span className="text-sm font-medium tracking-tight">InfinaBox</span>
        <Separator orientation="vertical" className="h-4" />
        <Button
          size="sm"
          variant="outline"
          onClick={() => void handleOpenProject()}
          title={projectPath ?? undefined}
        >
          <FolderOpen />
          {projectPath ? folderName(projectPath) : "Open Project"}
        </Button>
      </div>
      <div className="flex items-center gap-2">
        <Button size="sm">
          <Plus />
          New task
        </Button>
      </div>
    </header>
  );
}
```

(This removes the `Section` type export, the `SECTIONS` array, the `DropdownMenu`, and the `activeSection`/`onSelectSection` props — nothing else in the codebase imports `Section` from this file after Task 10 deletes the section components, so this is safe.)

- [ ] **Step 2: Rewrite `App.tsx`**

Replace the entire contents of `src/App.tsx` with:

```tsx
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { TopBar } from "@/components/cockpit/TopBar";
import { ProjectWindow } from "@/components/cockpit/ProjectWindow";
import { TerminalPanel } from "@/components/cockpit/TerminalPanel";

function App() {
  // The currently open project folder — shared by ProjectWindow (all three
  // of its tabs) and the terminal's starting directory. Seeded from
  // get_default_project_path on mount so the app still opens showing the
  // familiar test project by default; from then on it's real, user-driven
  // state set via the TopBar's "Open Project" picker.
  const [projectPath, setProjectPath] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    async function loadDefaultProject() {
      const defaultPath = await invoke<string>("get_default_project_path");
      if (!cancelled) {
        setProjectPath(defaultPath);
      }
    }

    void loadDefaultProject();

    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <div className="flex h-screen w-screen flex-col gap-2 bg-background p-2 text-foreground">
      <TopBar projectPath={projectPath} onOpenProject={setProjectPath} />
      <div className="flex min-h-0 flex-1 gap-2">
        <ProjectWindow projectPath={projectPath} />
        <TerminalPanel projectPath={projectPath} />
      </div>
    </div>
  );
}

export default App;
```

- [ ] **Step 3: Verify it compiles**

Run: `cd /Users/raoulkaleba/Developer/InfinaBox && npx tsc --noEmit`
Expected: this WILL still show errors at this point, because `App.tsx` no longer imports the now-orphaned section/console/old-panel components — but those files themselves still exist and are unused, which `tsc` does not error on by default (unused *files* aren't a compile error, only unused *local variables/imports within a file that's still referenced* would be — and nothing references them anymore). If you see errors, they should only be inside `SectionOverlay.tsx`, `DesignSection.tsx`, `QaSection.tsx`, `BusinessSection.tsx`, `LiveOpsSection.tsx`, `FileBrowserPanel.tsx`, or `ViewportPanel.tsx` themselves (e.g. one of them importing the now-changed `Section` type) — that's expected and fixed by Task 10 deleting them. Do not fix those files; delete them in the next task.

- [ ] **Step 4: Commit**

```bash
cd /Users/raoulkaleba/Developer/InfinaBox
git add src/App.tsx src/components/cockpit/TopBar.tsx
git commit -m "Wire ProjectWindow into App, simplify TopBar (no more section dropdown)"
```

---

## Task 10: Delete obsolete components, final verification

**Files:**
- Delete: `src/components/cockpit/ConsoleDock.tsx`
- Delete: `src/components/cockpit/SectionOverlay.tsx`
- Delete: `src/components/cockpit/DesignSection.tsx`
- Delete: `src/components/cockpit/QaSection.tsx`
- Delete: `src/components/cockpit/BusinessSection.tsx`
- Delete: `src/components/cockpit/LiveOpsSection.tsx`
- Delete: `src/components/cockpit/FileBrowserPanel.tsx`
- Delete: `src/components/cockpit/ViewportPanel.tsx`

**Interfaces:** none — this task only removes files nothing references anymore after Task 9.

- [ ] **Step 1: Confirm nothing still imports these files**

Run: `cd /Users/raoulkaleba/Developer/InfinaBox && grep -rn "ConsoleDock\|SectionOverlay\|DesignSection\|QaSection\|BusinessSection\|LiveOpsSection\|FileBrowserPanel\|ViewportPanel" src/App.tsx src/components/cockpit/TopBar.tsx src/components/cockpit/ProjectWindow.tsx src/components/cockpit/OverviewTab.tsx src/components/cockpit/ChangesTab.tsx src/components/cockpit/FilesTab.tsx`
Expected: no output (no matches). If there IS a match, stop and fix that reference before deleting anything — do not delete a file something still imports.

- [ ] **Step 2: Delete the files**

```bash
cd /Users/raoulkaleba/Developer/InfinaBox
rm src/components/cockpit/ConsoleDock.tsx
rm src/components/cockpit/SectionOverlay.tsx
rm src/components/cockpit/DesignSection.tsx
rm src/components/cockpit/QaSection.tsx
rm src/components/cockpit/BusinessSection.tsx
rm src/components/cockpit/LiveOpsSection.tsx
rm src/components/cockpit/FileBrowserPanel.tsx
rm src/components/cockpit/ViewportPanel.tsx
```

- [ ] **Step 3: Full frontend build**

Run: `cd /Users/raoulkaleba/Developer/InfinaBox && npm run build`
Expected: `tsc && vite build` completes with zero type errors.

- [ ] **Step 4: Full backend build and test suite**

Run: `cd /Users/raoulkaleba/Developer/InfinaBox && cargo build --workspace && cargo test --workspace`
Expected: builds clean, all tests pass — this includes every pre-existing test (Phase 0's graph/git-indexer/ask/mcp tests, Phase 1's fs/project/terminal tests) plus Tasks 1/2/3's new ones. This task doesn't add new Rust tests itself, but re-running the whole suite here is the final confirmation that deleting frontend files didn't somehow break the backend (it shouldn't — these are disjoint layers — but this is the last checkpoint before calling the redesign done).

- [ ] **Step 5: Visual sanity check (best-effort, per this project's established limitation)**

This environment cannot get a live screenshot of the actual Tauri window (no interactive display session to approve macOS's permission dialogs — confirmed repeatedly across this project's history). As a substitute, start the Vite dev server and check the page renders without crashing:

Run: `cd /Users/raoulkaleba/Developer/InfinaBox && npm run dev &` (or use this session's `mcp__Claude_Browser__preview_start` tool with the `vite-dev` config in `.claude/launch.json`, if available), then navigate a browser to `http://localhost:1420` and take a screenshot.
Expected: the top bar (logo, "Open Project" button, "New task" button — no Project dropdown), a `ProjectWindow` with three tabs (Overview/Changes/Files) on the left half, and the terminal on the right half, splitting evenly. `@tauri-apps/api`'s `invoke`/`listen` will throw outside a real Tauri webview (`Cannot read properties of undefined (reading 'invoke')`) — this is expected and does not indicate a bug; it means the layout/CSS is confirmed but the real data path (which is already covered by Rust tests) can't be exercised this way. Stop the dev server afterward.

- [ ] **Step 6: Commit**

```bash
cd /Users/raoulkaleba/Developer/InfinaBox
git add -A
git commit -m "Remove obsolete cockpit components superseded by ProjectWindow

Deletes ConsoleDock, SectionOverlay, DesignSection, QaSection,
BusinessSection, LiveOpsSection, FileBrowserPanel, and ViewportPanel —
all fully superseded by ProjectWindow's Overview/Changes/Files tabs.
This completes the workspace redesign described in
docs/superpowers/specs/2026-09-12-workspace-redesign-design.md."
```
