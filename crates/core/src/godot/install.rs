//! Downloads, verifies (SHA-512), and unpacks the pinned Godot release into
//! InfinaBox's app data directory (passed in; core never calls Tauri).
//!
//! Layout: `<app data>/godot/<PINNED_VERSION>/` holds the unpacked release.
//! Everything happens in a sibling staging folder first; the download is
//! written as `<asset>.part`, renamed only after its checksum matches, and
//! the staging folder becomes `<PINNED_VERSION>/` only once unpacking has
//! produced the executable. A half-finished install is never mistaken for a
//! real one.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use sha2::{Digest, Sha512};

use super::PINNED_VERSION;
use super::locate;
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

/// Official release downloads: `<base>/<version>/<file name>`.
pub const RELEASE_BASE_URL: &str = "https://github.com/godotengine/godot/releases/download";

/// Progress phases reported through `InstallProgress::phase`.
pub const PHASE_DOWNLOADING: &str = "downloading";
pub const PHASE_VERIFYING: &str = "verifying";
pub const PHASE_EXTRACTING: &str = "extracting";
pub const PHASE_DONE: &str = "done";

/// How often (in bytes) download progress is reported.
const PROGRESS_STEP: u64 = 512 * 1024;

/// The `PinnedAsset::platform` this build runs on, if Godot ships one.
pub fn current_platform() -> Option<&'static str> {
    if cfg!(target_os = "macos") {
        Some("macos-universal")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("linux-x86_64")
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        Some("windows-x86_64")
    } else {
        None
    }
}

pub fn pinned_asset() -> Result<&'static PinnedAsset> {
    let platform = current_platform().context(
        "InfinaBox doesn't have a pinned Godot build for this operating system and CPU yet",
    )?;
    PINNED_ASSETS
        .iter()
        .find(|a| a.platform == platform)
        .with_context(|| format!("no pinned Godot asset for {platform}"))
}

pub fn download_url(asset: &PinnedAsset) -> String {
    format!("{RELEASE_BASE_URL}/{PINNED_VERSION}/{}", asset.file_name)
}

/// Where the executable sits inside an unpacked release zip.
pub fn executable_relpath(asset: &PinnedAsset) -> PathBuf {
    if asset.platform.starts_with("macos") {
        // The macOS zip holds an app bundle.
        PathBuf::from("Godot.app/Contents/MacOS/Godot")
    } else {
        // Linux/Windows zips hold one binary named like the zip minus `.zip`
        // (confirmed for Linux against the real 4.7.2 download).
        PathBuf::from(asset.file_name.trim_end_matches(".zip"))
    }
}

/// `<app data>/godot/<PINNED_VERSION>/`.
pub fn managed_dir(app_data: &Path) -> PathBuf {
    app_data.join("godot").join(PINNED_VERSION)
}

/// The managed executable's path for this platform (whether or not it's
/// installed). `None` on a platform with no pinned build.
pub fn managed_executable(app_data: &Path) -> Option<PathBuf> {
    let asset = pinned_asset().ok()?;
    Some(managed_dir(app_data).join(executable_relpath(asset)))
}

/// Serialises installs within this process: two concurrent calls (e.g. a
/// double-clicked Install button) must not download over each other.
static INSTALL_LOCK: Mutex<()> = Mutex::new(());

/// Staging folders older than this are leftovers from a crashed install.
const STALE_STAGING_AGE: Duration = Duration::from_secs(60 * 60);
const STAGING_PREFIX: &str = ".partial-";

/// Installs the pinned Godot under `app_data/godot/<version>/`, reporting
/// progress through `on_progress`, and returns the executable's path.
/// Returns straight away if a working managed install is already there.
pub fn install(app_data: &Path, on_progress: &mut dyn FnMut(InstallProgress)) -> Result<PathBuf> {
    let asset = pinned_asset()?;
    let client = http_client(reqwest::blocking::Client::builder())?;
    install_locked(&client, app_data, &download_url(asset), asset, on_progress)
}

/// `install` from any URL, under the process-wide install lock. Re-checks
/// for a working install after taking the lock, so a call that waited on
/// another one returns that finished install instead of downloading again.
pub(crate) fn install_locked(
    client: &reqwest::blocking::Client,
    app_data: &Path,
    url: &str,
    asset: &PinnedAsset,
    on_progress: &mut dyn FnMut(InstallProgress),
) -> Result<PathBuf> {
    let _guard = INSTALL_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let exe = managed_dir(app_data).join(executable_relpath(asset));
    if working_install(&exe) {
        on_progress(InstallProgress {
            downloaded_bytes: 0,
            total_bytes: None,
            phase: PHASE_DONE.into(),
        });
        return Ok(exe);
    }
    install_from(client, app_data, url, asset, on_progress)
}

fn working_install(exe: &Path) -> bool {
    exe.is_file() && locate::read_version(exe).is_some()
}

fn http_client(builder: reqwest::blocking::ClientBuilder) -> Result<reqwest::blocking::Client> {
    builder
        .connect_timeout(Duration::from_secs(30))
        // Blocking reqwest applies this per read, so it's a stall timeout,
        // not a cap on the whole ~60MB download.
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| anyhow!("couldn't set up the HTTP client: {}", cause_chain(&e)))
}

/// An error's message followed by all of its causes, so users see the real
/// reason (e.g. "connection refused") and not just our summary of it.
fn cause_chain(e: &dyn std::error::Error) -> String {
    let mut msg = e.to_string();
    let mut source = e.source();
    while let Some(cause) = source {
        let text = cause.to_string();
        if !msg.contains(&text) {
            msg.push_str(": ");
            msg.push_str(&text);
        }
        source = cause.source();
    }
    msg
}

/// `.map_err` helper: "<what>: <full cause chain>" as one plain message.
trait WithCause<T> {
    fn with_cause(self, what: impl FnOnce() -> String) -> Result<T>;
}

impl<T, E: std::error::Error> WithCause<T> for std::result::Result<T, E> {
    fn with_cause(self, what: impl FnOnce() -> String) -> Result<T> {
        self.map_err(|e| anyhow!("{}: {}", what(), cause_chain(&e)))
    }
}

/// A staging folder name unique to this call (pid + time), so concurrent
/// installs from separate processes never share one.
fn unique_staging(godot_root: &Path) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    godot_root.join(format!("{STAGING_PREFIX}{}-{nanos}", std::process::id()))
}

/// Best-effort removal of staging folders left by installs that crashed
/// over an hour ago. Never touches a recent one (it may be in use).
fn remove_stale_staging(godot_root: &Path) {
    let Ok(entries) = fs::read_dir(godot_root) else {
        return;
    };
    for entry in entries.flatten() {
        let is_staging = entry
            .file_name()
            .to_string_lossy()
            .starts_with(STAGING_PREFIX);
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > STALE_STAGING_AGE);
        if is_staging && old {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

/// The download → verify → unpack pipeline, from any URL. Callers go
/// through `install_locked`; tests also call it directly.
pub(crate) fn install_from(
    client: &reqwest::blocking::Client,
    app_data: &Path,
    url: &str,
    asset: &PinnedAsset,
    on_progress: &mut dyn FnMut(InstallProgress),
) -> Result<PathBuf> {
    let godot_root = app_data.join("godot");
    let final_dir = managed_dir(app_data);
    remove_stale_staging(&godot_root);
    let staging = unique_staging(&godot_root);
    fs::create_dir_all(&staging).with_cause(|| format!("couldn't create {}", staging.display()))?;

    let result = (|| {
        let part = staging.join(format!("{}.part", asset.file_name));
        let (bytes, total, digest) = download(client, url, &part, on_progress)?;

        on_progress(InstallProgress {
            downloaded_bytes: bytes,
            total_bytes: total,
            phase: PHASE_VERIFYING.into(),
        });
        if !digest.eq_ignore_ascii_case(asset.sha512) {
            let _ = fs::remove_file(&part);
            bail!(
                "the Godot download failed its checksum check, so it was deleted and not installed \
                 ({} from {url}: expected SHA-512 {}, got {digest})",
                asset.file_name,
                asset.sha512
            );
        }
        let zip_path = staging.join(asset.file_name);
        fs::rename(&part, &zip_path).with_cause(|| {
            format!(
                "couldn't rename the verified download to {}",
                zip_path.display()
            )
        })?;

        on_progress(InstallProgress {
            downloaded_bytes: bytes,
            total_bytes: total,
            phase: PHASE_EXTRACTING.into(),
        });
        let unpacked = staging.join("unpacked");
        let file = File::open(&zip_path)
            .with_cause(|| format!("couldn't open the Godot download {}", zip_path.display()))?;
        let mut archive = zip::ZipArchive::new(file)
            .with_cause(|| "the Godot download isn't a valid zip".into())?;
        archive.extract(&unpacked).with_cause(|| {
            format!(
                "couldn't unpack the Godot download into {}",
                unpacked.display()
            )
        })?;
        fs::remove_file(&zip_path)
            .with_cause(|| format!("couldn't remove the unpacked zip {}", zip_path.display()))?;

        let exe_rel = executable_relpath(asset);
        let exe = unpacked.join(&exe_rel);
        if !exe.is_file() {
            bail!(
                "the Godot download didn't contain the expected executable {}",
                exe_rel.display()
            );
        }
        make_executable(&exe)?;

        // Another process may have finished the same install meanwhile; keep
        // its working copy rather than replacing it under a running game.
        let final_exe = final_dir.join(&exe_rel);
        if !working_install(&final_exe) {
            // A leftover (broken) install would block the rename.
            if final_dir.exists() {
                fs::remove_dir_all(&final_dir)
                    .with_cause(|| format!("couldn't replace {}", final_dir.display()))?;
            }
            fs::rename(&unpacked, &final_dir)
                .with_cause(|| format!("couldn't move Godot into {}", final_dir.display()))?;
        }

        on_progress(InstallProgress {
            downloaded_bytes: bytes,
            total_bytes: total,
            phase: PHASE_DONE.into(),
        });
        Ok(final_exe)
    })();

    let _ = fs::remove_dir_all(&staging);
    result
}

/// Streams `url` into `dest`, hashing as it goes. Returns (bytes, the
/// server's Content-Length if any, lowercase hex SHA-512).
fn download(
    client: &reqwest::blocking::Client,
    url: &str,
    dest: &Path,
    on_progress: &mut dyn FnMut(InstallProgress),
) -> Result<(u64, Option<u64>, String)> {
    let mut response = client
        .get(url)
        .send()
        .and_then(|r| r.error_for_status())
        .with_cause(|| format!("couldn't download Godot from {url}"))?;
    let total = response.content_length();

    let mut file =
        File::create(dest).with_cause(|| format!("couldn't write {}", dest.display()))?;
    let mut hasher = Sha512::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut downloaded: u64 = 0;
    let mut last_reported: u64 = 0;
    on_progress(InstallProgress {
        downloaded_bytes: 0,
        total_bytes: total,
        phase: PHASE_DOWNLOADING.into(),
    });
    loop {
        let n = response
            .read(&mut buf)
            .with_cause(|| format!("the Godot download from {url} was interrupted"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n])
            .with_cause(|| format!("couldn't write {}", dest.display()))?;
        downloaded += n as u64;
        if downloaded - last_reported >= PROGRESS_STEP {
            last_reported = downloaded;
            on_progress(InstallProgress {
                downloaded_bytes: downloaded,
                total_bytes: total,
                phase: PHASE_DOWNLOADING.into(),
            });
        }
    }
    file.sync_all()
        .with_cause(|| format!("couldn't write {}", dest.display()))?;
    if let Some(total) = total
        && downloaded != total
    {
        bail!("the Godot download from {url} ended early ({downloaded} of {total} bytes)");
    }
    if downloaded != last_reported {
        on_progress(InstallProgress {
            downloaded_bytes: downloaded,
            total_bytes: total,
            phase: PHASE_DOWNLOADING.into(),
        });
    }
    let digest = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    Ok((downloaded, total, digest))
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)
        .with_cause(|| format!("couldn't read {}", path.display()))?
        .permissions();
    perms.set_mode(perms.mode() | 0o755);
    fs::set_permissions(path, perms)
        .with_cause(|| format!("couldn't make {} executable", path.display()))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Cursor};
    use std::net::TcpListener;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    /// Serves `body` once over plain HTTP on 127.0.0.1 and returns the URL.
    fn serve_once(body: Vec<u8>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap() > 0 && line != "\r\n" {
                line.clear();
            }
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/zip\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(&body).unwrap();
        });
        format!("http://{addr}/{PINNED_VERSION}/godot.zip")
    }

    /// A local server, so never route through a proxy from the environment.
    fn test_client() -> reqwest::blocking::Client {
        http_client(reqwest::blocking::Client::builder().no_proxy()).unwrap()
    }

    fn sha512_hex(bytes: &[u8]) -> String {
        Sha512::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    /// A zip laid out like the Linux release: one binary at the root.
    fn linux_like_zip(exe_name: &str, contents: &[u8]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default().unix_permissions(0o644);
        writer.start_file(exe_name, options).unwrap();
        writer.write_all(contents).unwrap();
        writer.finish().unwrap().into_inner()
    }

    fn leak(s: String) -> &'static str {
        Box::leak(s.into_boxed_str())
    }

    #[test]
    fn every_supported_platform_has_one_pinned_asset_with_a_real_looking_checksum() {
        for platform in ["linux-x86_64", "macos-universal", "windows-x86_64"] {
            let assets: Vec<_> = PINNED_ASSETS
                .iter()
                .filter(|a| a.platform == platform)
                .collect();
            assert_eq!(assets.len(), 1, "{platform}");
            let sha = assets[0].sha512;
            assert_eq!(sha.len(), 128);
            assert!(
                sha.chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
            );
            assert!(assets[0].file_name.contains(PINNED_VERSION));
        }
        let linux = PINNED_ASSETS
            .iter()
            .find(|a| a.platform == "linux-x86_64")
            .unwrap();
        assert_eq!(
            download_url(linux),
            "https://github.com/godotengine/godot/releases/download/4.7.2-stable/Godot_v4.7.2-stable_linux.x86_64.zip"
        );
        assert_eq!(
            executable_relpath(linux),
            PathBuf::from("Godot_v4.7.2-stable_linux.x86_64")
        );
        let mac = PINNED_ASSETS
            .iter()
            .find(|a| a.platform == "macos-universal")
            .unwrap();
        assert_eq!(
            executable_relpath(mac),
            PathBuf::from("Godot.app/Contents/MacOS/Godot")
        );
    }

    #[test]
    fn a_checksum_mismatch_deletes_the_download_and_installs_nothing() {
        let app_data = tempfile::tempdir().unwrap();
        let zip = linux_like_zip("Godot_test_linux.x86_64", b"not really godot");
        let url = serve_once(zip);
        let asset = PinnedAsset {
            platform: "linux-x86_64",
            file_name: "Godot_test_linux.x86_64.zip",
            // Valid shape, wrong value.
            sha512: leak("0".repeat(128)),
        };
        let mut phases = Vec::new();
        let err = install_from(&test_client(), app_data.path(), &url, &asset, &mut |p| {
            phases.push(p.phase)
        })
        .unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("checksum"), "{msg}");
        assert!(msg.contains(asset.sha512), "{msg}");
        assert!(!phases.contains(&PHASE_EXTRACTING.to_string()));
        assert!(!managed_dir(app_data.path()).exists());
        // No partial download or staging folder left behind.
        let leftovers: Vec<_> = fs::read_dir(app_data.path().join("godot"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        assert_eq!(leftovers, Vec::<PathBuf>::new());
    }

    #[test]
    fn a_verified_download_is_unpacked_into_the_versioned_dir() {
        let app_data = tempfile::tempdir().unwrap();
        let zip = linux_like_zip("Godot_test_linux.x86_64", b"#!/bin/sh\necho hi\n");
        let asset = PinnedAsset {
            platform: "linux-x86_64",
            file_name: "Godot_test_linux.x86_64.zip",
            sha512: leak(sha512_hex(&zip)),
        };
        let len = zip.len() as u64;
        let url = serve_once(zip);
        let mut progress = Vec::new();
        let exe = install_from(&test_client(), app_data.path(), &url, &asset, &mut |p| {
            progress.push(p)
        })
        .unwrap();

        assert_eq!(
            exe,
            managed_dir(app_data.path()).join("Godot_test_linux.x86_64")
        );
        assert_eq!(fs::read(&exe).unwrap(), b"#!/bin/sh\necho hi\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&exe).unwrap().permissions().mode() & 0o111,
                0o111
            );
        }
        // The zip itself isn't kept, and the staging folder is gone.
        let entries: Vec<_> = fs::read_dir(app_data.path().join("godot"))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(entries, vec![PINNED_VERSION.to_string()]);
        assert_eq!(
            fs::read_dir(managed_dir(app_data.path())).unwrap().count(),
            1
        );

        let phases: Vec<&str> = progress.iter().map(|p| p.phase.as_str()).collect();
        assert_eq!(phases.first(), Some(&PHASE_DOWNLOADING));
        assert!(phases.ends_with(&[
            PHASE_DOWNLOADING,
            PHASE_VERIFYING,
            PHASE_EXTRACTING,
            PHASE_DONE
        ]));
        let last = progress.last().unwrap();
        assert_eq!((last.downloaded_bytes, last.total_bytes), (len, Some(len)));
    }

    #[test]
    fn a_zip_without_the_expected_executable_is_rejected() {
        let app_data = tempfile::tempdir().unwrap();
        let zip = linux_like_zip("something_else", b"x");
        let asset = PinnedAsset {
            platform: "linux-x86_64",
            file_name: "Godot_test_linux.x86_64.zip",
            sha512: leak(sha512_hex(&zip)),
        };
        let url = serve_once(zip);
        let err =
            install_from(&test_client(), app_data.path(), &url, &asset, &mut |_| {}).unwrap_err();
        assert!(err.to_string().contains("expected executable"), "{err}");
        assert!(!managed_dir(app_data.path()).exists());
    }

    #[test]
    fn an_http_error_is_reported_and_installs_nothing() {
        let app_data = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap() > 0 && line != "\r\n" {
                line.clear();
            }
            stream
                .write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
        });
        let asset = &PINNED_ASSETS[0];
        let err = install_from(
            &test_client(),
            app_data.path(),
            &format!("http://{addr}/missing.zip"),
            asset,
            &mut |_| {},
        )
        .unwrap_err();
        // Plain Display (what the UI shows) must carry the cause itself.
        let msg = err.to_string();
        assert!(msg.contains("couldn't download Godot from"), "{msg}");
        assert!(msg.contains("404"), "{msg}");
        assert!(!managed_dir(app_data.path()).exists());
    }

    #[test]
    fn a_refused_connection_names_the_real_cause() {
        let app_data = tempfile::tempdir().unwrap();
        // Bind then drop, so nothing is listening on the port.
        let addr = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        let err = install_from(
            &test_client(),
            app_data.path(),
            &format!("http://{addr}/godot.zip"),
            &PINNED_ASSETS[0],
            &mut |_| {},
        )
        .unwrap_err();
        let msg = err.to_string().to_lowercase();
        assert!(msg.contains("couldn't download godot from"), "{msg}");
        assert!(msg.contains("refused"), "cause missing from: {msg}");
    }

    #[test]
    fn filesystem_failures_say_what_was_being_done() {
        // App data is a file, so the godot folder can't be created under it.
        let dir = tempfile::tempdir().unwrap();
        let app_data = dir.path().join("not-a-dir");
        fs::write(&app_data, "x").unwrap();
        let err = install_from(
            &test_client(),
            &app_data,
            "http://127.0.0.1:9/unused.zip",
            &PINNED_ASSETS[0],
            &mut |_| {},
        )
        .unwrap_err();
        let msg = err.to_string();
        assert!(msg.starts_with("couldn't create "), "{msg}");
        assert!(msg.contains(&app_data.display().to_string()), "{msg}");
    }

    /// Serves `body` to every request, slowly (so concurrent installs
    /// overlap), counting requests.
    fn serve_slowly(body: Vec<u8>, requests: Arc<AtomicUsize>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let body = body.clone();
                let requests = requests.clone();
                thread::spawn(move || {
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut line = String::new();
                    while reader.read_line(&mut line).unwrap_or(0) > 0 && line != "\r\n" {
                        line.clear();
                    }
                    requests.fetch_add(1, Ordering::SeqCst);
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    for chunk in body.chunks(body.len().div_ceil(10).max(1)) {
                        let _ = stream.write_all(chunk);
                        let _ = stream.flush();
                        thread::sleep(Duration::from_millis(50));
                    }
                });
            }
        });
        format!("http://{addr}/{PINNED_VERSION}/godot.zip")
    }

    /// Two installs at once: one downloads, the other waits and returns the
    /// finished install. Before the install lock they shared one staging
    /// folder and corrupted each other. Unix-only: the fake "Godot" is a
    /// shell script answering `--version`.
    #[cfg(unix)]
    #[test]
    fn concurrent_installs_both_succeed_with_one_download() {
        let app_data = tempfile::tempdir().unwrap();
        let zip = linux_like_zip(
            "Godot_concurrent_linux.x86_64",
            b"#!/bin/sh\necho 4.7.2.fake.concurrent\n",
        );
        let asset: &'static PinnedAsset = Box::leak(Box::new(PinnedAsset {
            platform: "linux-x86_64",
            file_name: "Godot_concurrent_linux.x86_64.zip",
            sha512: leak(sha512_hex(&zip)),
        }));
        let requests = Arc::new(AtomicUsize::new(0));
        let url = serve_slowly(zip, requests.clone());

        let handles: Vec<_> = (0..2)
            .map(|_| {
                let app_data = app_data.path().to_path_buf();
                let url = url.clone();
                thread::spawn(move || {
                    let mut phases = Vec::new();
                    let exe = install_locked(&test_client(), &app_data, &url, asset, &mut |p| {
                        phases.push(p.phase)
                    });
                    (exe, phases)
                })
            })
            .collect();
        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        let expected = managed_dir(app_data.path()).join("Godot_concurrent_linux.x86_64");
        for (exe, _) in &results {
            assert_eq!(exe.as_ref().unwrap(), &expected);
        }
        assert_eq!(requests.load(Ordering::SeqCst), 1);
        // The one that waited reported only "done".
        let waited = results
            .iter()
            .filter(|(_, phases)| phases == &[PHASE_DONE.to_string()])
            .count();
        assert_eq!(waited, 1);
        assert_eq!(
            locate::read_version(&expected).as_deref(),
            Some("4.7.2.fake.concurrent")
        );
        // Only the finished install is left; no staging folders.
        let entries: Vec<_> = fs::read_dir(app_data.path().join("godot"))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(entries, vec![PINNED_VERSION.to_string()]);
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    #[test]
    #[ignore = "needs network access to github.com (~60MB); run with --ignored"]
    fn downloads_verifies_and_runs_the_real_pinned_godot() {
        let app_data = tempfile::tempdir().unwrap();
        let mut last = None;
        let exe = install(app_data.path(), &mut |p| last = Some(p)).unwrap();
        let last = last.unwrap();
        assert_eq!(last.phase, PHASE_DONE);
        assert!(
            last.total_bytes
                .is_some_and(|t| t == last.downloaded_bytes && t > 10_000_000)
        );

        let status = locate::status_with(app_data.path(), None);
        assert!(status.installed && status.managed);
        assert_eq!(status.path.as_deref(), Some(exe.as_path()));
        let recorded = fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/godot/version.txt"),
        )
        .unwrap();
        assert_eq!(status.version.as_deref(), Some(recorded.trim()));

        // A second install finds the working one and doesn't download again.
        let mut phases = Vec::new();
        assert_eq!(
            install(app_data.path(), &mut |p| phases.push(p.phase)).unwrap(),
            exe
        );
        assert_eq!(phases, vec![PHASE_DONE.to_string()]);
    }
}
