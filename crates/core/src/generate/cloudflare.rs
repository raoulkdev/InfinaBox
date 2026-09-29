//! Cloudflare Workers AI, images. The base URL comes from `INFINABOX_CLOUDFLARE_BASE` (for tests) or
//! `https://api.cloudflare.com`. The request and response shapes this provider assumes are
//! documented at the top of the finished module. Wave 0 stub — task GN.

use std::path::Path;

use anyhow::{Result, bail};

use super::{GenKind, GenProviderInfo, GenRequest, GenResult, Generator};
use crate::secrets::{SecretName, SecretStore};

pub const DEFAULT_BASE: &str = "https://api.cloudflare.com";
pub const BASE_ENV: &str = "INFINABOX_CLOUDFLARE_BASE";

const KINDS: [GenKind; 1] = [GenKind::Image];
const NEEDS: [SecretName; 2] = [SecretName::CloudflareAccountId, SecretName::CloudflareApiToken];
const BLURB: &str = "Draws sprites, textures, UI and concept art with image models on your Cloudflare account.";
const SIGNUP: &str = "https://dash.cloudflare.com/profile/api-tokens";

pub struct Cloudflare {
    pub base: String,
}

impl Cloudflare {
    pub fn from_env() -> Self {
        Self {
            base: std::env::var(BASE_ENV).unwrap_or_else(|_| DEFAULT_BASE.to_string()),
        }
    }
}

impl Generator for Cloudflare {
    fn info(&self) -> GenProviderInfo {
        GenProviderInfo {
            id: "cloudflare".into(),
            name: "Cloudflare Workers AI, images".into(),
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
