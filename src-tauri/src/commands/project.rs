//! Phase 1, milestone 4: the local project map, made permanent and
//! automatic — wires infinabox_core::graph into the running app, with the
//! SQLite graph database stored per-project under the app's data directory
//! (not inside the user's Git repo).

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::Manager;

#[derive(Serialize)]
pub struct GraphSummary {
    pub commits_indexed: usize,
    pub commits_skipped: usize,
    pub file_identities_created: usize,
    pub edges_created: usize,
}

impl From<infinabox_core::graph::ingest::IngestReport> for GraphSummary {
    fn from(report: infinabox_core::graph::ingest::IngestReport) -> Self {
        GraphSummary {
            commits_indexed: report.commits_indexed,
            commits_skipped: report.commits_skipped,
            file_identities_created: report.file_identities_created,
            edges_created: report.edges_created,
        }
    }
}

/// Deterministically resolves the per-project graph database path for
/// `project_path`, under `data_dir` (the app's own data directory — this is
/// InfinaBox's state, not a project artifact, so it never lives inside the
/// user's Git repo). Creates the containing directory if it doesn't exist.
///
/// The same `project_path` always maps to the same file; different project
/// paths practically never collide (we hash the canonicalized path).
fn resolve_project_db_path(data_dir: &Path, project_path: &str) -> Result<PathBuf, String> {
    // Canonicalize when possible so `./foo` and `/abs/foo` (or a path visited
    // through a symlink) resolve to the same identity. Fall back to the raw
    // path if the project doesn't exist yet / isn't reachable, rather than
    // failing outright — refreshing a graph shouldn't hard-depend on the
    // path already existing at the moment we merely need to *name* the db.
    let canonical = std::fs::canonicalize(project_path)
        .unwrap_or_else(|_| PathBuf::from(project_path));
    let canonical_str = canonical.to_string_lossy();

    let hash = fnv1a_hash64(canonical_str.as_bytes());

    // Keep a short, sanitized human-readable slug alongside the hash purely
    // to make the projects/ directory browsable; the hash is what actually
    // guarantees uniqueness.
    let slug = sanitize_for_filename(
        canonical
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "project".to_string()),
    );

    let projects_dir = data_dir.join("projects");
    std::fs::create_dir_all(&projects_dir)
        .map_err(|e| format!("failed to create app data projects directory {:?}: {e}", projects_dir))?;

    Ok(projects_dir.join(format!("{slug}-{hash:016x}.graph.db")))
}

/// Small dependency-free 64-bit FNV-1a hash, used only to derive a stable,
/// collision-resistant filename component from a canonicalized project path.
fn fnv1a_hash64(bytes: &[u8]) -> u64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET_BASIS;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Replaces anything that isn't filesystem-safe with `_`, and trims the
/// result so we never produce an empty or absurdly long slug.
fn sanitize_for_filename(raw: String) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('_');
    let slug = if trimmed.is_empty() { "project" } else { trimmed };
    slug.chars().take(64).collect()
}

/// The actual logic behind `refresh_project_graph`, factored out to take a
/// plain `data_dir` so it's directly testable without a running Tauri app /
/// `AppHandle`.
fn refresh_project_graph_impl(data_dir: &Path, project_path: &str) -> Result<GraphSummary, String> {
    let db_path = resolve_project_db_path(data_dir, project_path)?;
    let db_path_str = db_path.to_string_lossy();

    let report = infinabox_core::graph::ingest::build_graph(project_path, &db_path_str)
        .map_err(|e| e.to_string())?;

    Ok(report.into())
}

/// The actual logic behind `ask_question`, factored out the same way as
/// `refresh_project_graph_impl` — same db path resolution, so it only ever
/// opens a graph that `refresh_project_graph_impl` could have built.
fn ask_question_impl(data_dir: &Path, project_path: &str, question: &str) -> Result<String, String> {
    let db_path = resolve_project_db_path(data_dir, project_path)?;
    let db_path_str = db_path.to_string_lossy();

    let conn = infinabox_core::graph::open_and_init(&db_path_str).map_err(|e| e.to_string())?;
    infinabox_core::ask::answer(&conn, question).map_err(|e| e.to_string())
}

/// Builds (or incrementally refreshes — ingestion is already idempotent
/// per-commit, see infinabox_core::graph::ingest) the graph for the project
/// at `project_path`, storing it under this app's data directory.
#[tauri::command]
pub fn refresh_project_graph(app: tauri::AppHandle, project_path: String) -> Result<GraphSummary, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("failed to resolve app data dir: {e}"))?;
    refresh_project_graph_impl(&data_dir, &project_path)
}

/// Answers `question` using only the graph already built for `project_path`.
#[tauri::command]
pub fn ask_question(app: tauri::AppHandle, project_path: String, question: String) -> Result<String, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("failed to resolve app data dir: {e}"))?;
    ask_question_impl(&data_dir, &project_path, &question)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real fixture repo used across the project's milestones: a genuine
    /// Godot-shaped Git repo with 8 commits, including a rename
    /// (scripts/elevator_logic.gd -> scripts/ElevatorLogic.gd) that the
    /// graph must thread as one continuous file identity.
    const FIXTURE_REPO: &str = "/Users/raoulkaleba/Developer/hollow-meridian-test";

    fn temp_data_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "infinabox-project-rs-test-{name}-{}",
            std::process::id()
        ));
        // Start from a clean slate so re-running the test suite doesn't
        // observe a stale db from a previous run.
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn resolve_project_db_path_is_stable_and_distinct_per_project() {
        let data_dir = temp_data_dir("path-resolution");

        let path_a1 = resolve_project_db_path(&data_dir, FIXTURE_REPO).unwrap();
        let path_a2 = resolve_project_db_path(&data_dir, FIXTURE_REPO).unwrap();
        assert_eq!(path_a1, path_a2, "same project path must resolve to the same db file");

        let path_b = resolve_project_db_path(&data_dir, "/tmp/some-other-project").unwrap();
        assert_ne!(path_a1, path_b, "different project paths must never collide");

        assert!(path_a1.starts_with(data_dir.join("projects")));
        assert!(path_a1.to_string_lossy().ends_with(".graph.db"));
    }

    #[test]
    fn refresh_is_idempotent_and_ask_is_grounded_against_real_fixture() {
        let data_dir = temp_data_dir("hollow-meridian");

        // First build: fresh database, every commit is new.
        let first = refresh_project_graph_impl(&data_dir, FIXTURE_REPO)
            .expect("first refresh_project_graph_impl call should succeed");
        assert_eq!(first.commits_indexed, 8, "fresh db should index all 8 fixture commits");
        assert_eq!(first.commits_skipped, 0);

        // Second build against the SAME resolved db path: nothing new to do.
        // This is the "made permanent" proof — the graph persisted between
        // calls instead of being rebuilt from scratch.
        let second = refresh_project_graph_impl(&data_dir, FIXTURE_REPO)
            .expect("second refresh_project_graph_impl call should succeed");
        assert_eq!(second.commits_indexed, 0, "second run should index nothing new");
        assert_eq!(second.commits_skipped, 8, "second run should skip all 8 already-indexed commits");

        // Grounded question-answering over the now-persisted graph, through
        // the rename: the fixture's known-correct answer names commit
        // b064346 and its summary.
        let answer = ask_question_impl(
            &data_dir,
            FIXTURE_REPO,
            "what commit last touched ElevatorLogic.gd, and why",
        )
        .expect("ask_question_impl should succeed against a built graph");

        assert!(
            answer.contains("b064346"),
            "answer should cite commit b064346, got: {answer}"
        );
        assert!(
            answer.contains("Fix elevator trigger race"),
            "answer should mention the commit's summary, got: {answer}"
        );
    }
}
