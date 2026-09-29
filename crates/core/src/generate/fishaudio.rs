//! Fish Audio, voice. The base URL comes from `INFINABOX_FISHAUDIO_BASE` (for tests) or
//! `https://api.fish.audio`. The request and response shapes this provider assumes are
//! documented at the top of the finished module. Wave 0 stub — task GN.

use std::path::Path;

use anyhow::{Result, bail};

use super::{GenKind, GenProviderInfo, GenRequest, GenResult, Generator};
use crate::secrets::{SecretName, SecretStore};

pub const DEFAULT_BASE: &str = "https://api.fish.audio";
pub const BASE_ENV: &str = "INFINABOX_FISHAUDIO_BASE";

const KINDS: [GenKind; 1] = [GenKind::Voice];
const NEEDS: [SecretName; 1] = [SecretName::FishAudioApiKey];
const BLURB: &str = "Speaks character lines, narration and barks in a voice you choose, with your Fish Audio key.";
const SIGNUP: &str = "https://fish.audio";

pub struct FishAudio {
    pub base: String,
}

impl FishAudio {
    pub fn from_env() -> Self {
        Self {
            base: std::env::var(BASE_ENV).unwrap_or_else(|_| DEFAULT_BASE.to_string()),
        }
    }
}

impl Generator for FishAudio {
    fn info(&self) -> GenProviderInfo {
        GenProviderInfo {
            id: "fishaudio".into(),
            name: "Fish Audio, voice".into(),
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
