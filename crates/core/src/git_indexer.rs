//! Milestone 3 (Phase 0): turns a project's Git history into structured data —
//! the backbone of InfinaBox's "this bug was caused by this commit" traced chain.

use anyhow::{Context, Result};
use chrono::{FixedOffset, TimeZone};
use git2::{Delta, DiffFindOptions, DiffOptions, Repository, Sort};
use serde::Serialize;

/// A single changed-file entry within a commit's diff against its (first) parent.
#[derive(Serialize, Clone, Debug)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum FileChange {
    Added { path: String },
    Deleted { path: String },
    Modified { path: String },
    Renamed { from: String, to: String },
    Copied { from: String, to: String },
    Typechange { path: String },
    /// Catch-all for any other libgit2 delta status we don't special-case above
    /// (e.g. Unreadable, Conflicted) — kept so the walk never silently drops a file.
    Other { path: String, kind: String },
}

#[derive(Serialize, Clone, Debug)]
pub struct CommitInfo {
    pub sha: String,
    pub short_sha: String,
    pub author_name: String,
    pub author_email: String,
    /// First line of the commit message.
    pub summary: String,
    /// Full commit message, including body.
    pub message: String,
    /// RFC3339 timestamp of the commit, using the commit's own timezone offset.
    pub timestamp: String,
    pub files_changed: Vec<FileChange>,
}

/// Walks the full commit history reachable from HEAD (newest first, matching
/// `git log`'s default order) and prints it as a JSON array to stdout.
pub fn run(path: &str) -> Result<()> {
    let commits = walk_commits(path)?;
    let json = serde_json::to_string_pretty(&commits)
        .context("failed to serialize commit history to JSON")?;
    println!("{json}");
    Ok(())
}

/// Walks the full commit history reachable from HEAD (newest first) and
/// returns it as structured data — reusable by the graph builder (milestone 4)
/// as well as the CLI's own JSON-printing `run()` above.
pub fn walk_commits(path: &str) -> Result<Vec<CommitInfo>> {
    let repo = Repository::open(path)
        .with_context(|| format!("failed to open Git repository at '{path}'"))?;

    let mut revwalk = repo.revwalk().context("failed to create revwalk")?;
    revwalk.push_head().context(
        "failed to start walk from HEAD (is this an empty repo, or is HEAD unborn?)",
    )?;
    // Match `git log`'s default order: newest first, with topological sort as
    // a tie-breaker for commits sharing the same timestamp (common in quickly
    // scripted test histories, where commit time alone can't disambiguate).
    revwalk
        .set_sorting(Sort::TIME | Sort::TOPOLOGICAL)
        .context("failed to set revwalk sort order")?;

    let mut commits = Vec::new();

    for oid in revwalk {
        let oid = oid.context("failed to read commit id while walking history")?;
        let commit = repo
            .find_commit(oid)
            .with_context(|| format!("failed to look up commit {oid}"))?;

        let sha = commit.id().to_string();
        let short_sha = commit
            .as_object()
            .short_id()
            .ok()
            .and_then(|buf| buf.as_str().ok().map(str::to_string))
            .unwrap_or_else(|| sha[..sha.len().min(7)].to_string());

        let author = commit.author();
        let author_name = author.name().unwrap_or("<invalid utf-8>").to_string();
        let author_email = author.email().unwrap_or("<invalid utf-8>").to_string();

        let summary = commit
            .summary()
            .ok()
            .flatten()
            .unwrap_or("<invalid utf-8>")
            .to_string();
        let message = commit.message().unwrap_or("<invalid utf-8>").to_string();

        let git_time = commit.time();
        let offset = FixedOffset::east_opt(git_time.offset_minutes() * 60)
            .unwrap_or_else(|| FixedOffset::east_opt(0).unwrap());
        let timestamp = offset
            .timestamp_opt(git_time.seconds(), 0)
            .single()
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_else(|| commit.time().seconds().to_string());

        let files_changed = diff_files_for_commit(&repo, &commit)
            .with_context(|| format!("failed to diff commit {sha}"))?;

        commits.push(CommitInfo {
            sha,
            short_sha,
            author_name,
            author_email,
            summary,
            message,
            timestamp,
            files_changed,
        });
    }

    Ok(commits)
}

/// Diffs `commit` against its first parent (or against an empty tree, for a
/// root commit) and returns the list of changed files, with rename detection
/// enabled so moved/renamed files are reported as such instead of a
/// delete+add pair.
fn diff_files_for_commit(
    repo: &Repository,
    commit: &git2::Commit,
) -> Result<Vec<FileChange>> {
    let new_tree = commit.tree().context("failed to get commit tree")?;

    let old_tree = if commit.parent_count() > 0 {
        let parent = commit.parent(0).context("failed to get parent commit")?;
        Some(parent.tree().context("failed to get parent tree")?)
    } else {
        None
    };

    let mut diff_opts = DiffOptions::new();
    diff_opts.include_typechange(true);

    let mut diff = repo
        .diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), Some(&mut diff_opts))
        .context("failed to compute tree-to-tree diff")?;

    let mut find_opts = DiffFindOptions::new();
    find_opts.renames(true);
    find_opts.renames_from_rewrites(true);
    diff.find_similar(Some(&mut find_opts))
        .context("failed to run rename detection on diff")?;

    let mut files = Vec::new();
    for delta in diff.deltas() {
        let old_path = delta
            .old_file()
            .path()
            .map(|p| p.to_string_lossy().into_owned());
        let new_path = delta
            .new_file()
            .path()
            .map(|p| p.to_string_lossy().into_owned());

        let change = match delta.status() {
            Delta::Added => FileChange::Added {
                path: new_path.or(old_path).unwrap_or_default(),
            },
            Delta::Deleted => FileChange::Deleted {
                path: old_path.or(new_path).unwrap_or_default(),
            },
            Delta::Modified => FileChange::Modified {
                path: new_path.or(old_path).unwrap_or_default(),
            },
            Delta::Renamed => FileChange::Renamed {
                from: old_path.unwrap_or_default(),
                to: new_path.unwrap_or_default(),
            },
            Delta::Copied => FileChange::Copied {
                from: old_path.unwrap_or_default(),
                to: new_path.unwrap_or_default(),
            },
            Delta::Typechange => FileChange::Typechange {
                path: new_path.or(old_path).unwrap_or_default(),
            },
            other => FileChange::Other {
                path: new_path.or(old_path).unwrap_or_default(),
                kind: format!("{other:?}"),
            },
        };
        files.push(change);
    }

    Ok(files)
}
