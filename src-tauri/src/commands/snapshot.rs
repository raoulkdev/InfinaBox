//! Thin Tauri wrappers over `infinabox_core::snapshot` for Studio's History
//! panel. Every change to the snapshot list emits `snapshots-changed`, and
//! going back (restore/undo) restarts a running game so what's on screen is
//! the restored game, not the one from before.
//!
//! Split the same way `watcher.rs` splits `start_watching` from
//! `watch_project_path`: the plain `create`/`restore`/`undo` functions take
//! an `on_changed` callback and no `AppHandle`, so when they report a
//! change is directly unit-testable against a real temp repository; the
//! `#[tauri::command]` wrappers only wire that callback to `app.emit(...)`
//! and the game restart. They're `#[tauri::command(async)]` (like
//! `overview.rs`) so git work, and a game restart that may run a Godot
//! import, happens off the main thread instead of freezing the window.
//!
//! Core's errors are already plain sentences written for users (with their
//! cause chained on), so they're passed through whole via `{:#}` rather
//! than re-worded or cut down to the outermost context.

use std::path::Path;

use infinabox_core::snapshot::{self, Snapshot};
use tauri::{AppHandle, Emitter};

/// Payload-less event, like `project-fs-changed`.
pub const EVENT_SNAPSHOTS_CHANGED: &str = "snapshots-changed";

/// Formats a core error with its whole cause chain ("outer: inner: ...") —
/// that's what `anyhow`'s alternate `{:#}` Display does. Generic so this
/// crate needs no direct `anyhow` dependency.
fn user_error(e: impl std::fmt::Display) -> String {
    format!("{e:#}")
}

/// Saves a snapshot; calls `on_changed` only when one was actually made
/// (`None` means nothing had changed since the last one).
fn create(
    project: &Path,
    title: &str,
    on_changed: impl FnOnce(),
) -> Result<Option<Snapshot>, String> {
    let made = snapshot::create_snapshot(project, title, None).map_err(user_error)?;
    if made.is_some() {
        on_changed();
    }
    Ok(made)
}

/// Goes back to `snapshot_id`. Calls `on_changed` even when this fails:
/// `restore_to` auto-saves uncommitted work as its own snapshot before
/// anything else, so a failure after that point has still added to the
/// list, and a spurious refetch is harmless while a missed one isn't.
fn restore(project: &Path, snapshot_id: &str, on_changed: impl FnOnce()) -> Result<Snapshot, String> {
    let result = snapshot::restore_to(project, snapshot_id).map_err(user_error);
    on_changed();
    result
}

/// Undoes the latest snapshot. `Ok(None)` (nothing to undo) touched
/// nothing, so it's the one outcome that doesn't call `on_changed`; an
/// error still does, for the same auto-save reason as `restore`.
fn undo(project: &Path, on_changed: impl FnOnce()) -> Result<Option<Snapshot>, String> {
    let result = snapshot::undo_last(project).map_err(user_error);
    if !matches!(result, Ok(None)) {
        on_changed();
    }
    result
}

fn emit_changed(app: &AppHandle) {
    let _ = app.emit(EVENT_SNAPSHOTS_CHANGED, ());
}

/// After going back, a running game still shows the old code; restart it
/// onto the restored files. A failed restart doesn't undo the restore
/// (which already happened and is what the user asked for), so it's only
/// logged — the game's own `game-state`/`game-error` events report it.
fn restart_game(app: &AppHandle) {
    if let Err(e) = crate::commands::godot::restart_if_running(app) {
        eprintln!("snapshot: restarting the game after going back failed: {e}");
    }
}

#[tauri::command(async)]
pub fn snapshot_list(project_path: String, limit: usize) -> Result<Vec<Snapshot>, String> {
    snapshot::list_snapshots(Path::new(&project_path), limit).map_err(user_error)
}

#[tauri::command(async)]
pub fn snapshot_create(app: AppHandle, project_path: String, title: String) -> Result<Option<Snapshot>, String> {
    create(Path::new(&project_path), &title, || emit_changed(&app))
}

#[tauri::command(async)]
pub fn snapshot_restore(app: AppHandle, project_path: String, snapshot_id: String) -> Result<Snapshot, String> {
    let restored = restore(Path::new(&project_path), &snapshot_id, || emit_changed(&app))?;
    restart_game(&app);
    Ok(restored)
}

#[tauri::command(async)]
pub fn snapshot_undo_last(app: AppHandle, project_path: String) -> Result<Option<Snapshot>, String> {
    let undone = undo(Path::new(&project_path), || emit_changed(&app))?;
    if undone.is_some() {
        restart_game(&app);
    }
    Ok(undone)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::fs;
    use std::path::PathBuf;

    /// A fresh folder under the system temp dir (never inside this repo,
    /// which core would refuse as "inside another git repository").
    fn temp_project(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "infinabox-snapshot-cmd-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn create_reports_a_change_only_when_a_snapshot_was_made() {
        let dir = temp_project("create");
        fs::write(dir.join("player.gd"), "extends Node2D\n").unwrap();

        let fired = Cell::new(0);
        let first = create(&dir, "First", || fired.set(fired.get() + 1)).unwrap();
        assert_eq!(first.expect("a snapshot").title, "First");
        assert_eq!(fired.get(), 1);

        // Nothing changed since: no snapshot, no event.
        let second = create(&dir, "Again", || fired.set(fired.get() + 1)).unwrap();
        assert!(second.is_none());
        assert_eq!(fired.get(), 1);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn undo_with_nothing_to_undo_reports_no_change() {
        let dir = temp_project("undo-none");
        fs::write(dir.join("a.txt"), "one\n").unwrap();
        create(&dir, "Only one", || {}).unwrap();

        // The very first snapshot has nothing before it to go back to.
        let fired = Cell::new(false);
        assert!(undo(&dir, || fired.set(true)).unwrap().is_none());
        assert!(!fired.get());

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn undo_and_restore_report_changes_and_rewrite_the_files() {
        let dir = temp_project("undo-restore");
        let file = dir.join("a.txt");
        fs::write(&file, "one\n").unwrap();
        let first = create(&dir, "One", || {}).unwrap().unwrap();
        fs::write(&file, "two\n").unwrap();
        create(&dir, "Two", || {}).unwrap().unwrap();

        let fired = Cell::new(0);
        let undone = undo(&dir, || fired.set(fired.get() + 1)).unwrap().expect("undid");
        assert!(undone.title.starts_with("Went back to"), "{}", undone.title);
        assert_eq!(fs::read_to_string(&file).unwrap(), "one\n");
        assert_eq!(fired.get(), 1);

        fs::write(&file, "three\n").unwrap();
        create(&dir, "Three", || {}).unwrap().unwrap();
        restore(&dir, &first.id, || fired.set(fired.get() + 1)).unwrap();
        assert_eq!(fs::read_to_string(&file).unwrap(), "one\n");
        assert_eq!(fired.get(), 2);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn restore_failure_still_reports_and_keeps_core_message() {
        let dir = temp_project("restore-bad");
        fs::write(dir.join("a.txt"), "one\n").unwrap();
        create(&dir, "One", || {}).unwrap();

        let fired = Cell::new(false);
        let err = restore(&dir, "not-a-real-snapshot", || fired.set(true)).unwrap_err();
        assert!(err.contains("no snapshot 'not-a-real-snapshot'"), "{err}");
        assert!(fired.get());

        fs::remove_dir_all(&dir).unwrap();
    }
}
