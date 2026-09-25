//! Finds a usable Godot: the managed install first, then a user-configured
//! path. A binary only counts as installed when it really runs: the version
//! shown is whatever its own `--version` printed.

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use super::install;
use super::types::GodotStatus;

/// The user-configured Godot path (Phase A: this env var only; a settings UI
/// comes later).
pub const GODOT_PATH_ENV: &str = "INFINABOX_GODOT";

/// `--version` is instant on a working binary; anything slower is broken.
const VERSION_TIMEOUT: Duration = Duration::from_secs(20);
/// A version string is one short line; never buffer more than this.
const VERSION_MAX_BYTES: u64 = 4096;

pub fn status(app_data: &Path) -> GodotStatus {
    let user_path = std::env::var_os(GODOT_PATH_ENV).map(PathBuf::from);
    status_with(app_data, user_path.as_deref())
}

/// `status` with the user-configured path passed in, so it's testable
/// without touching the process environment.
pub fn status_with(app_data: &Path, user_path: Option<&Path>) -> GodotStatus {
    if let Some(managed) = install::managed_executable(app_data).filter(|p| p.is_file())
        && let Some(version) = read_version(&managed)
    {
        return GodotStatus {
            installed: true,
            version: Some(version),
            path: Some(managed),
            managed: true,
        };
    }
    if let Some(path) = user_path.filter(|p| p.is_file())
        && let Some(version) = read_version(path)
    {
        return GodotStatus {
            installed: true,
            version: Some(version),
            path: Some(path.to_path_buf()),
            managed: false,
        };
    }
    GodotStatus {
        installed: false,
        version: None,
        path: None,
        managed: false,
    }
}

/// Runs `<godot> --version` and returns its first non-empty stdout line
/// (e.g. `4.7.2.stable.official.ed1daf0bf`), or `None` if it didn't run,
/// failed, or hung.
pub fn read_version(godot: &Path) -> Option<String> {
    read_version_within(godot, VERSION_TIMEOUT)
}

/// `read_version` with an explicit overall deadline. Everything is bounded:
/// the wait for exit, the wait for output (a background process the binary
/// left behind may hold stdout open forever), and the bytes read.
fn read_version_within(godot: &Path, timeout: Duration) -> Option<String> {
    let deadline = Instant::now() + timeout;
    let mut child = Command::new(godot)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    let (tx, rx) = mpsc::channel();
    // Sends the first non-empty line as soon as it arrives, or nothing at
    // EOF. May outlive this call if something keeps the pipe open; it then
    // ends when that pipe closes.
    thread::spawn(move || {
        let reader = BufReader::new(stdout.take(VERSION_MAX_BYTES));
        let first = reader
            .lines()
            .map_while(Result::ok)
            .map(|l| l.trim().to_string())
            .find(|l| !l.is_empty());
        let _ = tx.send(first);
    });

    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    if !status.success() {
        return None;
    }
    rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .ok()
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use crate::godot::test_support::fake_binary;
    use crate::godot::test_support::real_godot;

    #[test]
    fn nothing_installed_or_configured_is_not_installed() {
        let app_data = tempfile::tempdir().unwrap();
        assert_eq!(
            status_with(app_data.path(), None),
            GodotStatus {
                installed: false,
                version: None,
                path: None,
                managed: false
            }
        );
    }

    #[test]
    fn a_configured_path_that_does_not_exist_is_not_installed() {
        let app_data = tempfile::tempdir().unwrap();
        let status = status_with(app_data.path(), Some(Path::new("/no/such/godot")));
        assert!(!status.installed);
        assert_eq!(status.path, None);
    }

    #[test]
    fn a_file_that_is_not_a_working_godot_is_not_installed() {
        let app_data = tempfile::tempdir().unwrap();
        let fake = app_data.path().join("not-godot.txt");
        std::fs::write(&fake, "hello").unwrap();
        assert!(!status_with(app_data.path(), Some(&fake)).installed);
        assert_eq!(read_version(&fake), None);
    }

    /// A binary that prints its version but leaves a background process
    /// holding stdout open used to block until that process exited.
    #[cfg(unix)]
    #[test]
    fn a_background_process_holding_stdout_does_not_block_the_version() {
        let dir = tempfile::tempdir().unwrap();
        let fake = fake_binary(dir.path(), "godot", "sleep 10 &\necho 4.7.2.fake");
        let started = Instant::now();
        assert_eq!(read_version(&fake).as_deref(), Some("4.7.2.fake"));
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
    }

    #[cfg(unix)]
    #[test]
    fn silence_with_stdout_held_open_times_out_at_the_deadline() {
        let dir = tempfile::tempdir().unwrap();
        let fake = fake_binary(dir.path(), "godot", "sleep 10 &\nexit 0");
        let started = Instant::now();
        assert_eq!(read_version_within(&fake, Duration::from_secs(1)), None);
        let took = started.elapsed();
        assert!(took < Duration::from_secs(3), "{took:?}");
    }

    #[cfg(unix)]
    #[test]
    fn a_huge_unterminated_output_is_capped() {
        let dir = tempfile::tempdir().unwrap();
        let fake = fake_binary(
            dir.path(),
            "godot",
            // `exit 0`: `tr` dies of SIGPIPE once we stop reading.
            "head -c 5000000 /dev/zero | tr '\\0' a\nexit 0",
        );
        let version = read_version(&fake).unwrap();
        assert_eq!(version.len() as u64, VERSION_MAX_BYTES);
    }

    #[test]
    #[ignore = "needs a real Godot (INFINABOX_GODOT); run with --ignored"]
    fn reads_the_real_version_from_a_user_configured_godot() {
        let godot = real_godot();
        let app_data = tempfile::tempdir().unwrap();
        let recorded = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/godot/version.txt"),
        )
        .unwrap();
        assert_eq!(
            status_with(app_data.path(), Some(&godot)),
            GodotStatus {
                installed: true,
                version: Some(recorded.trim().to_string()),
                path: Some(godot.clone()),
                managed: false,
            }
        );
    }

    #[cfg(unix)]
    #[test]
    #[ignore = "needs a real Godot (INFINABOX_GODOT); run with --ignored"]
    fn prefers_the_managed_install_over_a_configured_path() {
        let godot = real_godot();
        let app_data = tempfile::tempdir().unwrap();
        let managed = install::managed_executable(app_data.path()).unwrap();
        std::fs::create_dir_all(managed.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&godot, &managed).unwrap();

        let status = status_with(app_data.path(), Some(&godot));
        assert!(status.installed && status.managed);
        assert_eq!(status.path, Some(managed));
        assert!(status.version.unwrap().starts_with("4.7.2.stable"));
    }
}
