//! ElevenLabs, sound effects and music. The base URL comes from `INFINABOX_ELEVENLABS_BASE` (for tests) or
//! `https://api.elevenlabs.io`. The request and response shapes this provider assumes are
//! documented at the top of the finished module. Wave 0 stub — task GN.

use std::path::Path;

use anyhow::{Result, bail};

use super::{GenKind, GenProviderInfo, GenRequest, GenResult, Generator};
use crate::secrets::{SecretName, SecretStore};

pub const DEFAULT_BASE: &str = "https://api.elevenlabs.io";
pub const BASE_ENV: &str = "INFINABOX_ELEVENLABS_BASE";

const KINDS: [GenKind; 2] = [GenKind::Sfx, GenKind::Music];
const NEEDS: [SecretName; 1] = [SecretName::ElevenLabsApiKey];
const BLURB: &str = "Makes sound effects and music loops with your ElevenLabs key.";
const SIGNUP: &str = "https://elevenlabs.io";

pub struct ElevenLabs {
    pub base: String,
}

impl ElevenLabs {
    pub fn from_env() -> Self {
        Self {
            base: std::env::var(BASE_ENV).unwrap_or_else(|_| DEFAULT_BASE.to_string()),
        }
    }
}

impl Generator for ElevenLabs {
    fn info(&self) -> GenProviderInfo {
        GenProviderInfo {
            id: "elevenlabs".into(),
            name: "ElevenLabs, sound effects and music".into(),
            kinds: KINDS.to_vec(),
            needs: NEEDS.to_vec(),
            blurb: BLURB.into(),
            signup_url: SIGNUP.into(),
        }
    }

    fn generate(&self, request: &GenRequest, secrets: &dyn SecretStore, out_dir: &Path) -> Result<GenResult> {
        let _ = (request, secrets, out_dir);
        bail!("not implemented yet (Phase C, task GN)")
    }
}
