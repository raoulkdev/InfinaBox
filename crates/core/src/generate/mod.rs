//! Generating assets with the person's own accounts (spec §7.3): images
//! (Cloudflare Workers AI), voice (Fish Audio), sound effects and music
//! (ElevenLabs). InfinaBox never proxies or pays for generation; keys come
//! from the keychain through `SecretStore`.
//!
//! The public types and the `Generator` trait are a frozen Phase C contract.
//! The providers are written from their documented public APIs and tested
//! against local fake servers only; see `tests/fixtures/providers/README.md`
//! for what is assumed and not verified against the real services.

pub mod cloudflare;
pub mod elevenlabs;
pub mod fishaudio;
pub mod postprocess;

#[cfg(test)]
pub(crate) mod fake;

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};

use crate::secrets::{SecretName, SecretStore};

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

/// Longest prompt the person may write (before the style guide is added).
pub const MAX_PROMPT_CHARS: usize = 2000;
/// How much of the Style Guide is added to a prompt.
const STYLE_CHARS: usize = 800;
/// Largest response body we accept from a provider.
const MAX_RESPONSE_BYTES: u64 = 40 * 1024 * 1024;

/// The one entry point the app calls: picks the provider for
/// `request.kind`, adds the style guide to the prompt, generates and
/// post-processes (images).
pub fn run(
    request: &GenRequest,
    secrets: &dyn SecretStore,
    out_dir: &Path,
) -> Result<GenResult> {
    let prompt = request.prompt.trim();
    if prompt.is_empty() {
        bail!("Describe what you want first, then try again.");
    }
    if prompt.chars().count() > MAX_PROMPT_CHARS {
        bail!("That description is too long. Keep it under {MAX_PROMPT_CHARS} characters.");
    }
    let generator = generator_for(request.kind)
        .ok_or_else(|| anyhow!("There's no generator for that kind of asset yet."))?;
    let mut request = request.clone();
    request.prompt = prompt_with_style(&request);
    generator.generate(&request, secrets, out_dir)
}

/// The prompt to send: the person's words plus the Style Guide. Voice lines
/// are the exception: they are spoken aloud, so the style guide is never added
/// to them.
pub(crate) fn prompt_with_style(request: &GenRequest) -> String {
    let prompt = request.prompt.trim();
    if request.kind == GenKind::Voice {
        return prompt.to_string();
    }
    let Some(guide) = request.style_guide.as_deref().map(str::trim).filter(|g| !g.is_empty()) else {
        return prompt.to_string();
    };
    let text = if matches!(request.kind, GenKind::Sfx | GenKind::Music) {
        audio_mood(guide)
    } else {
        guide.to_string()
    };
    let text: String = text.trim().chars().take(STYLE_CHARS).collect();
    if text.trim().is_empty() {
        return prompt.to_string();
    }
    format!("{prompt}\n\nStyle: {}", text.trim())
}

/// The audio-mood part of a Style Guide: lines that mention audio, sound,
/// music or mood, and the lines under a heading that does. Falls back to the
/// whole guide when nothing is called out.
fn audio_mood(guide: &str) -> String {
    let is_audio = |l: &str| {
        let l = l.to_lowercase();
        ["audio", "sound", "music", "mood"].iter().any(|w| l.contains(w))
    };
    let mut out = Vec::new();
    let mut in_section = false;
    for line in guide.lines() {
        if line.trim_start().starts_with('#') {
            in_section = is_audio(line);
        } else if in_section || is_audio(line) {
            out.push(line.trim());
        }
    }
    let joined = out.into_iter().filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" ");
    if joined.is_empty() { guide.replace('\n', " ") } else { joined }
}

// --- shared HTTP and file helpers for the providers -----------------------

pub(crate) fn http_client() -> Result<reqwest::blocking::Client> {
    // Follow redirects only within the host we called, so a key is never
    // forwarded elsewhere.
    let policy = reqwest::redirect::Policy::custom(|attempt| {
        let same_host = attempt.previous().first().and_then(|u| u.host_str().map(str::to_owned))
            == attempt.url().host_str().map(str::to_owned);
        if attempt.previous().len() >= 5 || !same_host {
            attempt.stop()
        } else {
            attempt.follow()
        }
    });
    reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(180))
        .redirect(policy)
        .user_agent("InfinaBox")
        .build()
        .map_err(|_| anyhow!("Couldn't start a network connection on this computer."))
}

/// A secret from the store, trimmed; `None` when unset or blank.
pub(crate) fn secret(secrets: &dyn SecretStore, name: SecretName) -> Result<Option<String>> {
    Ok(secrets.get(name)?.map(|v| v.trim().to_string()).filter(|v| !v.is_empty()))
}

/// A successful provider response.
pub(crate) struct Reply {
    pub bytes: Vec<u8>,
    pub content_type: String,
}

/// POSTs `body` as JSON with `headers` and maps every failure to a plain
/// sentence. `provider` is the display name ("Fish Audio"). Nothing in the
/// returned errors contains a header value.
pub(crate) fn post_json(
    provider: &str,
    url: &str,
    headers: &[(&str, &str)],
    body: &serde_json::Value,
) -> Result<Reply> {
    let client = http_client()?;
    let mut req = client.post(url).header("Content-Type", "application/json");
    for (name, value) in headers {
        let value = reqwest::header::HeaderValue::from_str(value).map_err(|_| {
            anyhow!("Your {provider} key has characters that aren't allowed. Paste it again in Assets → Generate.")
        })?;
        req = req.header(*name, value);
    }
    let resp = req.body(body.to_string()).send().map_err(|e| net_error(provider, &e))?;
    let status = resp.status();
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let mut bytes = Vec::new();
    resp.take(MAX_RESPONSE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| anyhow!("Couldn't reach {provider}. Check your internet connection."))?;
    if bytes.len() as u64 > MAX_RESPONSE_BYTES {
        bail!("{provider} sent back something too large to use.");
    }
    if !status.is_success() {
        return Err(status_error(provider, status.as_u16(), &bytes));
    }
    Ok(Reply { bytes, content_type })
}

fn net_error(provider: &str, e: &reqwest::Error) -> anyhow::Error {
    if e.is_timeout() {
        anyhow!("{provider} took too long to answer. Try again.")
    } else {
        anyhow!("Couldn't reach {provider}. Check your internet connection.")
    }
}

fn status_error(provider: &str, status: u16, body: &[u8]) -> anyhow::Error {
    match status {
        401 | 403 => anyhow!("{provider} didn't accept your key. Check it in Assets → Generate."),
        402 => anyhow!("Your {provider} account has no credit left."),
        429 => anyhow!("{provider} says you've used your limit for now. Try again in a while."),
        500..=599 => anyhow!("{provider} had a problem (status {status}). Try again."),
        400..=499 => match error_message(body) {
            Some(m) => anyhow!("{provider} said: {m}"),
            None => anyhow!("{provider} couldn't do that (status {status})."),
        },
        _ => anyhow!("{provider} sent an unexpected answer (status {status})."),
    }
}

/// The human message in a JSON error body, whatever the provider's shape:
/// `errors[0].message`, `error.message`, `error`, `message`, `detail`
/// (string, or `detail.message`, or `detail[0].msg`).
pub(crate) fn error_message(body: &[u8]) -> Option<String> {
    let v: serde_json::Value = serde_json::from_slice(body).ok()?;
    let text = |x: &serde_json::Value| x.as_str().map(str::to_owned);
    let found = text(&v["errors"][0]["message"])
        .or_else(|| text(&v["error"]["message"]))
        .or_else(|| text(&v["error"]))
        .or_else(|| text(&v["message"]))
        .or_else(|| text(&v["detail"]))
        .or_else(|| text(&v["detail"]["message"]))
        .or_else(|| text(&v["detail"][0]["msg"]))?;
    let found: String = found.trim().chars().take(300).collect();
    (!found.is_empty()).then_some(found)
}

/// Image container from magic bytes: (mime, extension).
pub(crate) fn image_kind(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some(("image/png", "png"))
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(("image/jpeg", "jpg"))
    } else if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(("image/webp", "webp"))
    } else {
        None
    }
}

/// Writes `bytes` to `out_dir/gen-<uuid>.<ext>` atomically (temp file, then
/// rename).
pub(crate) fn write_output(out_dir: &Path, bytes: &[u8], ext: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(out_dir)
        .map_err(|_| anyhow!("Couldn't create a place to keep the result."))?;
    let id = uuid::Uuid::new_v4();
    let tmp = out_dir.join(format!(".gen-{id}.tmp"));
    let dest = out_dir.join(format!("gen-{id}.{ext}"));
    std::fs::write(&tmp, bytes).map_err(|_| anyhow!("Couldn't save the result."))?;
    if std::fs::rename(&tmp, &dest).is_err() {
        let _ = std::fs::remove_file(&tmp);
        bail!("Couldn't save the result.");
    }
    Ok(dest)
}

/// Turns an audio reply into a result file. Providers answer with mp3 bytes; a
/// JSON body here means an error we should surface.
pub(crate) fn audio_result(
    provider_id: &str,
    provider: &str,
    reply: Reply,
    out_dir: &Path,
    model: &str,
    prompt_used: String,
    started: std::time::Instant,
) -> Result<GenResult> {
    if reply.bytes.is_empty() {
        bail!("{provider} sent back an empty sound. Try again.");
    }
    if reply.content_type.contains("json") || reply.bytes.first() == Some(&b'{') {
        match error_message(&reply.bytes) {
            Some(m) => bail!("{provider} said: {m}"),
            None => bail!("{provider} didn't send back any audio."),
        }
    }
    let file = write_output(out_dir, &reply.bytes, "mp3")?;
    Ok(GenResult {
        file,
        mime: "audio/mpeg".into(),
        extension: "mp3".into(),
        provider: provider_id.into(),
        model: model.into(),
        prompt_used,
        duration_ms: started.elapsed().as_millis() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(kind: GenKind, prompt: &str, style: Option<&str>) -> GenRequest {
        GenRequest { kind, prompt: prompt.into(), options: GenOptions::default(), style_guide: style.map(Into::into) }
    }

    #[test]
    fn style_is_appended_and_trimmed() {
        let long = "x".repeat(1000);
        let p = prompt_with_style(&req(GenKind::Image, " a cat ", Some(&long)));
        assert_eq!(p, format!("a cat\n\nStyle: {}", "x".repeat(800)));
        assert_eq!(prompt_with_style(&req(GenKind::Image, "a cat", None)), "a cat");
        assert_eq!(prompt_with_style(&req(GenKind::Image, "a cat", Some("  "))), "a cat");
    }

    #[test]
    fn voice_lines_never_get_the_style_guide() {
        let p = prompt_with_style(&req(GenKind::Voice, "Hello there", Some("Bright pixel art")));
        assert_eq!(p, "Hello there");
    }

    #[test]
    fn audio_uses_the_audio_mood_part() {
        let guide = "# Look\nBright pixel art\n\n## Audio mood\nWarm chiptune, gentle\n";
        let p = prompt_with_style(&req(GenKind::Music, "menu theme", Some(guide)));
        assert_eq!(p, "menu theme\n\nStyle: Warm chiptune, gentle");
        let p = prompt_with_style(&req(GenKind::Sfx, "jump", Some("Bright pixel art")));
        assert_eq!(p, "jump\n\nStyle: Bright pixel art");
    }

    #[test]
    fn run_rejects_bad_prompts() {
        let secrets = crate::secrets::MemoryStore::default();
        let dir = tempfile::tempdir().unwrap();
        let e = run(&req(GenKind::Image, "   ", None), &secrets, dir.path()).unwrap_err();
        assert!(e.to_string().contains("Describe what you want"));
        let e = run(&req(GenKind::Image, &"a".repeat(2001), None), &secrets, dir.path()).unwrap_err();
        assert!(e.to_string().contains("too long"));
    }

    #[test]
    fn error_messages_are_found_in_common_shapes() {
        assert_eq!(error_message(br#"{"errors":[{"message":"bad"}]}"#).as_deref(), Some("bad"));
        assert_eq!(error_message(br#"{"detail":{"message":"nope"}}"#).as_deref(), Some("nope"));
        assert_eq!(error_message(br#"{"detail":[{"msg":"short"}]}"#).as_deref(), Some("short"));
        assert_eq!(error_message(b"not json"), None);
    }

    #[test]
    fn every_kind_has_a_provider() {
        for k in [GenKind::Image, GenKind::Voice, GenKind::Sfx, GenKind::Music] {
            assert!(generator_for(k).is_some());
        }
    }
}
