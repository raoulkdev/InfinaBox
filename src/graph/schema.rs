//! Table definitions for the local project graph.
//!
//! - `nodes` — one row per thing (a commit, a file identity). `content` is a
//!   JSON blob of type-specific fields, matching the product plan's node
//!   model (`id`, `type`, `content jsonb`, ...).
//! - `edges` — one row per relationship, carrying `evidence` — the literal,
//!   historically-accurate fact that justified the edge (e.g. "in commit X,
//!   this path was modified"). Edges are never rewritten after the fact;
//!   that immutability is what keeps old queries correct even after a file
//!   is later renamed again.
//! - `path_index` — maps *every* path a file identity has ever had (old and
//!   current) to that identity's node id. This is what makes renames
//!   transparent to queries: looking up an old, superseded path still
//!   resolves to the same identity as looking up its current path.

use anyhow::{Context, Result};
use rusqlite::Connection;

pub fn open_and_init(db_path: &str) -> Result<Connection> {
    let conn = Connection::open(db_path)
        .with_context(|| format!("failed to open graph database at '{db_path}'"))?;

    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS nodes (
            id          TEXT PRIMARY KEY,
            type        TEXT NOT NULL,
            label       TEXT NOT NULL,
            content     TEXT NOT NULL,
            created_at  TEXT NOT NULL,
            updated_at  TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS edges (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            from_node   TEXT NOT NULL REFERENCES nodes(id),
            to_node     TEXT NOT NULL REFERENCES nodes(id),
            relation    TEXT NOT NULL,
            confidence  REAL NOT NULL DEFAULT 1.0,
            evidence    TEXT NOT NULL,
            created_at  TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS path_index (
            path        TEXT PRIMARY KEY,
            node_id     TEXT NOT NULL REFERENCES nodes(id)
        );

        CREATE INDEX IF NOT EXISTS idx_edges_from ON edges(from_node);
        CREATE INDEX IF NOT EXISTS idx_edges_to   ON edges(to_node);
        "#,
    )
    .context("failed to initialize graph schema")?;

    Ok(conn)
}
