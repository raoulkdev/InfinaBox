//! Cloudflare Workers AI, images. The base URL comes from `INFINABOX_CLOUDFLARE_BASE` (for tests) or
//! `https://api.cloudflare.com`.
//!
//! Assumed API shape, written from Cloudflare's public documentation and NOT verified against the
//! real service (the build machine has no network access to it):
//!
//! - `POST {base}/client/v4/accounts/{account_id}/ai/run/{model}` with headers
//!   `Authorization: Bearer <api token>` and `Content-Type: application/json`.
//! - Body `{"prompt": "...", "steps"?: n, "width"?: n, "height"?: n, "seed"?: n}`; only what the
//!   options set is sent. Default model `@cf/black-forest-labs/flux-1-schnell` (with `steps` 4).
//! - Response is either JSON `{"result": {"image": "<base64 PNG/JPEG>"}, "success": true,
//!   "errors": [], "messages": []}` (Flux models) or raw image bytes (`Content-Type: image/png`,
//!   Stable Diffusion models). `success: false` carries `errors[0].message`.
//!
//! Post-processing (resize, background clean-up, palette snap) runs here, and the file is saved as PNG.

use std::path::Path;
use std::time::Instant;

use anyhow::{Result, anyhow, bail};
use base64::Engine;
use serde_json::{Value, json};

use super::{
    GenKind, GenProviderInfo, GenRequest, GenResult, Generator, error_message, image_kind, post_json,
    postprocess, secret, write_output,
};
use crate::secrets::{SecretName, SecretStore};

pub const DEFAULT_BASE: &str = "https://api.cloudflare.com";
pub const BASE_ENV: &str = "INFINABOX_CLOUDFLARE_BASE";
pub const DEFAULT_MODEL: &str = "@cf/black-forest-labs/flux-1-schnell";

const PROVIDER: &str = "Cloudflare";
const PALETTE_COLORS: usize = 16;
const CLEAR_TOLERANCE: u8 = 24;
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

    /// The JSON body: only what the options set, plus `steps` 4 for the default model.
    fn body(request: &GenRequest, model: &str) -> Value {
        let o = &request.options;
        let mut body = json!({ "prompt": request.prompt });
        if model == DEFAULT_MODEL {
            body["steps"] = json!(4);
        }
        if let Some(w) = o.width {
            body["width"] = json!(w);
        }
        if let Some(h) = o.height {
            body["height"] = json!(h);
        }
        if let Some(s) = o.seed {
            body["seed"] = json!(s);
        }
        body
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
        if request.kind != GenKind::Image {
            bail!("Cloudflare only makes images.");
        }
        let (Some(account), Some(token)) = (
            secret(secrets, SecretName::CloudflareAccountId)?,
            secret(secrets, SecretName::CloudflareApiToken)?,
        ) else {
            bail!("Connect Cloudflare first: add your account ID and API token in Assets → Generate.");
        };
        if !account.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            bail!("Your Cloudflare account ID doesn't look right. Check it in Assets → Generate.");
        }
        let model = request
            .options
            .model
            .clone()
            .filter(|m| !m.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_MODEL.into());
        let started = Instant::now();
        let url = format!("{}/client/v4/accounts/{account}/ai/run/{model}", self.base.trim_end_matches('/'));
        let bearer = format!("Bearer {token}");
        let reply = post_json(PROVIDER, &url, &[("Authorization", &bearer)], &Self::body(request, &model))?;

        let image_bytes = if reply.content_type.contains("json") || reply.bytes.first() == Some(&b'{') {
            let v: Value = serde_json::from_slice(&reply.bytes)
                .map_err(|_| anyhow!("Cloudflare sent back something that isn't a picture."))?;
            if v["success"].as_bool() == Some(false) {
                match error_message(&reply.bytes) {
                    Some(m) => bail!("Cloudflare said: {m}"),
                    None => bail!("Cloudflare couldn't make that picture."),
                }
            }
            let b64 = v["result"]["image"]
                .as_str()
                .ok_or_else(|| anyhow!("Cloudflare didn't send back a picture."))?;
            base64::engine::general_purpose::STANDARD
                .decode(b64.trim())
                .map_err(|_| anyhow!("Cloudflare sent back a picture that couldn't be read."))?
        } else {
            reply.bytes
        };
        let Some((native_mime, native_ext)) = image_kind(&image_bytes) else {
            bail!("Cloudflare sent back something that isn't a picture.");
        };

        let o = &request.options;
        let sized = o.width.zip(o.height);
        let (bytes, mime, extension) = if sized.is_none() && !o.pixel_art && !o.transparent {
            // Nothing to change: keep the provider's own bytes.
            (image_bytes, native_mime, native_ext)
        } else {
            let mut img = image::load_from_memory(&image_bytes)
                .map_err(|_| anyhow!("Cloudflare sent back a picture that couldn't be read."))?
                .to_rgba8();
            if let Some((w, h)) = sized {
                img = postprocess::resize(&img, w, h, o.pixel_art)?;
            }
            if o.transparent {
                img = postprocess::clear_background(&img, CLEAR_TOLERANCE)?;
            }
            if o.pixel_art {
                img = postprocess::palette_snap(&img, PALETTE_COLORS)?;
            }
            let mut png = Vec::new();
            img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                .map_err(|_| anyhow!("Couldn't save the picture."))?;
            (png, "image/png", "png")
        };
        let file = write_output(out_dir, &bytes, extension)?;
        Ok(GenResult {
            file,
            mime: mime.into(),
            extension: extension.into(),
            provider: "cloudflare".into(),
            model,
            prompt_used: request.prompt.clone(),
            duration_ms: started.elapsed().as_millis() as u64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::GenOptions;
    use crate::generate::fake::{Canned, Fake, png_bytes};
    use crate::secrets::MemoryStore;

    const TOKEN: &str = "test-token-SECRET-123";

    fn store() -> MemoryStore {
        let s = MemoryStore::default();
        s.set(SecretName::CloudflareAccountId, "acct123").unwrap();
        s.set(SecretName::CloudflareApiToken, TOKEN).unwrap();
        s
    }

    fn req(options: GenOptions) -> GenRequest {
        GenRequest { kind: GenKind::Image, prompt: "a red slime".into(), options, style_guide: None }
    }

    fn flux_reply(png: &[u8]) -> Canned {
        let b64 = base64::engine::general_purpose::STANDARD.encode(png);
        Canned::json(200, &format!(r#"{{"result":{{"image":"{b64}"}},"success":true,"errors":[],"messages":[]}}"#))
    }

    #[test]
    fn flux_json_reply_becomes_an_image_file() {
        let fake = Fake::start(flux_reply(&png_bytes(8, 8)));
        let dir = tempfile::tempdir().unwrap();
        let g = Cloudflare { base: fake.base.clone() };
        let r = g
            .generate(&req(GenOptions { seed: Some(7), ..Default::default() }), &store(), dir.path())
            .unwrap();

        let seen = fake.only_request();
        assert_eq!(seen.method, "POST");
        assert_eq!(seen.url, "/client/v4/accounts/acct123/ai/run/@cf/black-forest-labs/flux-1-schnell");
        assert_eq!(seen.header("authorization"), Some(format!("Bearer {TOKEN}").as_str()));
        assert_eq!(seen.header("content-type"), Some("application/json"));
        assert_eq!(seen.header("user-agent"), Some("InfinaBox"));
        assert_eq!(seen.json(), json!({"prompt": "a red slime", "steps": 4, "seed": 7}));

        assert_eq!((r.mime.as_str(), r.extension.as_str()), ("image/png", "png"));
        assert_eq!(r.provider, "cloudflare");
        assert_eq!(r.model, DEFAULT_MODEL);
        assert_eq!(r.prompt_used, "a red slime");
        assert!(r.file.starts_with(dir.path()));
        assert!(r.file.file_name().unwrap().to_string_lossy().starts_with("gen-"));
        assert_eq!(image::open(&r.file).unwrap().width(), 8);
    }

    #[test]
    fn raw_image_reply_and_custom_model_without_steps() {
        let fake = Fake::start(Canned::new(200, "image/png", png_bytes(6, 4)));
        let dir = tempfile::tempdir().unwrap();
        let g = Cloudflare { base: fake.base.clone() };
        let opts = GenOptions {
            model: Some("@cf/stabilityai/stable-diffusion-xl-base-1.0".into()),
            width: Some(6),
            height: Some(4),
            ..Default::default()
        };
        let r = g.generate(&req(opts), &store(), dir.path()).unwrap();
        let seen = fake.only_request();
        assert_eq!(seen.url, "/client/v4/accounts/acct123/ai/run/@cf/stabilityai/stable-diffusion-xl-base-1.0");
        assert_eq!(seen.json(), json!({"prompt": "a red slime", "width": 6, "height": 4}));
        assert_eq!(image::image_dimensions(&r.file).unwrap(), (6, 4));
    }

    #[test]
    fn postprocessing_follows_the_options() {
        // 8x8 white background with a dark square in the middle.
        let mut img = image::RgbaImage::from_pixel(8, 8, image::Rgba([255, 255, 255, 255]));
        for x in 3..5 {
            for y in 3..5 {
                img.put_pixel(x, y, image::Rgba([20, 20, 20, 255]));
            }
        }
        let mut png = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let fake = Fake::start(flux_reply(&png));
        let dir = tempfile::tempdir().unwrap();
        let g = Cloudflare { base: fake.base.clone() };
        let opts = GenOptions {
            width: Some(16),
            height: Some(16),
            pixel_art: true,
            transparent: true,
            ..Default::default()
        };
        let r = g.generate(&req(opts), &store(), dir.path()).unwrap();
        let out = image::open(&r.file).unwrap().to_rgba8();
        assert_eq!(out.dimensions(), (16, 16));
        assert_eq!(out.get_pixel(0, 0)[3], 0, "background cleared");
        assert_eq!(out.get_pixel(8, 8)[3], 255, "subject kept");
    }

    #[test]
    fn success_false_surfaces_the_message() {
        let fake = Fake::start(Canned::json(
            200,
            r#"{"success":false,"errors":[{"code":1,"message":"prompt was refused"}],"result":null}"#,
        ));
        let dir = tempfile::tempdir().unwrap();
        let g = Cloudflare { base: fake.base.clone() };
        let e = g.generate(&req(GenOptions::default()), &store(), dir.path()).unwrap_err().to_string();
        assert_eq!(e, "Cloudflare said: prompt was refused");
    }

    #[test]
    fn missing_credentials_say_where_to_connect() {
        let dir = tempfile::tempdir().unwrap();
        let g = Cloudflare { base: "http://127.0.0.1:1".into() };
        let only_token = MemoryStore::default();
        only_token.set(SecretName::CloudflareApiToken, TOKEN).unwrap();
        for s in [MemoryStore::default(), only_token] {
            let e = g.generate(&req(GenOptions::default()), &s, dir.path()).unwrap_err().to_string();
            assert!(e.contains("Connect Cloudflare") && e.contains("Assets → Generate"), "{e}");
        }
    }

    #[test]
    fn statuses_map_to_plain_sentences_and_never_leak_the_token() {
        let cases: [(u16, &str, &str); 6] = [
            (401, "{}", "Cloudflare didn't accept your key. Check it in Assets → Generate."),
            (403, "{}", "Cloudflare didn't accept your key. Check it in Assets → Generate."),
            (402, "{}", "Your Cloudflare account has no credit left."),
            (429, "{}", "Cloudflare says you've used your limit for now. Try again in a while."),
            (503, "oops", "Cloudflare had a problem (status 503). Try again."),
            (
                400,
                r#"{"errors":[{"message":"width must be a multiple of 8"}]}"#,
                "Cloudflare said: width must be a multiple of 8",
            ),
        ];
        for (status, body, want) in cases {
            let fake = Fake::start(Canned::json(status, body));
            let dir = tempfile::tempdir().unwrap();
            let g = Cloudflare { base: fake.base.clone() };
            let e = g.generate(&req(GenOptions::default()), &store(), dir.path()).unwrap_err();
            let shown = format!("{e} / {e:?} / {e:#}");
            assert!(shown.starts_with(want), "{status}: {shown}");
            assert!(!shown.contains(TOKEN), "token leaked: {shown}");
        }
    }

    #[test]
    fn connection_failure_is_plain() {
        let dir = tempfile::tempdir().unwrap();
        let g = Cloudflare { base: "http://127.0.0.1:1".into() };
        let e = g.generate(&req(GenOptions::default()), &store(), dir.path()).unwrap_err();
        assert_eq!(e.to_string(), "Couldn't reach Cloudflare. Check your internet connection.");
        assert!(!format!("{e:?}").contains(TOKEN));
    }

    #[test]
    fn garbage_bytes_are_not_accepted_as_a_picture() {
        let fake = Fake::start(Canned::new(200, "image/png", b"not an image".to_vec()));
        let dir = tempfile::tempdir().unwrap();
        let g = Cloudflare { base: fake.base.clone() };
        assert!(g.generate(&req(GenOptions::default()), &store(), dir.path()).is_err());
    }
}
