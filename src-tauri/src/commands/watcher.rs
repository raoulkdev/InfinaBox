//! Live filesystem watching for the currently open project — so every
//! panel that reads from disk (Files/Documents/Graphs/... file trees, open
//! file content, git Overview/Changes) picks up changes made outside the
//! app (another editor, git operations run in the embedded terminal, a
//! build script) without the user having to restart InfinaBox.
//!
//! Deliberately coarse-grained: rather than tracking exactly which paths
//! changed and diffing that against what each frontend panel currently
//! shows, this just debounces raw filesystem events into a single
//! `project-fs-changed` event and lets each panel re-fetch its own current
//! view — the same `read_file`/`list_directory` round trip a manual
//! refresh would do, just triggered automatically instead of never.
//!
//! Split the same way `terminal.rs` splits `TerminalSession` from its
//! `#[tauri::command]` wrappers: `start_watching` takes a plain callback
//! and no `AppHandle`, so it's directly unit-testable against a real
//! temp directory; `watch_project_path` is the thin, untested wrapper that
//! wires that callback to `app.emit(...)`.

use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use notify_debouncer_mini::notify::{Error as NotifyError, RecursiveMode};
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};
use tauri::{AppHandle, Emitter, Manager};

type FsWatcher = notify_debouncer_mini::notify::RecommendedWatcher;

/// Starts watching `path` recursively, calling `on_change` (debounced by
/// `debounce`) whenever anything changes anywhere under it. The returned
/// `Debouncer` must be kept alive for as long as the watch should run —
/// dropping it stops the background watch thread.
fn start_watching(
    path: &Path,
    debounce: Duration,
    mut on_change: impl FnMut() + Send + 'static,
) -> Result<Debouncer<FsWatcher>, NotifyError> {
    let mut debouncer = new_debouncer(debounce, move |result: DebounceEventResult| {
        // An empty or error batch isn't a real change worth telling
        // anyone about — skip so nothing refetches for no reason.
        let Ok(events) = result else { return };
        if events.is_empty() {
            return;
        }
        on_change();
    })?;

    debouncer.watcher().watch(path, RecursiveMode::Recursive)?;
    Ok(debouncer)
}

/// Holds the live watcher for whichever project is currently open — at
/// most one at a time, matching the rest of the app's single-project
/// model. Dropping a `Debouncer` stops its background watch thread, so
/// replacing the value here (what `watch_project_path` does on every call)
/// is exactly how switching projects retargets the watch onto the new
/// root; nothing has to explicitly "stop" the old one.
#[derive(Default)]
pub struct WatcherState(pub Mutex<Option<Debouncer<FsWatcher>>>);

/// (Re)starts watching `path` recursively, replacing any previously
/// watched project. Emits a debounced `project-fs-changed` event (no
/// payload — every listener already knows what it's currently showing and
/// just re-fetches that) whenever anything changes anywhere under `path`.
/// Debounced on a short delay so a burst of writes — a git checkout, an
/// editor auto-save, this app's own Save button — collapses into one
/// event instead of a flood of them.
#[tauri::command]
pub fn watch_project_path(app: AppHandle, path: String) -> Result<(), String> {
    let state = app.state::<WatcherState>();
    let mut guard = state.0.lock().map_err(|_| "watcher state poisoned".to_string())?;

    let app_for_emit = app.clone();
    let debouncer = start_watching(Path::new(&path), Duration::from_millis(400), move || {
        let _ = app_for_emit.emit("project-fs-changed", ());
    })
    .map_err(|e| format!("failed to watch '{path}': {e}"))?;

    *guard = Some(debouncer);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Instant;

    /// Polls `predicate` until it's true or `deadline` passes, whichever
    /// comes first — the same wait-for-real-async-effect pattern
    /// `terminal.rs`'s tests use for its background PTY reader thread.
    fn wait_until(deadline: Duration, predicate: impl Fn() -> bool) -> bool {
        let start = Instant::now();
        loop {
            if predicate() {
                return true;
            }
            if start.elapsed() > deadline {
                return predicate();
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("infinabox-watcher-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("creating the test's temp directory should succeed");
        dir
    }

    #[test]
    fn reports_a_real_file_created_after_watching_starts() {
        let dir = temp_dir("create");
        let fired = Arc::new(AtomicUsize::new(0));
        let fired_for_cb = Arc::clone(&fired);

        let _debouncer = start_watching(&dir, Duration::from_millis(100), move || {
            fired_for_cb.fetch_add(1, Ordering::SeqCst);
        })
        .expect("watching a real, existing directory should succeed");

        std::fs::write(dir.join("new-file.txt"), b"hello")
            .expect("writing a real file into the watched directory should succeed");

        let saw_it = wait_until(Duration::from_secs(5), || fired.load(Ordering::SeqCst) > 0);
        std::fs::remove_dir_all(&dir).ok();

        assert!(
            saw_it,
            "expected on_change to fire after a real file was created in the watched directory"
        );
    }

    #[test]
    fn reports_a_real_file_edited_in_a_nested_subdirectory() {
        let dir = temp_dir("nested");
        let nested = dir.join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();
        let file = nested.join("doc.md");
        std::fs::write(&file, "before").unwrap();

        let fired = Arc::new(AtomicUsize::new(0));
        let fired_for_cb = Arc::clone(&fired);

        let _debouncer = start_watching(&dir, Duration::from_millis(100), move || {
            fired_for_cb.fetch_add(1, Ordering::SeqCst);
        })
        .expect("watching a real, existing directory should succeed");

        std::fs::write(&file, "after").expect("editing the nested file should succeed");

        let saw_it = wait_until(Duration::from_secs(5), || fired.load(Ordering::SeqCst) > 0);
        std::fs::remove_dir_all(&dir).ok();

        assert!(
            saw_it,
            "expected a recursive watch to report a change made several directories deep, matching \
             what the whole project tree (Files/Documents/Graphs/...) needs"
        );
    }

    #[test]
    fn multiple_rapid_writes_collapse_into_far_fewer_events_than_writes() {
        let dir = temp_dir("debounce");
        let count = Arc::new(AtomicUsize::new(0));
        let count_for_cb = Arc::clone(&count);

        let _debouncer = start_watching(&dir, Duration::from_millis(300), move || {
            count_for_cb.fetch_add(1, Ordering::SeqCst);
        })
        .expect("watching a real, existing directory should succeed");

        let file = dir.join("churn.txt");
        for i in 0..20 {
            std::fs::write(&file, format!("write {i}")).unwrap();
            std::thread::sleep(Duration::from_millis(5));
        }

        // Give the debounce window time to flush after the last write,
        // then a little more margin before reading the final count.
        std::thread::sleep(Duration::from_millis(800));
        let fired = count.load(Ordering::SeqCst);
        std::fs::remove_dir_all(&dir).ok();

        assert!(
            fired >= 1 && fired < 20,
            "expected 20 rapid writes to debounce into noticeably fewer than 20 events, got {fired}"
        );
    }

    #[test]
    fn errors_cleanly_instead_of_silently_watching_nothing_for_a_missing_path() {
        let missing = std::env::temp_dir().join(format!("infinabox-watcher-does-not-exist-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&missing);

        let result = start_watching(&missing, Duration::from_millis(100), || {});

        assert!(
            result.is_err(),
            "watching a path that doesn't exist should fail loudly rather than silently watch nothing"
        );
    }
}
