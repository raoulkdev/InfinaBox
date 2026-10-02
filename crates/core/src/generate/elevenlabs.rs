//! ElevenLabs, sound effects and music. The base URL comes from `INFINABOX_ELEVENLABS_BASE` (for tests) or
//! `https://api.elevenlabs.io`.
//!
//! Assumed API shape, written from ElevenLabs' public documentation and NOT verified against the
//! real service (the build machine has no network access to it):
//!
//! - Sound effects: `POST {base}/v1/sound-generation?output_format=mp3_44100_128` with header
//!   `xi-api-key: <key>` and JSON `{"text": "...", "duration_seconds"?: 0.5-22,
//!   "prompt_influence"?: 0.3}`. Response: audio bytes (`audio/mpeg`). `duration_seconds` is clamped
//!   to 0.5-22.
//! - Music: `POST {base}/v1/music?output_format=mp3_44100_128`, same header, JSON
//!   `{"prompt": "...", "music_length_ms": 3000-300000}` (from `duration_seconds`, default 30 s,
//!   clamped). When `looping` is set the prompt gets "seamless loop, same start and end" appended.
//!   Response: audio bytes.
//!
//! The music endpoint's exact shape is the LEAST certain of everything in this module: it is newer
//! than the sound-effects endpoint and its path, field names and query parameters may differ from
//! what is assumed here. If real music generation fails with a 404 or a 4xx naming a field, this is
//! the first place to look.
//!
//! The prompt reaching `generate` already has the style guide's audio-mood text appended (see
//! `generate::run`).

use std::path::Path;
use std::time::Instant;

use anyhow::{Result, bail};
use serde_json::{Value, json};

use super::{GenKind, GenProviderInfo, GenRequest, GenResult, Generator, audio_result, post_json, secret};
use crate::secrets::{SecretName, SecretStore};

pub const DEFAULT_BASE: &str = "https://api.elevenlabs.io";
pub const BASE_ENV: &str = "INFINABOX_ELEVENLABS_BASE";

const PROVIDER: &str = "ElevenLabs";
const OUTPUT_FORMAT: &str = "mp3_44100_128";
const SFX_MODEL: &str = "sound-generation";
const MUSIC_MODEL: &str = "music";
const LOOP_HINT: &str = "seamless loop, same start and end";
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

/// Returns (path, model label, body, prompt actually sent) for the request.
fn build(request: &GenRequest) -> (&'static str, &'static str, Value, String) {
    let seconds = request.options.duration_seconds.filter(|s| s.is_finite());
    match request.kind {
        GenKind::Music => {
            let mut prompt = request.prompt.trim().to_string();
            if request.options.looping {
                prompt = format!("{prompt}, {LOOP_HINT}");
            }
            let ms = (seconds.unwrap_or(30.0) as f64 * 1000.0).round().clamp(3_000.0, 300_000.0) as u64;
            ("/v1/music", MUSIC_MODEL, json!({ "prompt": prompt, "music_length_ms": ms }), prompt)
        }
        _ => {
            let prompt = request.prompt.trim().to_string();
            let mut body = json!({ "text": prompt });
            if let Some(s) = seconds {
                // Rounded to 0.1 s so the JSON number is tidy.
                body["duration_seconds"] = json!(((s as f64).clamp(0.5, 22.0) * 10.0).round() / 10.0);
                body["prompt_influence"] = json!(0.3);
            }
            ("/v1/sound-generation", SFX_MODEL, body, prompt)
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
        if !KINDS.contains(&request.kind) {
            bail!("ElevenLabs makes sound effects and music here.");
        }
        let Some(key) = secret(secrets, SecretName::ElevenLabsApiKey)? else {
            bail!("Connect ElevenLabs first: add your API key in Assets → Generate.");
        };
        let (path, model, body, prompt_used) = build(request);
        let started = Instant::now();
        let url = format!("{}{path}?output_format={OUTPUT_FORMAT}", self.base.trim_end_matches('/'));
        let reply = post_json(PROVIDER, &url, &[("xi-api-key", &key)], &body)?;
        audio_result("elevenlabs", PROVIDER, reply, out_dir, model, prompt_used, started)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::GenOptions;
    use crate::generate::fake::{Canned, Fake};
    use crate::secrets::MemoryStore;

    const KEY: &str = "test-key-SECRET-123";
    const MP3: &[u8] = b"ID3\x04\x00\x00\x00\x00\x00\x00fake mp3 frames";

    fn store() -> MemoryStore {
        let s = MemoryStore::default();
        s.set(SecretName::ElevenLabsApiKey, KEY).unwrap();
        s
    }

    fn req(kind: GenKind, prompt: &str, options: GenOptions) -> GenRequest {
        GenRequest { kind, prompt: prompt.into(), options, style_guide: None }
    }

    #[test]
    fn sound_effect_request_and_reply() {
        let fake = Fake::start(Canned::new(200, "audio/mpeg", MP3));
        let dir = tempfile::tempdir().unwrap();
        let g = ElevenLabs { base: fake.base.clone() };
        let opts = GenOptions { duration_seconds: Some(1.5), ..Default::default() };
        let r = g.generate(&req(GenKind::Sfx, "coin pickup", opts), &store(), dir.path()).unwrap();

        let seen = fake.only_request();
        assert_eq!(seen.method, "POST");
        assert_eq!(seen.url, "/v1/sound-generation?output_format=mp3_44100_128");
        assert_eq!(seen.header("xi-api-key"), Some(KEY));
        assert_eq!(seen.header("content-type"), Some("application/json"));
        assert_eq!(seen.json(), json!({"text": "coin pickup", "duration_seconds": 1.5, "prompt_influence": 0.3}));
        assert_eq!((r.mime.as_str(), r.extension.as_str(), r.provider.as_str()), ("audio/mpeg", "mp3", "elevenlabs"));
        assert_eq!(std::fs::read(&r.file).unwrap(), MP3);
        assert_eq!(r.prompt_used, "coin pickup");
    }

    #[test]
    fn sound_effect_without_duration_sends_only_text_and_clamps() {
        let fake = Fake::start(Canned::new(200, "audio/mpeg", MP3));
        let dir = tempfile::tempdir().unwrap();
        let g = ElevenLabs { base: fake.base.clone() };
        g.generate(&req(GenKind::Sfx, "jump", GenOptions::default()), &store(), dir.path()).unwrap();
        assert_eq!(fake.seen.lock().unwrap()[0].json(), json!({"text": "jump"}));
        for (asked, sent) in [(0.1, 0.5), (60.0, 22.0)] {
            let opts = GenOptions { duration_seconds: Some(asked), ..Default::default() };
            let (_, _, body, _) = build(&req(GenKind::Sfx, "x", opts));
            assert_eq!(body["duration_seconds"], json!(sent));
        }
    }

    #[test]
    fn music_request_defaults_clamps_and_loops() {
        let fake = Fake::start(Canned::new(200, "audio/mpeg", MP3));
        let dir = tempfile::tempdir().unwrap();
        let g = ElevenLabs { base: fake.base.clone() };
        let opts = GenOptions { looping: true, ..Default::default() };
        let r = g.generate(&req(GenKind::Music, "calm forest theme", opts), &store(), dir.path()).unwrap();
        let seen = fake.only_request();
        assert_eq!(seen.url, "/v1/music?output_format=mp3_44100_128");
        assert_eq!(seen.header("xi-api-key"), Some(KEY));
        assert_eq!(
            seen.json(),
            json!({"prompt": "calm forest theme, seamless loop, same start and end", "music_length_ms": 30000})
        );
        assert_eq!(r.prompt_used, "calm forest theme, seamless loop, same start and end");
        assert_eq!(r.model, "music");

        for (secs, ms) in [(1.0, 3_000u64), (1000.0, 300_000), (12.5, 12_500)] {
            let opts = GenOptions { duration_seconds: Some(secs), ..Default::default() };
            let (_, _, body, _) = build(&req(GenKind::Music, "x", opts));
            assert_eq!(body["music_length_ms"], json!(ms));
        }
    }

    #[test]
    fn images_and_voice_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let g = ElevenLabs { base: "http://127.0.0.1:1".into() };
        assert!(g.generate(&req(GenKind::Image, "x", GenOptions::default()), &store(), dir.path()).is_err());
    }

    #[test]
    fn missing_key_says_where_to_connect() {
        let dir = tempfile::tempdir().unwrap();
        let g = ElevenLabs { base: "http://127.0.0.1:1".into() };
        let e = g.generate(&req(GenKind::Sfx, "x", GenOptions::default()), &MemoryStore::default(), dir.path());
        let e = e.unwrap_err().to_string();
        assert!(e.contains("Connect ElevenLabs") && e.contains("Assets → Generate"));
    }

    #[test]
    fn statuses_map_to_plain_sentences_and_never_leak_the_key() {
        let cases: [(u16, &str, &str); 5] = [
            (401, "{}", "ElevenLabs didn't accept your key. Check it in Assets → Generate."),
            (402, "{}", "Your ElevenLabs account has no credit left."),
            (429, "{}", "ElevenLabs says you've used your limit for now. Try again in a while."),
            (502, "bad gateway", "ElevenLabs had a problem (status 502). Try again."),
            (
                422,
                r#"{"detail":{"status":"invalid","message":"prompt not allowed"}}"#,
                "ElevenLabs said: prompt not allowed",
            ),
        ];
        for kind in [GenKind::Sfx, GenKind::Music] {
            for (status, body, want) in cases {
                let fake = Fake::start(Canned::json(status, body));
                let dir = tempfile::tempdir().unwrap();
                let g = ElevenLabs { base: fake.base.clone() };
                let e = g.generate(&req(kind, "x", GenOptions::default()), &store(), dir.path()).unwrap_err();
                let shown = format!("{e} / {e:?} / {e:#}");
                assert!(shown.starts_with(want), "{status}: {shown}");
                assert!(!shown.contains(KEY), "key leaked: {shown}");
            }
        }
    }

    #[test]
    fn connection_failure_is_plain_and_does_not_leak_the_key() {
        let dir = tempfile::tempdir().unwrap();
        let g = ElevenLabs { base: "http://127.0.0.1:1".into() };
        let e = g.generate(&req(GenKind::Sfx, "x", GenOptions::default()), &store(), dir.path()).unwrap_err();
        assert_eq!(e.to_string(), "Couldn't reach ElevenLabs. Check your internet connection.");
        assert!(!format!("{e:?}").contains(KEY));
    }

    #[test]
    fn run_appends_audio_mood_and_reports_the_prompt_sent() {
        let fake = Fake::start(Canned::new(200, "audio/mpeg", MP3));
        let dir = tempfile::tempdir().unwrap();
        // `run` builds providers from the environment; call the provider the way it does.
        let mut request = req(GenKind::Sfx, "jump", GenOptions::default());
        request.style_guide = Some("# Look\nBright\n## Audio mood\nWarm chiptune".into());
        request.prompt = crate::generate::prompt_with_style(&request);
        let g = ElevenLabs { base: fake.base.clone() };
        let r = g.generate(&request, &store(), dir.path()).unwrap();
        assert_eq!(r.prompt_used, "jump\n\nStyle: Warm chiptune");
        assert_eq!(fake.only_request().json()["text"], json!("jump\n\nStyle: Warm chiptune"));
    }
}
