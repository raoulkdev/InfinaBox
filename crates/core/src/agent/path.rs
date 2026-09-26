//! Finding the user's agent CLI the way their own terminal would.
//!
//! A GUI app launched from the macOS Dock or Finder does not inherit the
//! `PATH` the user's shell sets up (Homebrew, `~/.local/bin`, nvm, ...), so
//! a plain lookup against our own process's `PATH` often misses a `claude`
//! that works fine in Terminal. `login_shell_path` asks the user's login
//! shell once — the same concern `src-tauri/src/commands/terminal.rs`
//! solves by spawning its shell with `-l` — and both detection and spawning
//! use the result.

use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// How long the login shell gets to print its `PATH`. A slow shell profile
/// shouldn't stall detection forever; past this we use our own `PATH`.
const LOGIN_SHELL_TIMEOUT: Duration = Duration::from_secs(5);

/// Markers around the printed `PATH`, so anything a profile prints to
/// stdout (banners, `fortune`, ...) can't be mistaken for part of it.
const START_MARKER: &str = "__INFINABOX_PATH_START__";
const END_MARKER: &str = "__INFINABOX_PATH_END__";

/// True if `name` resolves to an executable file somewhere on this
/// process's `PATH`, the same resolution order a shell uses — moved here
/// from `src-tauri/src/commands/environment.rs` (which keeps its own copy).
pub fn is_on_path(name: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|path| find_on_path(name, &path).is_some())
}

/// The full path of `name` in the given `PATH`-style list, if it's there.
/// On Windows `.exe` and `.cmd` (how npm installs CLIs there) also match.
pub fn find_on_path(name: &str, path_var: &OsStr) -> Option<PathBuf> {
    std::env::split_paths(path_var).find_map(|dir| {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        if cfg!(windows) {
            for ext in ["exe", "cmd"] {
                let candidate = dir.join(format!("{name}.{ext}"));
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        None
    })
}

/// The `PATH` the user's login shell sets up, followed by any entries of
/// our own `PATH` it doesn't already include. Resolved once per process and
/// cached. Falls back to our own `PATH` when there's no `$SHELL` (Windows),
/// the shell fails, or it takes longer than `LOGIN_SHELL_TIMEOUT`.
pub fn login_shell_path() -> &'static OsString {
    static CACHED: OnceLock<OsString> = OnceLock::new();
    CACHED.get_or_init(|| {
        let own = std::env::var_os("PATH").unwrap_or_default();
        match query_login_shell_path() {
            Some(login) => merge_paths(&login, &own),
            None => own,
        }
    })
}

fn query_login_shell_path() -> Option<OsString> {
    if cfg!(windows) {
        return None;
    }
    let shell = std::env::var_os("SHELL").filter(|s| !s.is_empty())?;
    let script = format!("printf '%s%s%s' '{START_MARKER}' \"$PATH\" '{END_MARKER}'");
    let mut child = Command::new(shell)
        .args(["-l", "-c", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    // Read on another thread so a chatty profile can't fill the pipe and
    // block the shell while we wait on it.
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut out = Vec::new();
        let _ = stdout.read_to_end(&mut out);
        out
    });

    let deadline = Instant::now() + LOGIN_SHELL_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                // Anything the shell started in the background could keep
                // the pipe open; don't wait for the reader.
                return None;
            }
        }
    }
    let out = reader.join().ok()?;
    extract_marked_path(&String::from_utf8_lossy(&out)).map(OsString::from)
}

fn extract_marked_path(output: &str) -> Option<String> {
    let start = output.find(START_MARKER)? + START_MARKER.len();
    let end = start + output[start..].find(END_MARKER)?;
    let path = output[start..end].trim();
    (!path.is_empty()).then(|| path.to_string())
}

/// `first`'s entries in order, then `second`'s entries not already present.
fn merge_paths(first: &OsStr, second: &OsStr) -> OsString {
    let mut dirs: Vec<PathBuf> = std::env::split_paths(first).collect();
    for dir in std::env::split_paths(second) {
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    std::env::join_paths(dirs).unwrap_or_else(|_| first.to_os_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_tool_that_is_really_on_path() {
        // `cargo` is on PATH wherever these tests run — a real positive case.
        assert!(is_on_path("cargo"));
        assert!(!is_on_path("definitely-not-a-real-cli-tool-xyz123"));
    }

    #[test]
    fn find_on_path_returns_the_full_path() {
        let path = std::env::var_os("PATH").unwrap();
        let found = find_on_path("cargo", &path).expect("cargo on PATH");
        assert!(found.is_absolute() || found.components().count() > 1);
        assert!(found.is_file());
    }

    #[test]
    fn login_shell_path_contains_our_own_path_entries() {
        let resolved = login_shell_path();
        let own = std::env::var_os("PATH").unwrap();
        let resolved: Vec<PathBuf> = std::env::split_paths(resolved).collect();
        for dir in std::env::split_paths(&own) {
            assert!(resolved.contains(&dir), "{dir:?} missing");
        }
    }

    #[test]
    fn extracts_the_path_between_markers_ignoring_profile_noise() {
        let out = format!("Welcome!\n{START_MARKER}/opt/bin:/usr/bin{END_MARKER}");
        assert_eq!(
            extract_marked_path(&out).as_deref(),
            Some("/opt/bin:/usr/bin")
        );
        assert_eq!(extract_marked_path("no markers"), None);
    }

    #[test]
    fn merge_keeps_order_and_drops_duplicates() {
        let merged = merge_paths(OsStr::new("/a:/b"), OsStr::new("/b:/c"));
        let dirs: Vec<PathBuf> = std::env::split_paths(&merged).collect();
        assert_eq!(
            dirs,
            vec![
                PathBuf::from("/a"),
                PathBuf::from("/b"),
                PathBuf::from("/c")
            ]
        );
    }
}
