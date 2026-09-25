//! Downloads, verifies (SHA-512), and unpacks the pinned Godot release into
//! InfinaBox's app data directory. Phase A Task C fills this in.

use std::path::{Path, PathBuf};

use anyhow::Result;

use super::types::InstallProgress;

/// One official release asset for `super::PINNED_VERSION`: the file name
/// under `https://github.com/godotengine/godot/releases/download/<version>/`
/// and its SHA-512 from that release's `SHA512-SUMS.txt`.
pub struct PinnedAsset {
    pub platform: &'static str,
    pub file_name: &'static str,
    pub sha512: &'static str,
}

/// Recorded 2026-09-25 from the official 4.7.2-stable `SHA512-SUMS.txt`.
pub const PINNED_ASSETS: &[PinnedAsset] = &[
    PinnedAsset {
        platform: "linux-x86_64",
        file_name: "Godot_v4.7.2-stable_linux.x86_64.zip",
        sha512: "9aa00f7a605200940bce3027a567b782f49bd8e940dd06ae9e987bd65aee1b1467edd56ed84fcdcbdd44354bf613bdbb4e5d2913e925850368e150c59ed54c65",
    },
    PinnedAsset {
        platform: "macos-universal",
        file_name: "Godot_v4.7.2-stable_macos.universal.zip",
        sha512: "38aa16e5bba2083941fc5b3e54be0089bd4cc35e32415f5b9fd9a8a6a7b9818255d44532ea8ef94b5aef56c4b407c2d634fa4f657e4ebe681ebbf59b7bac69ca",
    },
    PinnedAsset {
        platform: "windows-x86_64",
        file_name: "Godot_v4.7.2-stable_win64.exe.zip",
        sha512: "83decd58fdf67b9d657958a1ae6bf1929c20785315a81effe245874cdc57acb709bf868e00778a96984338c1b29dafdb453c6847747694621c6ecf5da2259993",
    },
];

/// Installs the pinned Godot under `app_data/godot/<version>/`, reporting
/// progress through `on_progress`, and returns the executable's path.
pub fn install(
    _app_data: &Path,
    _on_progress: &mut dyn FnMut(InstallProgress),
) -> Result<PathBuf> {
    anyhow::bail!("not implemented yet: godot::install::install")
}
