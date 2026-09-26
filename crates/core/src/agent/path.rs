//! Finding the user's agent CLI the way their own terminal would.
//!
//! A GUI app launched from the macOS Dock or Finder does not inherit the
//! `PATH` the user's shell sets up (Homebrew, `~/.local/bin`, nvm, ...), so
//! a plain lookup against our own process's `PATH` often misses a `claude`
//! that works fine in Terminal. `login_shell_path` asks the user's
//! interactive login shell once — the same concern
//! `src-tauri/src/commands/terminal.rs` solves by spawning its shell with
//! `-l` — and both detection and spawning use the result.

use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{OnceLock, mpsc};
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
/// On Windows a `name.exe` anywhere on the path is preferred over a
/// `name.cmd` npm shim. (Windows isn't a Phase A target: a `.cmd` runs
/// through cmd.exe, whose argument escaping differs from a normal
/// program's, so passing chat messages to a shim needs checking before
/// relying on it.)
pub fn find_on_path(name: &str, path_var: &OsStr) -> Option<PathBuf> {
    let find = |file: &str| {
        std::env::split_paths(path_var)
            .map(|dir| dir.join(file))
            .find(|candidate| candidate.is_file())
    };
    if cfg!(windows) {
        return find(&format!("{name}.exe"))
            .or_else(|| find(name))
            .or_else(|| find(&format!("{name}.cmd")));
    }
    find(name)
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
    query_shell_path(&shell, LOGIN_SHELL_TIMEOUT)
}

/// Runs `shell -i -l -c <print PATH between markers>` and returns the
/// printed `PATH`, or `None` if it isn't printed within `timeout`.
///
/// `-i` as well as `-l`: zsh (the macOS default) reads `.zshrc` — where
/// nvm and `~/.local/bin` PATH edits usually live — only for interactive
/// shells. `printenv PATH` rather than `$PATH` so fish (whose `$PATH` is a
/// list) prints the same colon-separated form. stdin is null, so an rc file
/// waiting for input gets EOF; one that hangs anyway hits the timeout.
///
/// Output is read on a helper thread that sends chunks over a channel, and
/// we stop as soon as the end marker arrives: a profile that leaves a
/// background process holding stdout open would otherwise keep the pipe
/// from ever reaching EOF. That thread is never joined; it ends when the
/// pipe closes.
fn query_shell_path(shell: &OsStr, timeout: Duration) -> Option<OsString> {
    let script = format!("printf '%s' '{START_MARKER}'; printenv PATH; printf '%s' '{END_MARKER}'");
    let mut child = Command::new(shell)
        .args(["-i", "-l", "-c", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let (tx, rx) = mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match stdout.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });

    let deadline = Instant::now() + timeout;
    let mut out = Vec::new();
    let found = loop {
        if let Some(path) = extract_marked_path(&String::from_utf8_lossy(&out)) {
            break Some(path);
        }
        let Some(left) = deadline.checked_duration_since(Instant::now()) else {
            break None;
        };
        match rx.recv_timeout(left) {
            Ok(chunk) => out.extend_from_slice(&chunk),
            // Timed out, or the pipe closed without the end marker.
            Err(_) => break None,
        }
    };
    // Don't leave the shell running (or a zombie), whatever happened.
    if !matches!(child.try_wait(), Ok(Some(_))) {
        let _ = child.kill();
        let _ = child.wait();
    }
    found.map(OsString::from)
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

    /// A stand-in shell script (it ignores `-i -l -c ...`).
    #[cfg(unix)]
    fn fake_shell(body: &str) -> (tempfile::TempDir, PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fakesh");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        (dir, path)
    }

    #[cfg(unix)]
    #[test]
    fn a_real_shell_prints_its_path() {
        let path = query_shell_path(OsStr::new("/bin/sh"), Duration::from_secs(10))
            .expect("sh should print PATH");
        assert!(!path.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn a_background_process_holding_stdout_does_not_block() {
        let (_dir, shell) = fake_shell(&format!(
            "sleep 30 &\nprintf '%s/from/profile%s' '{START_MARKER}' '{END_MARKER}'"
        ));
        let started = Instant::now();
        let path = query_shell_path(shell.as_os_str(), Duration::from_secs(5));
        assert_eq!(path, Some(OsString::from("/from/profile")));
        assert!(
            started.elapsed() < Duration::from_secs(4),
            "{:?}",
            started.elapsed()
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_hanging_profile_times_out() {
        let (_dir, shell) = fake_shell("exec sleep 30");
        let started = Instant::now();
        assert_eq!(
            query_shell_path(shell.as_os_str(), Duration::from_millis(500)),
            None
        );
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "{:?}",
            started.elapsed()
        );
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
