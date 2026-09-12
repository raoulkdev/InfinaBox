//! The two queries Milestone 4's "done when" bar is measured against:
//! "what files changed in commit X" and "what commits touched file Y" —
//! and the latter must still work when Y is a path that was renamed away
//! from long ago.

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::Serialize;

#[derive(Serialize, Debug)]
pub struct FileTouched {
    pub relation: String,
    pub evidence: serde_json::Value,
}

/// What changed in a given commit. `sha_or_prefix` may be the full sha or
/// any unambiguous leading prefix (like `git show <short-sha>`).
pub fn files_changed_in_commit(conn: &Connection, sha_or_prefix: &str) -> Result<Vec<FileTouched>> {
    let commit_node_id = resolve_commit_node(conn, sha_or_prefix)?;

    let mut stmt = conn
        .prepare(
            "SELECT relation, evidence FROM edges WHERE from_node = ?1 ORDER BY id",
        )
        .context("failed to prepare files-changed query")?;

    let rows = stmt
        .query_map(params![commit_node_id], |row| {
            let relation: String = row.get(0)?;
            let evidence_str: String = row.get(1)?;
            Ok((relation, evidence_str))
        })
        .context("failed to run files-changed query")?;

    let mut results = Vec::new();
    for row in rows {
        let (relation, evidence_str) = row.context("failed to read files-changed row")?;
        let evidence: serde_json::Value =
            serde_json::from_str(&evidence_str).context("failed to parse edge evidence as JSON")?;
        results.push(FileTouched { relation, evidence });
    }
    Ok(results)
}

#[derive(Serialize, Debug)]
pub struct CommitTouching {
    pub sha: String,
    pub short_sha: String,
    pub summary: String,
    pub timestamp: String,
    pub relation: String,
    pub evidence: serde_json::Value,
}

/// Every commit that ever touched `path` — resolved through the file's full
/// identity, so an old (superseded) path returns exactly the same history as
/// querying by the file's current path.
pub fn commits_touching_file(conn: &Connection, path: &str) -> Result<Vec<CommitTouching>> {
    let node_id: Option<String> = conn
        .query_row(
            "SELECT node_id FROM path_index WHERE path = ?1",
            params![path],
            |row| row.get(0),
        )
        .optional_or_none()?;

    let Some(node_id) = node_id else {
        return Ok(Vec::new());
    };

    let mut stmt = conn
        .prepare(
            "SELECT n.content, e.relation, e.evidence
             FROM edges e
             JOIN nodes n ON n.id = e.from_node
             WHERE e.to_node = ?1 AND n.type = 'commit'
             ORDER BY n.content ->> '$.timestamp' DESC, e.id DESC",
        )
        .context("failed to prepare commits-touching-file query")?;

    let rows = stmt
        .query_map(params![node_id], |row| {
            let content_str: String = row.get(0)?;
            let relation: String = row.get(1)?;
            let evidence_str: String = row.get(2)?;
            Ok((content_str, relation, evidence_str))
        })
        .context("failed to run commits-touching-file query")?;

    let mut results = Vec::new();
    for row in rows {
        let (content_str, relation, evidence_str) = row.context("failed to read commit-touching row")?;
        let content: serde_json::Value =
            serde_json::from_str(&content_str).context("failed to parse commit node content")?;
        let evidence: serde_json::Value =
            serde_json::from_str(&evidence_str).context("failed to parse edge evidence")?;

        results.push(CommitTouching {
            sha: content["sha"].as_str().unwrap_or_default().to_string(),
            short_sha: content["short_sha"].as_str().unwrap_or_default().to_string(),
            summary: content["summary"].as_str().unwrap_or_default().to_string(),
            timestamp: content["timestamp"].as_str().unwrap_or_default().to_string(),
            relation,
            evidence,
        });
    }
    Ok(results)
}

fn resolve_commit_node(conn: &Connection, sha_or_prefix: &str) -> Result<String> {
    let exact_id = format!("commit:{sha_or_prefix}");
    let exact: Option<String> = conn
        .query_row(
            "SELECT id FROM nodes WHERE id = ?1",
            params![exact_id],
            |row| row.get(0),
        )
        .optional_or_none()?;
    if let Some(id) = exact {
        return Ok(id);
    }

    let prefix_pattern = format!("commit:{sha_or_prefix}%");
    let prefixed: Option<String> = conn
        .query_row(
            "SELECT id FROM nodes WHERE type = 'commit' AND id LIKE ?1 LIMIT 1",
            params![prefix_pattern],
            |row| row.get(0),
        )
        .optional_or_none()?;

    prefixed.with_context(|| format!("no commit found matching '{sha_or_prefix}'"))
}

/// Small helper trait so `.optional_or_none()?` reads cleanly above instead
/// of the more verbose `rusqlite::OptionalExtension` + match boilerplate at
/// every call site.
trait OptionalOrNone<T> {
    fn optional_or_none(self) -> Result<Option<T>>;
}

impl<T> OptionalOrNone<T> for rusqlite::Result<T> {
    fn optional_or_none(self) -> Result<Option<T>> {
        use rusqlite::OptionalExtension;
        self.optional().context("query failed")
    }
}
