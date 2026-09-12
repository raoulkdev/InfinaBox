//! Milestone 2 (Phase 0): a recursive file-system watcher that notices every
//! add/rename/move/delete in a real project directory in real time.
//! This is the "eyes" of InfinaBox's shared project memory — everything
//! downstream (the graph, the agents) trusts what this module reports.

use std::path::Path;
use std::sync::mpsc::channel;

use anyhow::{Context, Result};
use chrono::Utc;
use notify::event::{ModifyKind, RenameMode};
use notify::{EventKind, RecursiveMode, Watcher};
use serde::Serialize;

/// A single structured file-system event, printed to stdout as one JSON line.
#[derive(Serialize)]
struct FileEvent {
    event_type: &'static str,
    path: String,
    timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    to: Option<String>,
}

impl FileEvent {
    fn simple(event_type: &'static str, path: String) -> Self {
        Self {
            event_type,
            path,
            timestamp: Utc::now().to_rfc3339(),
            from: None,
            to: None,
        }
    }

    fn renamed(path: String, from: Option<String>, to: Option<String>) -> Self {
        Self {
            event_type: "renamed",
            path,
            timestamp: Utc::now().to_rfc3339(),
            from,
            to,
        }
    }

    fn print(&self) {
        match serde_json::to_string(self) {
            Ok(line) => println!("{line}"),
            Err(e) => eprintln!("failed to serialize file event: {e}"),
        }
    }
}

/// Recursively watch `path` for filesystem events and print one JSON line per
/// event to stdout until the process is killed (e.g. Ctrl+C).
pub fn run(path: &str) -> Result<()> {
    let root = Path::new(path);

    let (tx, rx) = channel::<notify::Result<notify::Event>>();

    let mut watcher = notify::recommended_watcher(tx)
        .context("failed to create filesystem watcher")?;
    watcher
        .watch(root, RecursiveMode::Recursive)
        .with_context(|| format!("failed to watch path: {path}"))?;

    // Confirmation line so a human running the CLI knows it started successfully.
    println!("watching: {path}");

    for res in rx {
        match res {
            Ok(event) => handle_event(event),
            Err(e) => eprintln!("watch error: {e}"),
        }
    }

    // The channel only closes when the watcher is dropped, which shouldn't
    // happen while this loop is running; reaching here just means the
    // sender side went away (e.g. during shutdown).
    Ok(())
}

fn handle_event(event: notify::Event) {
    let paths: Vec<String> = event
        .paths
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();

    match event.kind {
        EventKind::Create(_) => {
            for path in paths {
                FileEvent::simple("created", path).print();
            }
        }
        EventKind::Modify(ModifyKind::Name(rename_mode)) => {
            handle_rename(rename_mode, paths);
        }
        EventKind::Modify(_) => {
            for path in paths {
                FileEvent::simple("modified", path).print();
            }
        }
        EventKind::Remove(_) => {
            for path in paths {
                FileEvent::simple("removed", path).print();
            }
        }
        _ => {
            // EventKind::Access, EventKind::Any, EventKind::Other, etc. —
            // not one of the core event types this phase cares about.
            for path in paths {
                FileEvent::simple("other", path).print();
            }
        }
    }
}

fn handle_rename(rename_mode: RenameMode, paths: Vec<String>) {
    match rename_mode {
        RenameMode::Both => {
            // notify's convention: paths[0] is the old path, paths[1] the new one.
            if paths.len() >= 2 {
                let from = paths[0].clone();
                let to = paths[1].clone();
                FileEvent::renamed(to.clone(), Some(from), Some(to)).print();
            } else {
                // Malformed/short event — fall back to whatever we got rather
                // than panicking.
                for path in paths {
                    FileEvent::renamed(path.clone(), None, Some(path)).print();
                }
            }
        }
        RenameMode::From => {
            for path in paths {
                FileEvent::renamed(path.clone(), Some(path), None).print();
            }
        }
        RenameMode::To => {
            for path in paths {
                FileEvent::renamed(path.clone(), None, Some(path)).print();
            }
        }
        RenameMode::Any | RenameMode::Other => {
            // Some backend gave us a rename but couldn't say which side.
            // Emit it as a rename with only the generic path populated,
            // rather than trying (and failing) to correlate it.
            for path in paths {
                FileEvent::renamed(path, None, None).print();
            }
        }
    }
}
