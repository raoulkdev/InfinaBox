//! Fish Audio, voice. The base URL comes from `INFINABOX_FISHAUDIO_BASE` (for tests) or
//! `https://api.fish.audio`.
//!
//! Assumed API shape, written from Fish Audio's public documentation and NOT verified against the
//! real service (the build machine has no network access to it):
//!
//! - `POST {base}/v1/tts` with headers `Authorization: Bearer <api key>`,
//!   `Content-Type: application/json` and `model: s1` (or `options.model`).
//! - Body `{"text": "...", "format": "mp3", "reference_id"?: "<voice id>", "latency": "normal"}`.
//! - Response: the audio bytes (`Content-Type: audio/mpeg`).
//!
//! The prompt is the line to speak. The Style Guide is deliberately NOT added to it: text sent to a
//! text-to-speech service is read aloud, so appending style notes would have the voice say them.
//! (`generate::run` skips the style guide for voice for the same reason.)

use std::path::Path;
use std::time::Instant;

use anyhow::{Result, bail};
use serde_json::json;

use super::{GenKind, GenProviderInfo, GenRequest, GenResult, Generator, audio_result, post_json, secret};
use crate::secrets::{SecretName, SecretStore};

pub const DEFAULT_BASE: &str = "https://api.fish.audio";
pub const BASE_ENV: &str = "INFINABOX_FISHAUDIO_BASE";
pub const DEFAULT_MODEL: &str = "s1";

const PROVIDER: &str = "Fish Audio";
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
        if request.kind != GenKind::Voice {
            bail!("Fish Audio only makes voices.");
        }
        let Some(key) = secret(secrets, SecretName::FishAudioApiKey)? else {
            bail!("Connect Fish Audio first: add your API key in Assets → Generate.");
        };
        let text = request.prompt.trim();
        if text.is_empty() {
            bail!("Write the line to speak first.");
        }
        let model = request
            .options
            .model
            .clone()
            .filter(|m| !m.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_MODEL.into());
        let mut body = json!({ "text": text, "format": "mp3", "latency": "normal" });
        if let Some(voice) = request.options.voice_id.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
            body["reference_id"] = json!(voice);
        }
        let started = Instant::now();
        let url = format!("{}/v1/tts", self.base.trim_end_matches('/'));
        let bearer = format!("Bearer {key}");
        let reply = post_json(PROVIDER, &url, &[("Authorization", &bearer), ("model", &model)], &body)?;
        audio_result("fishaudio", PROVIDER, reply, out_dir, &model, text.to_string(), started)
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
        s.set(SecretName::FishAudioApiKey, KEY).unwrap();
        s
    }

    fn req(options: GenOptions) -> GenRequest {
        GenRequest {
            kind: GenKind::Voice,
            prompt: "Welcome, traveller.".into(),
            options,
            style_guide: Some("Bright pixel art".into()),
        }
    }

    #[test]
    fn sends_the_documented_request_and_saves_the_mp3() {
        let fake = Fake::start(Canned::new(200, "audio/mpeg", MP3));
        let dir = tempfile::tempdir().unwrap();
        let g = FishAudio { base: fake.base.clone() };
        let opts = GenOptions { voice_id: Some("voice-42".into()), ..Default::default() };
        let r = g.generate(&req(opts), &store(), dir.path()).unwrap();

        let seen = fake.only_request();
        assert_eq!((seen.method.as_str(), seen.url.as_str()), ("POST", "/v1/tts"));
        assert_eq!(seen.header("authorization"), Some(format!("Bearer {KEY}").as_str()));
        assert_eq!(seen.header("content-type"), Some("application/json"));
        assert_eq!(seen.header("model"), Some("s1"));
        assert_eq!(
            seen.json(),
            json!({"text": "Welcome, traveller.", "format": "mp3", "reference_id": "voice-42", "latency": "normal"})
        );

        assert_eq!((r.mime.as_str(), r.extension.as_str(), r.provider.as_str()), ("audio/mpeg", "mp3", "fishaudio"));
        assert_eq!(r.model, "s1");
        assert_eq!(r.prompt_used, "Welcome, traveller.");
        assert_eq!(std::fs::read(&r.file).unwrap(), MP3);
        assert!(r.file.file_name().unwrap().to_string_lossy().ends_with(".mp3"));
    }

    #[test]
    fn no_voice_means_no_reference_id_and_model_can_be_overridden() {
        let fake = Fake::start(Canned::new(200, "audio/mpeg", MP3));
        let dir = tempfile::tempdir().unwrap();
        let g = FishAudio { base: fake.base.clone() };
        let opts = GenOptions { model: Some("speech-1.6".into()), ..Default::default() };
        g.generate(&req(opts), &store(), dir.path()).unwrap();
        let seen = fake.only_request();
        assert_eq!(seen.header("model"), Some("speech-1.6"));
        assert!(seen.json().get("reference_id").is_none());
    }

    #[test]
    fn missing_key_says_where_to_connect() {
        let dir = tempfile::tempdir().unwrap();
        let g = FishAudio { base: "http://127.0.0.1:1".into() };
        let e = g.generate(&req(GenOptions::default()), &MemoryStore::default(), dir.path()).unwrap_err();
        assert!(e.to_string().contains("Connect Fish Audio") && e.to_string().contains("Assets → Generate"));
    }

    #[test]
    fn statuses_map_to_plain_sentences_and_never_leak_the_key() {
        let cases: [(u16, &str, &str); 5] = [
            (401, "{}", "Fish Audio didn't accept your key. Check it in Assets → Generate."),
            (402, "{}", "Your Fish Audio account has no credit left."),
            (429, "{}", "Fish Audio says you've used your limit for now. Try again in a while."),
            (500, "", "Fish Audio had a problem (status 500). Try again."),
            (422, r#"{"detail":"unknown voice"}"#, "Fish Audio said: unknown voice"),
        ];
        for (status, body, want) in cases {
            let fake = Fake::start(Canned::json(status, body));
            let dir = tempfile::tempdir().unwrap();
            let g = FishAudio { base: fake.base.clone() };
            let e = g.generate(&req(GenOptions::default()), &store(), dir.path()).unwrap_err();
            let shown = format!("{e} / {e:?} / {e:#}");
            assert!(shown.starts_with(want), "{status}: {shown}");
            assert!(!shown.contains(KEY), "key leaked: {shown}");
        }
    }

    #[test]
    fn connection_failure_is_plain_and_does_not_leak_the_key() {
        let dir = tempfile::tempdir().unwrap();
        let g = FishAudio { base: "http://127.0.0.1:1".into() };
        let e = g.generate(&req(GenOptions::default()), &store(), dir.path()).unwrap_err();
        assert_eq!(e.to_string(), "Couldn't reach Fish Audio. Check your internet connection.");
        assert!(!format!("{e:?}").contains(KEY));
    }

    #[test]
    fn a_json_body_with_200_is_an_error_not_a_sound() {
        let fake = Fake::start(Canned::json(200, r#"{"message":"text too long"}"#));
        let dir = tempfile::tempdir().unwrap();
        let g = FishAudio { base: fake.base.clone() };
        let e = g.generate(&req(GenOptions::default()), &store(), dir.path()).unwrap_err();
        assert_eq!(e.to_string(), "Fish Audio said: text too long");
    }
}
