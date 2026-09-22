// Shared classification helpers for error messages surfaced by the Tauri
// backend's Git commands (`current_branch`, `list_recent_commits`, defined in
// src-tauri/src/commands/overview.rs, delegating to infinabox_core::git_indexer).
// Both OverviewTab and ChangesTab need to tell "not a Git repo at all" apart
// from "a real Git repo with zero commits yet" instead of collapsing both
// into the same (sometimes false) message.

/** True when `message` is the error libgit2 produces when the path isn't a Git repository at all. */
export function isMissingGitRepoError(message: string): boolean {
  return message.toLowerCase().includes("failed to open git repository");
}

/**
 * True when `message` is one of the errors libgit2 produces for a real Git
 * repository that has no commits yet (HEAD is unborn) — from either
 * `walk_commits` ("is this an empty repo, or is HEAD unborn") or
 * `current_branch` ("failed to read repository HEAD").
 */
export function isEmptyRepoError(message: string): boolean {
  const lower = message.toLowerCase();
  return (
    lower.includes("is this an empty repo, or is head unborn") ||
    lower.includes("failed to read repository head")
  );
}

/**
 * True when `message` is the error `list_directory` produces (see
 * src-tauri/src/commands/fs.rs) for a path that doesn't exist yet — the
 * normal first-run state for a folder like `.ibproject/docs` that a
 * project hasn't adopted yet, not a real error worth alarming over.
 */
export function isMissingDirectoryError(message: string): boolean {
  return message.toLowerCase().includes("is not a directory");
}
