//! "Connect your AI" (spec §8.2): finds which agent CLIs the user has,
//! whether they're signed in, how to install or sign in to each, and which
//! one to recommend. InfinaBox never holds the credentials — the CLIs do.
//!
//! Wave 0 stub (Phase B plan, Task CN fills it in).

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

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

pub fn detect_all() -> Vec<ProviderInfo> {
    ProviderId::ALL.into_iter().map(detect).collect()
}

pub fn detect(id: ProviderId) -> ProviderInfo {
    let (name, login) = match id {
        ProviderId::ClaudeCode => ("Claude Code", "claude auth login"),
        ProviderId::Codex => ("Codex", "codex login"),
    };
    ProviderInfo {
        id,
        name: name.into(),
        installed: false,
        version: None,
        logged_in: None,
        blurb: String::new(),
        install_command: None,
        install_blocker: None,
        login_command: login.into(),
        docs_url: String::new(),
    }
}

/// Program and arguments that run the installer inside a terminal.
pub fn install_invocation(_id: ProviderId) -> Result<(String, Vec<String>)> {
    bail!("not implemented yet")
}

/// Program and arguments that run the CLI's own sign-in inside a terminal.
pub fn login_invocation(_id: ProviderId) -> Result<(String, Vec<String>)> {
    bail!("not implemented yet")
}

/// The first installed and signed-in provider, else the first installed,
/// else Claude Code.
pub fn recommend(infos: &[ProviderInfo]) -> ProviderId {
    let _ = infos;
    ProviderId::ClaudeCode
}
