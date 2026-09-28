//! "Connect your AI" (spec §8.2): finds which agent CLIs the user has,
//! whether they're signed in, how to install or sign in to each, and which
//! one to recommend. InfinaBox never holds the credentials — the CLIs do.
//!
//! Everything here asks the real machine. Programs are looked up on the
//! login-shell `PATH` (`agent/path.rs`, so a GUI app finds the same CLI the
//! user's Terminal does) and every probe runs with a timeout, so a hung CLI
//! can't hang the connect screen. Checked against the real CLIs (Claude
//! Code 2.1.283, `codex-cli` 0.157.1):
//! - `claude --version` prints `2.1.283 (Claude Code)`; `codex --version`
//!   prints `codex-cli 0.157.1`.
//! - `claude auth status --json` prints an object with a boolean
//!   `loggedIn` (JSON is also its default output).
//! - `codex login status` exits 0 when signed in; signed out it prints
//!   `Not logged in` to stderr and exits 1.
//! - `claude auth login` and `codex login` are the CLIs' own sign-in
//!   commands (both listed in their `--help`).
//!
//! Install commands are the vendors' own documented ones: Anthropic's
//! native installer (`https://claude.ai/install.sh` / `install.ps1`, which
//! redirect to `downloads.claude.ai`'s bootstrap scripts), and for Codex the
//! npm package `@openai/codex` or the Homebrew cask `codex`, both listed in
//! the `openai/codex` README.

use std::ffi::OsStr;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};

use crate::agent::path::{find_on_path, login_shell_path, refresh_login_shell_path};

/// The agent CLIs InfinaBox can drive.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProviderId {
    #[serde(rename = "claude-code")]
    ClaudeCode,
    #[serde(rename = "codex")]
    Codex,
}

impl ProviderId {
    pub const ALL: [ProviderId; 2] = [ProviderId::ClaudeCode, ProviderId::Codex];

    /// The name written into chat thread headers (`ThreadSummary::provider`).
    pub fn as_str(self) -> &'static str {
        match self {
            ProviderId::ClaudeCode => "claude-code",
            ProviderId::Codex => "codex",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.as_str() == s)
    }

    /// What the CLI is called on `PATH`.
    fn program(self) -> &'static str {
        match self {
            ProviderId::ClaudeCode => "claude",
            ProviderId::Codex => "codex",
        }
    }

    fn display_name(self) -> &'static str {
        match self {
            ProviderId::ClaudeCode => "Claude Code",
            ProviderId::Codex => "Codex",
        }
    }

    /// Who it suits, in plain words. Plan names only — no prices, which
    /// change and aren't ours to state.
    fn blurb(self) -> &'static str {
        match self {
            ProviderId::ClaudeCode => {
                "Anthropic's coding assistant. Use it if you have a Claude Pro or Max plan, \
                 or an Anthropic Console account."
            }
            ProviderId::Codex => {
                "OpenAI's coding assistant. Use it if you have a ChatGPT Plus, Pro or Business \
                 plan, or an OpenAI account."
            }
        }
    }

    /// The vendor's official documentation.
    fn docs_url(self) -> &'static str {
        match self {
            ProviderId::ClaudeCode => "https://code.claude.com/docs/en/overview",
            ProviderId::Codex => "https://developers.openai.com/codex",
        }
    }

    /// The CLI's own sign-in subcommand.
    fn login_args(self) -> &'static [&'static str] {
        match self {
            ProviderId::ClaudeCode => &["auth", "login"],
            ProviderId::Codex => &["login"],
        }
    }

    fn login_command(self) -> String {
        std::iter::once(self.program())
            .chain(self.login_args().iter().copied())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Everything the connect screen shows about one provider. Every field is
/// detected from the real machine or is fixed, factual copy.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct ProviderInfo {
    pub id: ProviderId,
    pub name: String,
    pub installed: bool,
    pub version: Option<String>,
    /// From the CLI's own status command; None when it can't tell.
    pub logged_in: Option<bool>,
    /// Who it suits and what it needs, in plain words (no invented prices).
    pub blurb: String,
    /// The exact command the installer runs, shown before it runs.
    pub install_command: Option<String>,
    /// Why it can't be installed from here (e.g. needs Node.js), if so.
    pub install_blocker: Option<String>,
    pub login_command: String,
    pub docs_url: String,
}

/// How long each probe (`--version`, the sign-in status) gets before
/// detection gives up on it and reports "don't know".
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// After a probe exits, how long its output pipes get to close. A CLI that
/// leaves a background process holding them must not stall detection.
const PIPE_GRACE: Duration = Duration::from_millis(500);

/// More probe output than this isn't kept.
const MAX_PROBE_OUTPUT: usize = 64 * 1024;

/// Anthropic's native installer, as its docs give it.
const CLAUDE_INSTALL_SH: &str = "curl -fsSL https://claude.ai/install.sh | bash";
const CLAUDE_INSTALL_PS1: &str = "irm https://claude.ai/install.ps1 | iex";

const CODEX_NPM_PACKAGE: &str = "@openai/codex";

const NODE_DOWNLOAD_URL: &str = "https://nodejs.org/en/download";

/// Detects every provider, all at once (each probe has its own timeout, so
/// the slowest CLI sets the pace, not the sum of them).
pub fn detect_all() -> Vec<ProviderInfo> {
    let path = login_shell_path();
    std::thread::scope(|s| {
        let handles: Vec<_> = ProviderId::ALL
            .into_iter()
            .map(|id| s.spawn(move || detect_in(id, path, Os::current(), PROBE_TIMEOUT)))
            .collect();
        handles
            .into_iter()
            .zip(ProviderId::ALL)
            .map(|(handle, id)| handle.join().unwrap_or_else(|_| not_detected(id)))
            .collect()
    })
}

pub fn detect(id: ProviderId) -> ProviderInfo {
    detect_in(id, login_shell_path(), Os::current(), PROBE_TIMEOUT)
}

/// Program and arguments that run the installer inside a terminal. An
/// error is the plain-words `install_blocker`.
///
/// When something the installer needs is missing, the login shell is asked
/// for its `PATH` again before giving up: the person may have just
/// installed it (e.g. Node.js) and come back.
pub fn install_invocation(id: ProviderId) -> Result<(String, Vec<String>)> {
    let plan = install_plan(id, Os::current(), login_shell_path())
        .or_else(|_| install_plan(id, Os::current(), refresh_login_shell_path()));
    plan.map(|p| (p.program, p.args))
        .map_err(|blocker| anyhow!(blocker))
}

/// Program and arguments that run the CLI's own sign-in inside a terminal.
/// Asks the login shell for its `PATH` again if the CLI isn't on the cached
/// one — it has usually just been installed.
pub fn login_invocation(id: ProviderId) -> Result<(String, Vec<String>)> {
    login_invocation_in(id, Os::current(), login_shell_path())
        .or_else(|_| login_invocation_in(id, Os::current(), refresh_login_shell_path()))
}

/// The first installed and signed-in provider, else the first installed,
/// else Claude Code.
pub fn recommend(infos: &[ProviderInfo]) -> ProviderId {
    infos
        .iter()
        .find(|p| p.installed && p.logged_in == Some(true))
        .or_else(|| infos.iter().find(|p| p.installed))
        .map_or(ProviderId::ClaudeCode, |p| p.id)
}

/// The operating system the install and sign-in commands are built for —
/// a parameter (not a `cfg!`) so each one's commands are testable anywhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Os {
    Mac,
    Linux,
    Windows,
}

impl Os {
    fn current() -> Self {
        if cfg!(windows) {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::Mac
        } else {
            Os::Linux
        }
    }
}

/// `detect`, with the `PATH` to search, the OS and the probe timeout given.
fn detect_in(id: ProviderId, path_var: &OsStr, os: Os, timeout: Duration) -> ProviderInfo {
    let program = find_program(id.program(), os, path_var);
    let (version, logged_in) = match &program {
        // Both probes at once: each can take up to `timeout`.
        Some(program) => std::thread::scope(|s| {
            let version = s.spawn(|| read_version(program, path_var, timeout));
            let logged_in = read_logged_in(id, program, path_var, timeout);
            (version.join().ok().flatten(), logged_in)
        }),
        None => (None, None),
    };
    let plan = install_plan(id, os, path_var);
    ProviderInfo {
        installed: program.is_some(),
        version,
        logged_in,
        install_command: plan.as_ref().ok().map(|p| p.display.clone()),
        install_blocker: plan.err(),
        ..not_detected(id)
    }
}

/// The fixed facts about a provider, with nothing detected.
fn not_detected(id: ProviderId) -> ProviderInfo {
    ProviderInfo {
        id,
        name: id.display_name().into(),
        installed: false,
        version: None,
        logged_in: None,
        blurb: id.blurb().into(),
        install_command: None,
        install_blocker: None,
        login_command: id.login_command(),
        docs_url: id.docs_url().into(),
    }
}

/// `name` on `path_var`. On Windows an `.exe` or an npm `.cmd` shim, never
/// the extensionless shell script npm also puts next to them (which
/// Windows can't run).
fn find_program(name: &str, os: Os, path_var: &OsStr) -> Option<PathBuf> {
    if os == Os::Windows {
        return find_on_path(&format!("{name}.exe"), path_var)
            .or_else(|| find_on_path(&format!("{name}.cmd"), path_var));
    }
    find_on_path(name, path_var)
}

/// `<program> --version` → the version number, e.g. `2.1.283 (Claude
/// Code)` → `2.1.283`, `codex-cli 0.157.1` → `0.157.1`. `None` if it
/// fails, times out, or prints no version number.
fn read_version(program: &Path, path_var: &OsStr, timeout: Duration) -> Option<String> {
    let out = run_probe(program, &["--version"], path_var, timeout)?;
    if !out.success {
        return None;
    }
    parse_version(&out.stdout)
}

fn parse_version(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .map(|word| {
            word.trim_start_matches('v')
                .trim_end_matches(|c: char| !c.is_ascii_alphanumeric())
        })
        .find(|word| {
            word.starts_with(|c: char| c.is_ascii_digit())
                && word.contains('.')
                && word
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
        })
        .map(str::to_string)
}

/// Whether the CLI says it's signed in; `None` when it can't tell (it
/// failed some other way, printed something unexpected, or timed out).
fn read_logged_in(
    id: ProviderId,
    program: &Path,
    path_var: &OsStr,
    timeout: Duration,
) -> Option<bool> {
    match id {
        ProviderId::ClaudeCode => {
            // Parsed whatever the exit code: signed out is still a valid
            // answer, not a failure.
            let out = run_probe(program, &["auth", "status", "--json"], path_var, timeout)?;
            parse_claude_auth_status(&out.stdout)
        }
        ProviderId::Codex => {
            let out = run_probe(program, &["login", "status"], path_var, timeout)?;
            if out.success {
                Some(true)
            } else if format!("{}\n{}", out.stdout, out.stderr).contains("Not logged in") {
                Some(false)
            } else {
                None
            }
        }
    }
}

/// `loggedIn` from `claude auth status --json`'s object.
fn parse_claude_auth_status(stdout: &str) -> Option<bool> {
    let start = stdout.find('{')?;
    let end = stdout.rfind('}')?;
    let value: serde_json::Value = serde_json::from_str(stdout.get(start..=end)?).ok()?;
    value.get("loggedIn")?.as_bool()
}

/// What a finished probe printed.
struct ProbeOutput {
    success: bool,
    stdout: String,
    stderr: String,
}

/// Runs `program args` with `PATH` set to `path_var` (so a CLI that is a
/// Node script finds `node` the way it would in Terminal) and no stdin.
/// `None` if it can't start or doesn't finish within `timeout` (it's
/// killed then).
///
/// Both pipes are read on helper threads that send chunks over a channel,
/// like `agent/path.rs`: once the process exits we wait at most
/// `PIPE_GRACE` for them to close, since a background process the CLI left
/// behind can hold them open indefinitely. Those threads are never joined;
/// they end when the pipes close.
fn run_probe(
    program: &Path,
    args: &[&str],
    path_var: &OsStr,
    timeout: Duration,
) -> Option<ProbeOutput> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .env("PATH", path_var)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        // CREATE_NO_WINDOW: no console window flashing up per probe.
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let mut child = cmd.spawn().ok()?;

    let (tx, rx) = mpsc::channel::<(bool, Option<Vec<u8>>)>();
    for (is_stdout, pipe) in [
        (
            true,
            child
                .stdout
                .take()
                .map(|p| Box::new(p) as Box<dyn Read + Send>),
        ),
        (
            false,
            child
                .stderr
                .take()
                .map(|p| Box::new(p) as Box<dyn Read + Send>),
        ),
    ] {
        let Some(mut pipe) = pipe else { continue };
        let tx = tx.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match pipe.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if tx.send((is_stdout, Some(buf[..n].to_vec()))).is_err() {
                            return;
                        }
                    }
                }
            }
            let _ = tx.send((is_stdout, None));
        });
    }
    drop(tx);

    let deadline = Instant::now() + timeout;
    let mut out = Captured::default();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => {
                // Keep the pipes drained while waiting, so a chatty CLI
                // can't block on a full pipe.
                if let Ok(chunk) = rx.recv_timeout(Duration::from_millis(20)) {
                    out.add(chunk);
                }
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    let grace_until = Instant::now() + PIPE_GRACE;
    while out.open_pipes > 0 {
        let Some(left) = grace_until.checked_duration_since(Instant::now()) else {
            break;
        };
        match rx.recv_timeout(left) {
            Ok(chunk) => out.add(chunk),
            Err(_) => break,
        }
    }
    Some(ProbeOutput {
        success: status.success(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

/// A probe's output as it arrives from `run_probe`'s reader threads: a
/// chunk from stdout (`true`) or stderr, or `None` when that pipe closed.
struct Captured {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    open_pipes: usize,
}

impl Default for Captured {
    fn default() -> Self {
        Self {
            stdout: Vec::new(),
            stderr: Vec::new(),
            open_pipes: 2,
        }
    }
}

impl Captured {
    fn add(&mut self, (is_stdout, chunk): (bool, Option<Vec<u8>>)) {
        let target = if is_stdout {
            &mut self.stdout
        } else {
            &mut self.stderr
        };
        match chunk {
            Some(bytes) if target.len() < MAX_PROBE_OUTPUT => target.extend_from_slice(&bytes),
            Some(_) => {}
            None => self.open_pipes -= 1,
        }
    }
}

/// How to install a provider here: what runs, and the command the person
/// sees before it does.
#[derive(Debug, PartialEq)]
struct InstallPlan {
    program: String,
    args: Vec<String>,
    /// The vendor's own command, exactly as it will run.
    display: String,
}

/// The install plan for `id` on `os`, or why there isn't one (the
/// `install_blocker`, in plain words).
fn install_plan(
    id: ProviderId,
    os: Os,
    path_var: &OsStr,
) -> std::result::Result<InstallPlan, String> {
    match (id, os) {
        (ProviderId::ClaudeCode, Os::Windows) => {
            let Some(powershell) = find_program("powershell", os, path_var) else {
                return Err(
                    "Installing Claude Code needs Windows PowerShell, which InfinaBox \
                            couldn't find on this computer."
                        .into(),
                );
            };
            Ok(InstallPlan {
                program: path_string(&powershell),
                args: [
                    "-NoProfile",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-Command",
                    CLAUDE_INSTALL_PS1,
                ]
                .map(String::from)
                .into(),
                display: CLAUDE_INSTALL_PS1.into(),
            })
        }
        (ProviderId::ClaudeCode, _) => {
            let bash = find_program("bash", os, path_var);
            let missing: Vec<&str> = [
                ("curl", find_program("curl", os, path_var)),
                ("bash", bash.clone()),
            ]
            .into_iter()
            .filter(|(_, found)| found.is_none())
            .map(|(name, _)| name)
            .collect();
            let Some(bash) = bash.filter(|_| missing.is_empty()) else {
                return Err(format!(
                    "Installing Claude Code needs {}, which InfinaBox couldn't find on this computer.",
                    missing
                        .iter()
                        .map(|m| format!("the `{m}` command"))
                        .collect::<Vec<_>>()
                        .join(" and ")
                ));
            };
            // A login shell, so the installer sees the same environment
            // the person's own Terminal would.
            Ok(InstallPlan {
                program: path_string(&bash),
                args: vec!["-lc".into(), CLAUDE_INSTALL_SH.into()],
                display: CLAUDE_INSTALL_SH.into(),
            })
        }
        (ProviderId::Codex, _) => {
            if os == Os::Mac
                && let Some(brew) = find_program("brew", os, path_var)
            {
                let args = ["install", "--cask", "codex"];
                let (program, args) = with_path(&brew, &args, os, path_var);
                return Ok(InstallPlan {
                    program,
                    args,
                    display: "brew install --cask codex".into(),
                });
            }
            if let Some(npm) = find_program("npm", os, path_var) {
                let args = ["install", "-g", CODEX_NPM_PACKAGE];
                let (program, args) = with_path(&npm, &args, os, path_var);
                return Ok(InstallPlan {
                    program,
                    args,
                    display: format!("npm install -g {CODEX_NPM_PACKAGE}"),
                });
            }
            Err(format!(
                "Codex installs with Node.js, which isn't on this computer yet. Install Node.js \
                 (the LTS version) from {NODE_DOWNLOAD_URL}, then press Install again."
            ))
        }
    }
}

/// `login_invocation`, with the OS and `PATH` given.
fn login_invocation_in(id: ProviderId, os: Os, path_var: &OsStr) -> Result<(String, Vec<String>)> {
    let Some(program) = find_program(id.program(), os, path_var) else {
        bail!(
            "{} isn't installed on this computer yet. Install it first, then sign in.",
            id.display_name()
        );
    };
    Ok(with_path(&program, id.login_args(), os, path_var))
}

/// Runs `program args` with `PATH` set to the login-shell `PATH` the
/// program was found on, whatever environment the terminal starts with: on
/// macOS/Linux through `/usr/bin/env PATH=… program args`. That matters
/// for Node-based CLIs (`npm`, an npm-installed `codex`), which find `node`
/// through `PATH`, and is the same `PATH` detection just used — without
/// re-running the person's shell profile. On Windows there's no separate
/// login `PATH`; an npm `.cmd` shim runs through `cmd /C`.
fn with_path(program: &Path, args: &[&str], os: Os, path_var: &OsStr) -> (String, Vec<String>) {
    let args = args.iter().map(|a| a.to_string());
    if os == Os::Windows {
        let is_cmd = program
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("cmd"));
        if is_cmd {
            let all = ["/C".to_string(), path_string(program)]
                .into_iter()
                .chain(args);
            return ("cmd".into(), all.collect());
        }
        return (path_string(program), args.collect());
    }
    let all = [
        format!("PATH={}", path_var.to_string_lossy()),
        path_string(program),
    ]
    .into_iter()
    .chain(args);
    ("/usr/bin/env".into(), all.collect())
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    fn info(id: ProviderId, installed: bool, logged_in: Option<bool>) -> ProviderInfo {
        ProviderInfo {
            installed,
            logged_in,
            ..not_detected(id)
        }
    }

    #[test]
    fn recommends_signed_in_then_installed_then_claude_code() {
        use ProviderId::*;
        assert_eq!(recommend(&[]), ClaudeCode);
        assert_eq!(
            recommend(&[info(ClaudeCode, false, None), info(Codex, false, None)]),
            ClaudeCode
        );
        assert_eq!(
            recommend(&[info(ClaudeCode, false, None), info(Codex, true, None)]),
            Codex
        );
        assert_eq!(
            recommend(&[
                info(ClaudeCode, true, Some(false)),
                info(Codex, true, Some(true))
            ]),
            Codex
        );
        assert_eq!(
            recommend(&[
                info(ClaudeCode, true, Some(true)),
                info(Codex, true, Some(true))
            ]),
            ClaudeCode
        );
        assert_eq!(
            recommend(&[info(ClaudeCode, true, None), info(Codex, true, Some(false))]),
            ClaudeCode
        );
    }

    #[test]
    fn parses_version_numbers_out_of_real_version_lines() {
        assert_eq!(
            parse_version("2.1.283 (Claude Code)\n").as_deref(),
            Some("2.1.283")
        );
        assert_eq!(
            parse_version("codex-cli 0.157.1\n").as_deref(),
            Some("0.157.1")
        );
        assert_eq!(
            parse_version("tool v1.2.3-beta.1,").as_deref(),
            Some("1.2.3-beta.1")
        );
        assert_eq!(parse_version("no version here"), None);
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn parses_claude_auth_status() {
        // The shape the real `claude auth status --json` prints.
        let real = r#"{
  "loggedIn": true,
  "authMethod": "oauth_token",
  "apiProvider": "firstParty"
}"#;
        assert_eq!(parse_claude_auth_status(real), Some(true));
        assert_eq!(
            parse_claude_auth_status(r#"{"loggedIn": false}"#),
            Some(false)
        );
        assert_eq!(parse_claude_auth_status("Not logged in"), None);
        assert_eq!(parse_claude_auth_status(r#"{"other": 1}"#), None);
    }

    #[test]
    fn provider_ids_round_trip() {
        for id in ProviderId::ALL {
            assert_eq!(ProviderId::parse(id.as_str()), Some(id));
            let json = serde_json::to_string(&id).unwrap();
            assert_eq!(json, format!("\"{}\"", id.as_str()));
        }
        assert_eq!(ProviderId::parse("other"), None);
    }

    #[test]
    fn nothing_installed_is_reported_honestly() {
        let dir = tempfile::tempdir().unwrap();
        for id in ProviderId::ALL {
            let info = detect_in(
                id,
                dir.path().as_os_str(),
                Os::Linux,
                Duration::from_secs(2),
            );
            assert!(!info.installed);
            assert_eq!(info.version, None);
            assert_eq!(info.logged_in, None);
            assert!(!info.blurb.is_empty());
            assert!(info.docs_url.starts_with("https://"));
        }
    }

    /// Fake executables in a temp dir, and a `PATH` of that dir followed by
    /// the system directories (the fakes use `sleep`).
    #[cfg(unix)]
    struct FakeBin {
        dir: tempfile::TempDir,
    }

    #[cfg(unix)]
    impl FakeBin {
        fn new() -> Self {
            Self {
                dir: tempfile::tempdir().unwrap(),
            }
        }

        fn add(&self, name: &str, body: &str) -> PathBuf {
            use std::os::unix::fs::PermissionsExt;
            let path = self.dir.path().join(name);
            std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        }

        /// Only the fakes.
        fn path_only(&self) -> OsString {
            self.dir.path().as_os_str().to_owned()
        }

        /// The fakes, then the system directories.
        fn path_with_system(&self) -> OsString {
            std::env::join_paths([self.dir.path(), Path::new("/usr/bin"), Path::new("/bin")])
                .unwrap()
        }
    }

    #[cfg(unix)]
    const FAKE_CLAUDE: &str = r#"case "$1 $2" in
  "--version ") echo "2.1.283 (Claude Code)" ;;
  "auth status") echo "{\"loggedIn\": $LOGGED_IN, \"authMethod\": \"claude.ai\"}"; [ "$LOGGED_IN" = true ] || exit 1 ;;
  *) exit 2 ;;
esac"#;

    #[cfg(unix)]
    #[test]
    fn detects_a_signed_in_claude() {
        let bin = FakeBin::new();
        bin.add("claude", &format!("LOGGED_IN=true\n{FAKE_CLAUDE}"));
        let info = detect_in(
            ProviderId::ClaudeCode,
            &bin.path_with_system(),
            Os::Linux,
            Duration::from_secs(5),
        );
        assert!(info.installed);
        assert_eq!(info.version.as_deref(), Some("2.1.283"));
        assert_eq!(info.logged_in, Some(true));
        assert_eq!(info.login_command, "claude auth login");
    }

    #[cfg(unix)]
    #[test]
    fn detects_a_signed_out_claude() {
        let bin = FakeBin::new();
        bin.add("claude", &format!("LOGGED_IN=false\n{FAKE_CLAUDE}"));
        let info = detect_in(
            ProviderId::ClaudeCode,
            &bin.path_with_system(),
            Os::Linux,
            Duration::from_secs(5),
        );
        assert!(info.installed);
        assert_eq!(info.logged_in, Some(false));
    }

    #[cfg(unix)]
    #[test]
    fn unexpected_auth_output_is_unknown_not_guessed() {
        let bin = FakeBin::new();
        bin.add(
            "claude",
            "[ \"$1\" = --version ] && { echo '2.1.283 (Claude Code)'; exit 0; }\necho 'error: unknown command' >&2; exit 1",
        );
        let info = detect_in(
            ProviderId::ClaudeCode,
            &bin.path_with_system(),
            Os::Linux,
            Duration::from_secs(5),
        );
        assert!(info.installed);
        assert_eq!(info.version.as_deref(), Some("2.1.283"));
        assert_eq!(info.logged_in, None);
    }

    #[cfg(unix)]
    fn fake_codex(login_status: &str) -> FakeBin {
        let bin = FakeBin::new();
        bin.add(
            "codex",
            &format!(
                "case \"$1 $2\" in\n  \"--version \") echo 'codex-cli 0.157.1' ;;\n  \"login status\") {login_status} ;;\n  *) exit 2 ;;\nesac"
            ),
        );
        bin
    }

    #[cfg(unix)]
    #[test]
    fn detects_codex_sign_in_from_the_exit_code() {
        let cases = [
            ("echo 'Logged in using ChatGPT'", Some(true)),
            // What the real CLI does signed out.
            ("echo 'Not logged in' >&2; exit 1", Some(false)),
            ("echo 'something else went wrong' >&2; exit 1", None),
        ];
        for (status, expected) in cases {
            let bin = fake_codex(status);
            let info = detect_in(
                ProviderId::Codex,
                &bin.path_with_system(),
                Os::Linux,
                Duration::from_secs(5),
            );
            assert!(info.installed);
            assert_eq!(info.version.as_deref(), Some("0.157.1"));
            assert_eq!(info.logged_in, expected, "{status}");
            assert_eq!(info.login_command, "codex login");
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_hung_cli_times_out_instead_of_hanging_detection() {
        let bin = FakeBin::new();
        // Not `exec`: the `sleep` outlives the killed script and keeps the
        // pipes open, the worst case.
        bin.add("claude", "sleep 30");
        bin.add("codex", "sleep 30");
        for id in ProviderId::ALL {
            let started = Instant::now();
            let info = detect_in(
                id,
                &bin.path_with_system(),
                Os::Linux,
                Duration::from_millis(500),
            );
            assert!(info.installed);
            assert_eq!(info.version, None);
            assert_eq!(info.logged_in, None);
            assert!(
                started.elapsed() < Duration::from_secs(3),
                "{:?}",
                started.elapsed()
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_background_process_holding_the_pipes_does_not_stall_a_probe() {
        let bin = FakeBin::new();
        bin.add("codex", "sleep 30 &\necho 'codex-cli 0.157.1'");
        let started = Instant::now();
        let out = run_probe(
            &bin.dir.path().join("codex"),
            &["--version"],
            &bin.path_with_system(),
            Duration::from_secs(10),
        )
        .expect("exits quickly");
        assert!(out.success);
        assert_eq!(parse_version(&out.stdout).as_deref(), Some("0.157.1"));
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "{:?}",
            started.elapsed()
        );
    }

    #[cfg(unix)]
    #[test]
    fn claude_installs_with_the_native_installer_in_a_login_shell() {
        let bin = FakeBin::new();
        let bash = bin.add("bash", "exit 0");
        bin.add("curl", "exit 0");
        for os in [Os::Linux, Os::Mac] {
            let plan = install_plan(ProviderId::ClaudeCode, os, &bin.path_only()).unwrap();
            assert_eq!(plan.program, bash.to_string_lossy());
            assert_eq!(
                plan.args,
                ["-lc", "curl -fsSL https://claude.ai/install.sh | bash"]
            );
            assert_eq!(
                plan.display,
                "curl -fsSL https://claude.ai/install.sh | bash"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn claude_install_is_blocked_without_curl() {
        let bin = FakeBin::new();
        bin.add("bash", "exit 0");
        let blocker =
            install_plan(ProviderId::ClaudeCode, Os::Linux, &bin.path_only()).unwrap_err();
        assert!(blocker.contains("`curl`"), "{blocker}");
        assert!(!blocker.contains("`bash`"), "{blocker}");

        let empty = FakeBin::new();
        let blocker =
            install_plan(ProviderId::ClaudeCode, Os::Linux, &empty.path_only()).unwrap_err();
        assert!(
            blocker.contains("`curl`") && blocker.contains("`bash`"),
            "{blocker}"
        );

        let info = detect_in(
            ProviderId::ClaudeCode,
            &empty.path_only(),
            Os::Linux,
            Duration::from_secs(1),
        );
        assert_eq!(info.install_command, None);
        assert_eq!(info.install_blocker, Some(blocker));
    }

    #[cfg(unix)]
    #[test]
    fn claude_installs_with_powershell_on_windows() {
        let bin = FakeBin::new();
        // `find_on_path` only adds `.exe` itself when built for Windows.
        let ps = bin.add("powershell.exe", "exit 0");
        let plan = install_plan(ProviderId::ClaudeCode, Os::Windows, &bin.path_only()).unwrap();
        assert_eq!(plan.program, ps.to_string_lossy());
        assert_eq!(
            plan.args,
            [
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                "irm https://claude.ai/install.ps1 | iex"
            ]
        );
        assert_eq!(plan.display, "irm https://claude.ai/install.ps1 | iex");

        let empty = FakeBin::new();
        let blocker =
            install_plan(ProviderId::ClaudeCode, Os::Windows, &empty.path_only()).unwrap_err();
        assert!(blocker.contains("PowerShell"), "{blocker}");
    }

    #[cfg(unix)]
    #[test]
    fn codex_installs_with_homebrew_on_macos_else_npm() {
        let bin = FakeBin::new();
        let brew = bin.add("brew", "exit 0");
        let npm = bin.add("npm", "exit 0");
        let path = bin.path_only();
        let path_arg = format!("PATH={}", path.to_string_lossy());

        let mac = install_plan(ProviderId::Codex, Os::Mac, &path).unwrap();
        assert_eq!(mac.program, "/usr/bin/env");
        assert_eq!(
            mac.args,
            [
                path_arg.as_str(),
                &brew.to_string_lossy(),
                "install",
                "--cask",
                "codex"
            ]
        );
        assert_eq!(mac.display, "brew install --cask codex");

        // Homebrew on Linux isn't used; npm is.
        let linux = install_plan(ProviderId::Codex, Os::Linux, &path).unwrap();
        assert_eq!(linux.program, "/usr/bin/env");
        assert_eq!(
            linux.args,
            [
                path_arg.as_str(),
                &npm.to_string_lossy(),
                "install",
                "-g",
                "@openai/codex"
            ]
        );
        assert_eq!(linux.display, "npm install -g @openai/codex");

        let npm_only = FakeBin::new();
        npm_only.add("npm", "exit 0");
        let mac = install_plan(ProviderId::Codex, Os::Mac, &npm_only.path_only()).unwrap();
        assert_eq!(mac.display, "npm install -g @openai/codex");
    }

    #[cfg(unix)]
    #[test]
    fn codex_install_without_node_says_to_get_node_first() {
        let empty = FakeBin::new();
        for os in [Os::Mac, Os::Linux, Os::Windows] {
            let blocker = install_plan(ProviderId::Codex, os, &empty.path_only()).unwrap_err();
            assert!(blocker.contains("Node.js"), "{blocker}");
            assert!(blocker.contains("https://nodejs.org"), "{blocker}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn codex_installs_through_the_npm_shim_on_windows() {
        let bin = FakeBin::new();
        // npm puts an extensionless shell script next to its `.cmd` shim;
        // Windows can only run the shim.
        bin.add("npm", "exit 0");
        let shim = bin.add("npm.cmd", "exit 0");
        let plan = install_plan(ProviderId::Codex, Os::Windows, &bin.path_only()).unwrap();
        assert_eq!(plan.program, "cmd");
        assert_eq!(
            plan.args,
            [
                "/C",
                &shim.to_string_lossy(),
                "install",
                "-g",
                "@openai/codex"
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn sign_in_runs_the_cli_with_the_login_path() {
        let bin = FakeBin::new();
        let claude = bin.add("claude", "exit 0");
        let codex = bin.add("codex", "exit 0");
        let path = bin.path_only();
        let path_arg = format!("PATH={}", path.to_string_lossy());

        let (program, args) =
            login_invocation_in(ProviderId::ClaudeCode, Os::Linux, &path).unwrap();
        assert_eq!(program, "/usr/bin/env");
        assert_eq!(
            args,
            [
                path_arg.as_str(),
                &claude.to_string_lossy(),
                "auth",
                "login"
            ]
        );

        let (program, args) = login_invocation_in(ProviderId::Codex, Os::Mac, &path).unwrap();
        assert_eq!(program, "/usr/bin/env");
        assert_eq!(args, [path_arg.as_str(), &codex.to_string_lossy(), "login"]);

        let empty = FakeBin::new();
        let err =
            login_invocation_in(ProviderId::Codex, Os::Linux, &empty.path_only()).unwrap_err();
        assert!(err.to_string().contains("isn't installed"), "{err}");
    }

    /// The `/usr/bin/env PATH=… program` form really runs the program with
    /// that `PATH` (the real `env`, and a script that needs `PATH` to find
    /// its interpreter, like an npm-installed CLI needs `node`).
    #[cfg(unix)]
    #[test]
    fn the_env_wrapper_really_sets_path() {
        let bin = FakeBin::new();
        bin.add("fake-interpreter", "echo \"ran with $1\"");
        let script = bin.dir.path().join("tool");
        std::fs::write(&script, "#!/usr/bin/env fake-interpreter\n").unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let (program, args) = with_path(&script, &[], Os::Linux, &bin.path_with_system());
        let out = Command::new(program)
            .args(args)
            .env("PATH", "/nonexistent")
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        assert!(String::from_utf8_lossy(&out.stdout).starts_with("ran with"));
    }

    #[test]
    #[ignore = "needs a signed-in claude CLI; run with --ignored"]
    fn detects_the_real_claude() {
        let info = detect(ProviderId::ClaudeCode);
        assert!(info.installed, "{info:?}");
        let version = info.version.clone().expect("a version");
        assert!(
            version.starts_with(|c: char| c.is_ascii_digit()),
            "{version}"
        );
        assert_eq!(info.logged_in, Some(true));
        let (program, args) = login_invocation(ProviderId::ClaudeCode).unwrap();
        assert!(!program.is_empty());
        assert!(args.ends_with(&["auth".to_string(), "login".to_string()]));
    }

    /// Set `INFINABOX_CODEX_DIR` to the directory holding a real `codex`
    /// (e.g. `node_modules/.bin`).
    #[test]
    #[ignore = "needs a real codex CLI in INFINABOX_CODEX_DIR; run with --ignored"]
    fn detects_the_real_codex() {
        let dir = std::env::var_os("INFINABOX_CODEX_DIR").expect("INFINABOX_CODEX_DIR");
        let own = std::env::var_os("PATH").unwrap_or_default();
        let path = std::env::join_paths(
            std::iter::once(PathBuf::from(dir)).chain(std::env::split_paths(&own)),
        )
        .unwrap();
        let info = detect_in(ProviderId::Codex, &path, Os::current(), PROBE_TIMEOUT);
        assert!(info.installed, "{info:?}");
        let version = info.version.clone().expect("a version");
        assert!(
            version.starts_with(|c: char| c.is_ascii_digit()),
            "{version}"
        );
        // Signed in or not, the real CLI gives a clear answer.
        assert!(info.logged_in.is_some(), "{info:?}");
    }
}
