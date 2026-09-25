//! Godot integration (spec §10): managed install, locating the binary,
//! running the game with output capture, and parsing real Godot errors.
//! Plain Rust with callbacks — no Tauri types — so it's unit-testable here.

pub mod errors;
pub mod install;
pub mod locate;
pub mod run;
pub mod types;
pub mod validate;

pub use types::*;

/// The one Godot version InfinaBox manages (spec §10.1). Upgrading means
/// changing this and every checksum in `install::PINNED_ASSETS` together.
pub const PINNED_VERSION: &str = "4.7.2-stable";

/// Shared helpers for tests that need a real Godot.
#[cfg(test)]
pub(crate) mod test_support {
    use std::path::{Path, PathBuf};

    /// The real Godot binary for `#[ignore]` tests, from the same
    /// user-configured env var `locate` honours.
    pub fn real_godot() -> PathBuf {
        let path = std::env::var_os(super::locate::GODOT_PATH_ENV).unwrap_or_else(|| {
            panic!(
                "set {} to a real Godot {} binary to run this test",
                super::locate::GODOT_PATH_ENV,
                super::PINNED_VERSION
            )
        });
        let path = PathBuf::from(path);
        assert!(path.is_file(), "{} is not a file", path.display());
        path
    }

    /// A throwaway copy of `tests/fixtures/godot/projects/<name>`: running
    /// Godot writes a `.godot/` cache into the project, and the committed
    /// fixtures must stay exactly as recorded.
    pub fn fixture_project(name: &str) -> tempfile::TempDir {
        let src = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/godot/projects")
            .join(name);
        let dir = tempfile::tempdir().unwrap();
        for entry in std::fs::read_dir(&src).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_file() {
                std::fs::copy(entry.path(), dir.path().join(entry.file_name())).unwrap();
            }
        }
        dir
    }
}
