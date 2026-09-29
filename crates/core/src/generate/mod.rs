//! Generating assets with the person's own accounts (spec §7.3): images
//! (Cloudflare Workers AI), voice (Fish Audio), sound effects and music
//! (ElevenLabs). InfinaBox never proxies or pays for generation; keys come
//! from the keychain through `SecretStore`.
//!
//! Phase C contract (frozen). Wave 0 stub — task GN fills in the providers.

pub mod cloudflare;
pub mod elevenlabs;
pub mod fishaudio;
pub mod postprocess;

use std::path::PathBuf;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::secrets::SecretStore;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum GenKind {
    Image,
    Voice,
    Sfx,
    Music,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct GenOptions {
    /// Provider model override (Cloudflare model id; ElevenLabs/Fish model).
    pub model: Option<String>,
    // --- images
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub seed: Option<u64>,
    /// Snap to a small palette after generating.
    pub pixel_art: bool,
    /// Make a near-uniform background transparent.
    pub transparent: bool,
    // --- audio
    pub duration_seconds: Option<f32>,
    /// Fish Audio voice (reference) id.
    pub voice_id: Option<String>,
    pub looping: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GenRequest {
    pub kind: GenKind,
    pub prompt: String,
    pub options: GenOptions,
    /// The Style Guide card's text, appended to the prompt when present.
    pub style_guide: Option<String>,
}

/// A generated file, held until the person accepts or discards it.
#[derive(Clone, Debug, PartialEq)]
pub struct GenResult {
    /// Temp file with the (post-processed) bytes.
    pub file: PathBuf,
    pub mime: String,
    /// "png", "mp3", …
    pub extension: String,
    pub provider: String,
    pub model: String,
    /// The prompt actually sent (with the style guide).
    pub prompt_used: String,
    pub duration_ms: u64,
}

/// What the Generate tab shows for a provider.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct GenProviderInfo {
    pub id: String,
    pub name: String,
    pub kinds: Vec<GenKind>,
    /// Secrets that must be set for it to be connected.
    pub needs: Vec<crate::secrets::SecretName>,
    pub blurb: String,
    /// Where the person makes an account / key.
    pub signup_url: String,
}

pub trait Generator: Send + Sync {
    fn info(&self) -> GenProviderInfo;
    /// Generates into a new temp file under `out_dir`.
    fn generate(
        &self,
        request: &GenRequest,
        secrets: &dyn SecretStore,
        out_dir: &std::path::Path,
    ) -> Result<GenResult>;
}

pub fn generators() -> Vec<Box<dyn Generator>> {
    vec![
        Box::new(cloudflare::Cloudflare::from_env()),
        Box::new(fishaudio::FishAudio::from_env()),
        Box::new(elevenlabs::ElevenLabs::from_env()),
    ]
}

/// The provider that handles `kind`.
pub fn generator_for(kind: GenKind) -> Option<Box<dyn Generator>> {
    generators().into_iter().find(|g| g.info().kinds.contains(&kind))
}

/// The one entry point the app calls: picks the provider for
/// `request.kind`, adds the style guide to the prompt, generates and
/// post-processes.
pub fn run(
    request: &GenRequest,
    secrets: &dyn SecretStore,
    out_dir: &std::path::Path,
) -> Result<GenResult> {
    let _ = (request, secrets, out_dir);
    bail!("not implemented yet (Phase C, task GN)")
}
