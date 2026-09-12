//! Builds the graph from a repository's Git history, oldest commit first, so
//! that file identity can be threaded correctly through renames as they
//! happen — a rename in commit N only makes sense if we've already seen the
//! file's earlier life in commits before N.

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::json;
use uuid::Uuid;

use crate::git_indexer::{self, FileChange};

/// Re-builds the graph at `db_path` from the full commit history of the
/// repository at `repo_path`. Safe to re-run: existing nodes/edges for a
/// commit that's already indexed are left as-is (matched by commit sha).
pub fn build_graph(repo_path: &str, db_path: &str) -> Result<IngestReport> {
    let conn = super::schema::open_and_init(db_path)?;

    let mut commits = git_indexer::walk_commits(repo_path)?;
    // walk_commits returns newest-first (matching `git log`); we need
    // oldest-first so rename chains resolve in the order they really happened.
    commits.reverse();

    let mut report = IngestReport::default();

    for commit in &commits {
        let commit_node_id = format!("commit:{}", commit.sha);

        let already_indexed: Option<String> = conn
            .query_row(
                "SELECT id FROM nodes WHERE id = ?1",
                params![commit_node_id],
                |row| row.get(0),
            )
            .optional()
            .context("failed to check for existing commit node")?;

        if already_indexed.is_some() {
            report.commits_skipped += 1;
            continue;
        }

        let now = Utc::now().to_rfc3339();
        let content = json!({
            "sha": commit.sha,
            "short_sha": commit.short_sha,
            "author_name": commit.author_name,
            "author_email": commit.author_email,
            "summary": commit.summary,
            "message": commit.message,
            "timestamp": commit.timestamp,
        });

        conn.execute(
            "INSERT INTO nodes (id, type, label, content, created_at, updated_at)
             VALUES (?1, 'commit', ?2, ?3, ?4, ?4)",
            params![commit_node_id, commit.summary, content.to_string(), now],
        )
        .context("failed to insert commit node")?;
        report.commits_indexed += 1;

        for change in &commit.files_changed {
            apply_file_change(&conn, &commit_node_id, &commit.timestamp, change, &mut report)?;
        }
    }

    Ok(report)
}

#[derive(Default, Debug)]
pub struct IngestReport {
    pub commits_indexed: usize,
    pub commits_skipped: usize,
    pub file_identities_created: usize,
    pub edges_created: usize,
}

fn apply_file_change(
    conn: &Connection,
    commit_node_id: &str,
    commit_timestamp: &str,
    change: &FileChange,
    report: &mut IngestReport,
) -> Result<()> {
    match change {
        FileChange::Added { path } | FileChange::Modified { path } => {
            let file_node_id = resolve_or_create_file_node(conn, path, report)?;
            let relation = if matches!(change, FileChange::Added { .. }) {
                "adds"
            } else {
                "modifies"
            };
            insert_edge(
                conn,
                commit_node_id,
                &file_node_id,
                relation,
                &json!({ "path": path }),
                commit_timestamp,
            )?;
            report.edges_created += 1;
        }
        FileChange::Deleted { path } => {
            // The identity must already exist to be deleted; if our history
            // is somehow incomplete, create it defensively rather than
            // silently dropping the event.
            let file_node_id = resolve_or_create_file_node(conn, path, report)?;
            insert_edge(
                conn,
                commit_node_id,
                &file_node_id,
                "deletes",
                &json!({ "path": path }),
                commit_timestamp,
            )?;
            report.edges_created += 1;
        }
        FileChange::Renamed { from, to } => {
            let file_node_id = resolve_or_create_file_node(conn, from, report)?;

            // `to` now also resolves to the same identity. `from` keeps
            // resolving to it too — that's the whole point: an old, dead
            // path still answers "what commits touched this file" correctly.
            conn.execute(
                "INSERT OR IGNORE INTO path_index (path, node_id) VALUES (?1, ?2)",
                params![to, file_node_id],
            )
            .context("failed to add renamed-to path to path_index")?;

            update_current_path(conn, &file_node_id, from, to)?;

            insert_edge(
                conn,
                commit_node_id,
                &file_node_id,
                "renames",
                &json!({ "from": from, "to": to }),
                commit_timestamp,
            )?;
            report.edges_created += 1;
        }
        FileChange::Copied { from, to } => {
            // A copy starts a new identity that's derived from the source,
            // rather than continuing the source's own identity.
            let source_node_id = resolve_or_create_file_node(conn, from, report)?;
            let new_node_id = resolve_or_create_file_node(conn, to, report)?;
            insert_edge(
                conn,
                commit_node_id,
                &new_node_id,
                "adds",
                &json!({ "path": to }),
                commit_timestamp,
            )?;
            insert_edge(
                conn,
                &new_node_id,
                &source_node_id,
                "derived_from",
                &json!({ "from": from, "to": to }),
                commit_timestamp,
            )?;
            report.edges_created += 2;
        }
        FileChange::Typechange { path } | FileChange::Other { path, .. } => {
            let file_node_id = resolve_or_create_file_node(conn, path, report)?;
            insert_edge(
                conn,
                commit_node_id,
                &file_node_id,
                "modifies",
                &json!({ "path": path }),
                commit_timestamp,
            )?;
            report.edges_created += 1;
        }
    }
    Ok(())
}

/// Looks up the file identity for `path` in `path_index`; creates a new one
/// if this is the first time we've ever seen this path.
fn resolve_or_create_file_node(
    conn: &Connection,
    path: &str,
    report: &mut IngestReport,
) -> Result<String> {
    let existing: Option<String> = conn
        .query_row(
            "SELECT node_id FROM path_index WHERE path = ?1",
            params![path],
            |row| row.get(0),
        )
        .optional()
        .context("failed to look up path_index")?;

    if let Some(node_id) = existing {
        return Ok(node_id);
    }

    let node_id = format!("file:{}", Uuid::new_v4());
    let now = Utc::now().to_rfc3339();
    let content = json!({
        "current_path": path,
        "path_history": [],
    });

    conn.execute(
        "INSERT INTO nodes (id, type, label, content, created_at, updated_at)
         VALUES (?1, 'file', ?2, ?3, ?4, ?4)",
        params![node_id, path, content.to_string(), now],
    )
    .context("failed to insert file node")?;

    conn.execute(
        "INSERT INTO path_index (path, node_id) VALUES (?1, ?2)",
        params![path, node_id],
    )
    .context("failed to insert into path_index")?;

    report.file_identities_created += 1;
    Ok(node_id)
}

/// Updates a file node's `current_path` and appends the superseded path to
/// its `path_history`, after a rename has been recorded.
fn update_current_path(conn: &Connection, node_id: &str, old_path: &str, new_path: &str) -> Result<()> {
    let content_str: String = conn
        .query_row(
            "SELECT content FROM nodes WHERE id = ?1",
            params![node_id],
            |row| row.get(0),
        )
        .context("failed to read file node content before rename update")?;

    let mut content: serde_json::Value =
        serde_json::from_str(&content_str).context("failed to parse file node content as JSON")?;

    if let Some(history) = content.get_mut("path_history").and_then(|v| v.as_array_mut()) {
        history.push(json!(old_path));
    }
    content["current_path"] = json!(new_path);

    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE nodes SET content = ?1, label = ?2, updated_at = ?3 WHERE id = ?4",
        params![content.to_string(), new_path, now, node_id],
    )
    .context("failed to persist renamed file node")?;

    Ok(())
}

fn insert_edge(
    conn: &Connection,
    from_node: &str,
    to_node: &str,
    relation: &str,
    evidence: &serde_json::Value,
    created_at: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO edges (from_node, to_node, relation, confidence, evidence, created_at)
         VALUES (?1, ?2, ?3, 1.0, ?4, ?5)",
        params![from_node, to_node, relation, evidence.to_string(), created_at],
    )
    .context("failed to insert edge")?;
    Ok(())
}
