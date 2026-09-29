# Generation provider shapes (assumed)

The build machine has no network access to these services, so the providers in
`crates/core/src/generate/` were written from each service's documented public
API and are tested only against local fake servers (`generate/fake.rs`).

**NOT verified against the real service: everything below.** Nothing here was
recorded from a real response. Test keys are obviously fake (`test-key-...`).

Base URLs can be overridden for tests with `INFINABOX_CLOUDFLARE_BASE`,
`INFINABOX_FISHAUDIO_BASE` and `INFINABOX_ELEVENLABS_BASE`.

All requests: `Content-Type: application/json`, `User-Agent: InfinaBox`,
connect timeout 10 s, total timeout 180 s, response capped at 40 MB, redirects
followed only within the same host.

## Cloudflare Workers AI (images)

- `POST {base}/client/v4/accounts/{account_id}/ai/run/{model}`
- Header `Authorization: Bearer <api token>`
- Default model `@cf/black-forest-labs/flux-1-schnell` (sends `steps: 4`).
- Body `{"prompt": "...", "steps"?: n, "width"?: n, "height"?: n, "seed"?: n}`;
  only what the options set is sent.
- Response, either:
  - JSON `{"result": {"image": "<base64 PNG/JPEG>"}, "success": true, "errors": [], "messages": []}`
    (Flux models); `success: false` uses `errors[0].message`; or
  - raw image bytes with `Content-Type: image/png` (Stable Diffusion models).
- Post-processing is done locally (resize, background clear, 16-colour snap).

## Fish Audio (voice)

- `POST {base}/v1/tts`
- Headers `Authorization: Bearer <api key>`, `model: s1` (or the model option).
- Body `{"text": "...", "format": "mp3", "reference_id"?: "<voice id>", "latency": "normal"}`
- Response: audio bytes, `Content-Type: audio/mpeg`.
- The style guide is never added to voice text (it would be read aloud).

## ElevenLabs (sound effects)

- `POST {base}/v1/sound-generation?output_format=mp3_44100_128`
- Header `xi-api-key: <api key>`
- Body `{"text": "...", "duration_seconds"?: 0.5-22, "prompt_influence"?: 0.3}`
  (`prompt_influence` is sent only together with a duration).
- Response: audio bytes, `Content-Type: audio/mpeg`.

## ElevenLabs (music) - least certain

- `POST {base}/v1/music?output_format=mp3_44100_128`
- Header `xi-api-key: <api key>`
- Body `{"prompt": "...", "music_length_ms": 3000-300000}`; default 30 000 ms;
  `looping` appends ", seamless loop, same start and end" to the prompt.
- Response: audio bytes.
- The path, field names and query parameters of this endpoint are the part most
  likely to differ from the real service.

## Error mapping (all providers)

401/403 "didn't accept your key", 402 "no credit left", 429 "used your limit",
other 4xx uses the JSON error message when there is one, 5xx "had a problem
(status N)", connection failure "Couldn't reach ...". Keys and tokens never
appear in errors.
