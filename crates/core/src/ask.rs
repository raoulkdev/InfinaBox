//! Milestone 5 (Phase 0): prove one agent can answer a real question using
//! only the local graph — grounded, citing its source, refusing to guess.
//!
//! This is deliberately a *deterministic* lookup agent, not an LLM call: the
//! risky, unproven part of the product's bet is whether the retrieval and
//! citation loop is correct, not whether a frontier model can phrase a
//! sentence. A rule-based agent that is *wrong zero times by construction*
//! (it only ever repeats facts it read out of the graph) proves that loop
//! more convincingly than a fluent LLM answer would, and costs nothing to
//! run. Swapping this for a real model later is a small, low-risk change:
//! once a key is configured (e.g. `ANTHROPIC_API_KEY`), the retrieved
//! `evidence` collected below is exactly the grounded context you'd hand to
//! the model, with instructions to answer only from it and cite the
//! sha/path — the "cite or stay quiet" agent principle from the product plan.

use anyhow::{Context, Result};
use rusqlite::Connection;

use crate::graph::query::{commits_touching_file, files_changed_in_commit};

pub fn answer(conn: &Connection, question: &str) -> Result<String> {
    let question_lower = question.to_lowercase();

    if let Some(sha) = find_matching_commit_sha(conn, &question_lower)? {
        let touched = files_changed_in_commit(conn, &sha)
            .with_context(|| format!("failed to look up files changed in commit {sha}"))?;

        if touched.is_empty() {
            return Ok(format!(
                "I found commit {sha} in the project map, but it has no recorded file changes — I won't guess beyond that.",
            ));
        }

        let mut lines = vec![format!(
            "Commit {sha} touched {} file(s), according to the project map:",
            touched.len()
        )];
        for t in &touched {
            lines.push(format!("  - [{}] {}", t.relation, t.evidence));
        }
        return Ok(lines.join("\n"));
    }

    if let Some(path) = find_matching_path(conn, &question_lower)? {
        let commits = commits_touching_file(conn, &path)
            .with_context(|| format!("failed to look up commits touching {path}"))?;

        let Some(most_recent) = commits.first() else {
            return Ok(format!(
                "'{path}' exists in the project map, but no commit is recorded against it — I won't guess why."
            ));
        };

        return Ok(format!(
            "The most recent commit touching '{path}' is {} (\"{}\"), recorded as a '{}' change: {}. \
             {} total commit(s) in this file's history are in the project map.",
            most_recent.short_sha,
            most_recent.summary,
            most_recent.relation,
            most_recent.evidence,
            commits.len(),
        ));
    }

    Ok("Nothing in the project map matches this question closely enough to answer without guessing \
        — try naming a specific file path or commit sha."
        .to_string())
}

fn find_matching_commit_sha(conn: &Connection, question_lower: &str) -> Result<Option<String>> {
    let mut stmt = conn.prepare("SELECT content FROM nodes WHERE type = 'commit'")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;

    for row in rows {
        let content_str = row?;
        let content: serde_json::Value = serde_json::from_str(&content_str)?;
        if let Some(short_sha) = content["short_sha"].as_str() {
            if !short_sha.is_empty() && question_lower.contains(&short_sha.to_lowercase()) {
                return Ok(Some(short_sha.to_string()));
            }
        }
        if let Some(sha) = content["sha"].as_str() {
            if question_lower.contains(&sha.to_lowercase()) {
                return Ok(Some(sha.to_string()));
            }
        }
    }
    Ok(None)
}

fn find_matching_path(conn: &Connection, question_lower: &str) -> Result<Option<String>> {
    let mut stmt = conn.prepare("SELECT path FROM path_index")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;

    // Prefer the longest matching path (so "ElevatorLogic.gd" beats a
    // coincidental shorter match) among all paths mentioned in the question.
    let mut best: Option<String> = None;
    for row in rows {
        let path = row?;
        let file_name = path.rsplit('/').next().unwrap_or(&path);
        if question_lower.contains(&file_name.to_lowercase()) || question_lower.contains(&path.to_lowercase()) {
            if best.as_ref().map(|b| path.len() > b.len()).unwrap_or(true) {
                best = Some(path);
            }
        }
    }
    Ok(best)
}
