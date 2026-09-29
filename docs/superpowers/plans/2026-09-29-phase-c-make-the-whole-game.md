# Phase C: Make the whole game — implementation plan

> **For agentic workers:** built the same way as Phases A and B — **subagents develop multiple tasks at the same time**, each in its own git worktree, against contracts the lead freezes first (Wave 0). The lead coordinates, reviews (with reviewer subagents), merges and verifies; it does not implement Wave 1 or Wave 2 tasks itself.

**Goal:** a person takes a prototype to a complete short game (start screen → gameplay → ending) inside InfinaBox (spec §14, Phase C exit): the AI works from a full, typed Context; assets come from free libraries, the person's own files, or their own generation accounts, with licenses tracked; a Producer journey says what to do next; more templates (including 3D) and more ways to connect an AI (API key, local model).

**Spec:** `docs/superpowers/specs/2026-09-25-ai-game-studio-product-spec.md` (§7.1–7.4, §8, §9, §12, §14 Phase C). **Builds on:** `docs/superpowers/plans/2026-09-28-phase-b-first-run.md` (its execution model and Global constraints apply unchanged and are included in every subagent prompt).

## Status

- 2026-09-29: plan written; Wave 0 done (contracts, stubs, template skeletons, dependencies `image`, `base64`, `keyring` 3, `tiny_http`, `three`). Subagent rules: `phase-c-subagent-rules.md` in the session scratchpad.

## What can and can't be verified here (read this first)

The build machine can reach **only `api.anthropic.com`** (everything else is blocked by its proxy), and holds no accounts for Cloudflare, Fish Audio, ElevenLabs, OpenAI, Poly Haven or any asset library, and no Ollama. So:

- Every network integration (asset libraries, image/voice/sound/music generation, OpenAI-compatible and Anthropic API runtimes) is built against a **local fake server** written to the provider's documented request/response shape, with the shape written down in the code and in the fixtures README. It is tested end to end against that fake. It has **not** been tried against the real service, and the docs, UI and final report say so.
- The Anthropic API runtime can be tried for real only if an API key is available (none is; the machine uses a subscription login). It stays unverified against the real API.
- Provider base URLs can be overridden with environment variables (`INFINABOX_CLOUDFLARE_BASE`, `INFINABOX_FISHAUDIO_BASE`, `INFINABOX_ELEVENLABS_BASE`, `INFINABOX_POLYHAVEN_BASE`, …) so the end-to-end tests point the real app at fakes. These overrides are for tests only and are documented as such.
- The OS keychain: macOS (Keychain) and Windows (Credential Manager) are not exercised here; Linux uses the Secret Service, which isn't running in this container, so keychain code is tested through a `SecretStore` trait with an in-memory fake, and the real keyring binding is compile-checked only.

## Scope

**In Phase C:**
1. **Full Context** (spec §7.2): typed cards (parse/write front-matter, links, backlinks, broken links), a Board view (Tasks/Bugs by status, drag between columns), a Map view of card links, a card editor, filters; the MCP `list_context_cards` returns each card's type, title and status. **Roles** (§7.1): Designer, Programmer, Artist, Sound, QA, Producer, Marketer (plus the Director default) as prompt sections chosen per message from the chat composer.
2. **Assets** (§7.3): a real Assets section: project assets with kind/size/license, preview (images and sprite sheets with animation, audio with waveform and playback, 3D models with an orbit viewer), import from a file/folder or a free library, license tracking as Asset cards, a generated credits page, health checks (unused, missing references, oversized, unlicensed). Library search through a `LibraryProvider` interface with **Poly Haven** (documented public API) and **My own files** (a folder the person downloaded from Kenney, Quaternius, OpenGameArt, …: InfinaBox reads the license file that packs ship with and asks otherwise).
3. **Generation with the person's own accounts** (§7.3): Cloudflare Workers AI images, Fish Audio voice, ElevenLabs sound effects and music; keys only in the OS keychain; the Style Guide card is added to every prompt; results are previewed, then accepted (imported with a license record saying it was generated) or discarded; images get simple post-processing (resize, palette snap for pixel art, background clean-up by corner flood fill).
4. **Producer journey** (§6.3): stages Idea → Prototype → Vertical Slice → Alpha → Beta → Launch with checklists whose items are checked only by a real signal (files, cards, snapshots, license records) or by the person; a "next best step" and an "Ask the Producer" button.
5. **Templates** (§7.4, §12): two more 2D (`puzzle-2d`, `visualnovel-2d`) and the first two 3D (`explorer-3d`, `firstperson-3d`); the interview offers 2D and 3D; every template boots in real Godot with zero errors.
6. **More runtimes** (§8.1): `anthropic-api` (Anthropic Messages API), `openai-api` (OpenAI-compatible chat completions), `local-model` (any OpenAI-compatible endpoint such as Ollama or LM Studio), all running InfinaBox's own tool loop with the same InfinaBox MCP tools plus sandboxed file tools; keys in the keychain.

**Not in Phase C:** Phase D (playtest sharing, release builds, publish, launch kit, accounts/Stripe), GitHub backup, embedding the game window, real background-removal models, video, Steam.

## Decisions made for this phase (the user delegated these)

| Question | Decision | Why |
|---|---|---|
| Which extra templates? | 2D: Puzzle (push-block) and Visual novel. 3D: Third-person explorer and First-person. | Puzzle and visual novel need little art and make "start → play → ending" easy to finish; explorer and first-person are the two 3D genres novices ask for, with fixed rigs the AI only customizes. |
| Which asset libraries? | Poly Haven (real public API, CC0) + "My own files" (folder import). Kenney, Quaternius, OpenGameArt are reached through the second (their packs ship a license file). | Poly Haven is the one with a documented API; the others have no stable search API, and scraping their sites would be fragile and possibly against their terms. |
| Where do license records live? | As **Asset cards** (`.ibproject/context/assets/<slug>.md`, front-matter `type: asset`, `file`, `license`, `source`, `author`, `url`, `generated_by`). No separate JSON. | Spec §7.2/§7.3: one model of the game, readable by the AI, committed with the project. |
| Card front-matter | Small YAML subset: `key: value`, `key: [a, b]`, `key:` + `- item` lists, double-quoted strings. Unknown keys are preserved on rewrite. The parser and writer are ours (no YAML dependency). | Cards are written by the AI and by us; a tiny strict subset is testable and round-trips. Anything the parser can't read is shown as "couldn't read this card's header" with the raw text, never dropped. |
| Links | `links: [mechanics/double-jump.md, ...]` (paths relative to the context folder). Backlinks and broken links are computed, not stored. | Diffable and simple. |
| Roles | A role changes the prompt (a role section after the Director prompt), not the tool set. `TurnOptions.role`, chosen in the composer ("Ask as: Auto / Designer / …"); Auto = Director. | Spec: "system prompts + tool sets over the same runtime"; the tool set stays whole because splitting it adds failure modes with no user benefit yet. Recorded as a deliberate simplification. |
| Preview transport | Assets are read through `assets_read_base64` (size-capped) rather than enabling Tauri's asset protocol. | No capability/CSP change; fine for the file sizes previewed (cap 25 MB, larger shows "too large to preview"). |
| 3D viewer | `three` (+ `GLTFLoader`, `OrbitControls`), loaded lazily. | The standard, one dependency. glTF/GLB only. |
| Secret storage | `keyring` 3 (macOS Keychain, Windows Credential Manager, Linux Secret Service, dbus vendored) behind a `SecretStore` trait. Values never leave the backend: commands only say connected/not connected. Values pass through `redact.rs` patterns before anything is logged or written. | Spec §7.3 and §8.1. |
| Runtime tool loop | InfinaBox runs the loop: model → tool calls → tool results → model. Tools: `read_file`, `write_file`, `edit_file` (exact-string replace), `list_files`, `search_files`, all sandboxed to the project; plus every tool of the InfinaBox MCP server through the existing MCP client. Writes to `.git`, `addons/infinabox`, `.ibproject/.ibx`, `.ibproject/chat` are refused. | Same restrictions as the CLI runtimes (`--tools Read,Edit,Write,Glob,Grep`). |
| Streaming | The API runtimes call the provider non-streaming, one completion per loop step, and emit `AssistantText`/`ToolUse`/`ToolResult` events between steps. | Simple, testable against fakes; token-by-token streaming is a later polish. |
| Loop limits | At most 40 model calls per turn, 200 KB per tool result (truncated with a note), 5-minute wall clock per model call. | Runaway protection. |
| Producer signals | Automatic where a real signal exists (see the Producer task); everything else is a manual tick the person can toggle. Never guessed. | Spec §6.3, principle 1. |

## Global constraints

Phase B's "Global constraints" section applies verbatim (which includes Phase A's: real not fabricated, match the codebase, testable logic in `crates/core`, tests use real things, verify external tools, never destroy user work, async Tauri commands, stay in your lane, the disk rules). Additions:

- **No live network in tests.** Provider tests run against a local fake HTTP server on 127.0.0.1 written from the provider's documented API shape; the shape (endpoint, headers, body, response) is stated in a comment at the top of each provider and in `crates/core/tests/fixtures/providers/README.md`. Say plainly in reports what is unverified.
- **Credentials**: never in project files, chat history, settings JSON, logs, command lines or test fixtures. Keychain only; tests use the in-memory store and obviously fake keys (`test-key-123`).
- **Frontend**: honest empty and error states (nothing invented); every backend error shown as its plain text; shadcn/ui and motion conventions; layouts checked at 1024×640 and 1280×800 in a browser with a stubbed `__TAURI_INTERNALS__` (stub never committed).
- **Real tools available**: Godot 4.7.2 at `/tmp/claude-0/godot/Godot_v4.7.2-stable_linux.x86_64`; a logged-in `claude` CLI; `codex` at `/tmp/claude-0/codex/node_modules/.bin/codex`. Nothing else is reachable on the network except `api.anthropic.com`.

## Execution model

Same roles and process as Phase B: lead (Wave 0, merges, Wave 3), one implementer subagent per task in its own worktree, a reviewer subagent per task before merge, findings sent back to the same implementer. Because rate limits interrupted Phase B repeatedly, no more than about 9 subagents run at once; an interrupted one is resumed with SendMessage; the lead checks in hourly.

```
Wave 0  (lead)            contracts, stubs, template skeletons, new dependencies, this plan
Wave 1A (backend, 9)      CC context cards · RO roles · AS assets · AL libraries · GN generation providers
                          SK secrets · RL runtime loop · RB runtime backends · PJ producer
Wave 1B (templates, 4)    T4 puzzle-2d · T5 visualnovel-2d · T6 explorer-3d · T7 firstperson-3d
Wave 1C (frontend, 4)     FX Context views · FA Assets section · FP Home journey + role picker · FK connect: API/local + credentials
Wave 2  (commands, 4)     X1 context+roles+journey commands · X2 assets+library+generate+credentials commands
                          X3 runtimes wiring (connect, agent, settings) · X4 onboarding for 2D/3D + templates
Wave 3  (lead + fix-ups)  integrate; E2E against the real app with fake providers; docs
```

Merge order, Wave 1: SK, CC, RO, AS, AL, GN, RL, RB, PJ, T4–T7, FX, FA, FP, FK. Wave 2: X1, X2, X3, X4.

### File ownership

| Task | Owns |
|---|---|
| Lead (W0) | every contract/stub file below; `Cargo.toml`s, `package.json`; `lib.rs`/`mod.rs` registrations; `src/lib/studio-types.ts`, `studio-api.ts`; `agent/types.rs`; `scaffold.rs` template registration |
| CC | `crates/core/src/context_cards.rs`, `crates/mcp-server/src/context.rs`, `crates/mcp-server/src/server.rs` (`list_context_cards` output only) |
| RO | `crates/core/src/agent/prompt.rs`, `crates/core/src/agent/prompts/roles/**` |
| AS | `crates/core/src/assets.rs` |
| AL | `crates/core/src/library/**` |
| GN | `crates/core/src/generate/**` |
| SK | `crates/core/src/secrets.rs`, `crates/core/src/redact.rs` |
| RL | `crates/core/src/agent/api/{mod,loop_,tools,mcp}.rs` |
| RB | `crates/core/src/agent/api/{openai,anthropic}.rs` |
| PJ | `crates/core/src/producer.rs` |
| T4 / T5 / T6 / T7 | `templates/puzzle-2d/**` / `templates/visualnovel-2d/**` / `templates/explorer-3d/**` / `templates/firstperson-3d/**` |
| FX | `src/components/context/**` |
| FA | `src/components/assets/**` |
| FP | `src/components/journey/**`, `src/components/studio/chat/RolePicker.tsx` (new file only; the lead wires it) |
| FK | `src/components/connect/**` |
| X1 | `src-tauri/src/commands/{context,journey}.rs`, `agent.rs` (role only) |
| X2 | `src-tauri/src/commands/{assets,library,generate,credentials}.rs` |
| X3 | `src-tauri/src/commands/{connect,settings}.rs`, `agent.rs` (runtime choice only), `crates/core/src/{connect,app_settings}.rs` |
| X4 | `src-tauri/src/commands/onboarding.rs`, `crates/core/src/{onboarding,scaffold}.rs`, `src/components/onboarding/**`, `src/components/cockpit/DashboardSection.tsx` |

## Wave 0 contracts

Frozen for Waves 1–2. Rust and TypeScript mirror each other (serde `snake_case`; TS fields snake_case; event payloads camelCase). The Rust files listed below contain exactly these types and signatures, with bodies that return "not implemented yet"; a task fills in the bodies and may add private helpers and tests but not change public signatures.

### Context (`crates/core/src/context_cards.rs`)
Types `CardType` (`concept, mechanic, character, level, story, asset, style-guide, task, playtest, other`), `CardMeta`, `Card`, `CardSummary`, `Board`/`BoardColumn`, `LinkGraph`, functions `list_cards`, `read_card`, `write_card`, `set_status`, `board`, `graph`. See the file.

### Roles (`agent/types.rs`)
`Role { Director, Designer, Programmer, Artist, Sound, Qa, Producer, Marketer }` (default Director); `TurnOptions.role`; `prompt::system_prompt` adds `prompts/roles/<role>.md` after the origin section for any role but Director.

### Assets (`assets.rs`) and libraries (`library/`)
`AssetKind`, `LicenseInfo`, `AssetInfo`, `HealthReport`, `scan`, `import_file`, `health`, `credits`, `read_base64`; `LibraryProvider` trait, `LibraryItem`, `LibraryQuery`, providers `polyhaven` and `local_folder`.

### Generation (`generate/`)
`GenKind`, `GenOptions`, `GenRequest`, `GenResult`, `Generator` trait, `cloudflare`, `fishaudio`, `elevenlabs`, `postprocess`, and `run(provider, request, secrets) -> GenResult`.

### Secrets (`secrets.rs`)
`SecretName`, `SecretStore` trait, `KeychainStore`, `MemoryStore`.

### Runtimes (`agent/api/`, `connect.rs`, `app_settings.rs`)
`ChatBackend` trait, `Message`, `ToolSpec`, `ToolCall`, `Completion`; `ApiRuntime`; `openai::OpenAiCompatible`, `anthropic::AnthropicMessages`. On the TypeScript side `ProviderId` already includes `anthropic-api`, `openai-api` and `local-model`; the Rust enum gains `AnthropicApi`, `OpenAiApi`, `LocalModel` in task X3 (it owns every match on `ProviderId`). `AppSettings.models: BTreeMap<String, ModelConfig { base_url, model }>` keyed by provider string.

### Producer (`producer.rs`)
`Stage`, `Criterion`, `StageStatus`, `Journey`, `NextStep`, `compute`, `set_manual`.

### Tauri commands (stubs in Wave 0; bodies in Wave 2)
| Command | Args | Returns | Task |
|---|---|---|---|
| `context_list` | `projectPath` | `CardSummary[]` | X1 |
| `context_read` | `projectPath, path` | `Card` | X1 |
| `context_write` | `projectPath, path, meta, body` | `Card` | X1 |
| `context_set_status` | `projectPath, path, status` | `Card` | X1 |
| `context_board` | `projectPath, types` | `Board` | X1 |
| `context_graph` | `projectPath` | `LinkGraph` | X1 |
| `journey_get` | `projectPath` | `Journey` | X1 |
| `journey_set_manual` | `projectPath, id, done` | `Journey` | X1 |
| `agent_send` (changed) | `…, origin?, role?` | `()` | X1 |
| `assets_scan` | `projectPath` | `AssetInfo[]` | X2 |
| `assets_import` | `projectPath, source, destSubdir?, title, license` | `AssetInfo` | X2 |
| `assets_health` | `projectPath` | `HealthReport` | X2 |
| `assets_credits` | `projectPath` | `string` (Markdown) | X2 |
| `assets_read_base64` | `projectPath, path, maxBytes?` | `FilePayload { mime, base64, truncated, size }` | X2 |
| `library_providers` | — | `LibraryProviderInfo[]` | X2 |
| `library_search` | `provider, query` | `LibraryItem[]` | X2 |
| `library_import` | `projectPath, provider, item, destSubdir?` | `AssetInfo[]` | X2 |
| `generate_providers` | — | `GenProviderInfo[]` (each with `connected`) | X2 |
| `generate_run` | `projectPath, request` | `GenPreview { temp_id, mime, base64, extension, provider, model, prompt_used, duration_ms }` | X2 |
| `generate_accept` | `projectPath, tempId, title, destSubdir?` | `AssetInfo` | X2 |
| `generate_discard` | `tempId` | `()` | X2 |
| `credential_status` | `names` | `{ name, connected }[]` | X2 |
| `credential_set` | `name, value` | `()` | X2 |
| `credential_clear` | `name` | `()` | X2 |

X3 changes the meaning of `ai_providers`, `ai_recommended`, `ai_test_connection`, `app_settings_*`, and `agent_status` (they cover the three new providers). X4 changes `onboarding_*` for 2D/3D.

### Frontend contract
`studio-types.ts` and `studio-api.ts` hold every type, wrapper and event above. Component props:
- `ContextSection` `{ projectPath }` (unchanged); `AssetsSection` `{ projectPath }` (new, replaces the placeholder in `App.tsx`; the lead wires it); `JourneyPanel` `{ projectPath, onAskProducer: (message: string) => void }`; `RolePicker` `{ value: Role; onChange: (r: Role) => void; disabled?: boolean }`.

## Wave 1 tasks
(Each subagent prompt carries its section, the rules file, and this plan's contracts, decisions and ownership.)

### CC: Context cards
Implement `context_cards.rs` per the file: the front-matter subset parser and writer (round-trips, preserves unknown keys and body byte-for-byte), listing with resolved links/backlinks/broken links, `write_card` (atomic, refuses paths outside the context folder like `context.rs` does), `set_status`, `board` (columns in the order todo, doing, done, then any other status found; cards without status go in `todo`), `graph`. `crates/mcp-server/src/context.rs` and `server.rs`: `list_context_cards` returns for each card its path, type, title, status (plain text lines the agent can read), unchanged for cards without front-matter. Tests over real temp projects including every template's real cards (`templates/*/.ibproject/context/**`).

### RO: Roles
Seven role prompt files under `prompts/roles/` (each a short, plain section: what the role cares about, which Context cards it reads, how it hands off; Producer: reads the journey and proposes the next step; QA: writes and runs checks, files Task/Bug cards; Marketer: store copy, devlog drafts). `system_prompt` includes the role section for non-Director roles, after origin and before explain/teach. Tests for every role.

### AS: Assets core
`assets.rs`: `scan` (by extension; image dimensions via the `image` crate; audio/model sizes), Asset-card-backed licenses (`license_for`), `import_file` (copy into `assets/<kind-folder>/`, unique names, refuse overwriting, write the Asset card via `context_cards`-compatible front-matter written directly so AS doesn't depend on CC), `health` (unused: no `res://` reference to the file in any `.tscn/.tres/.gd/.godot` file; missing references: `res://` paths in those files that don't exist; oversized: images over 4096 px or files over 20 MB; unlicensed), `credits` (Markdown grouped by license), `read_base64` (size-capped, mime by extension, path-sandboxed). Tests with real temp projects and real Godot-shaped files (use the templates).

### AL: Libraries
`library/`: `LibraryProvider` trait, `polyhaven.rs` (documented API: `GET {base}/assets?t=<hdris|textures|models>` → JSON object keyed by id with `name`, `authors`, `categories`, `tags`; `GET {base}/files/{id}` → JSON of file URLs by resolution/format; assets are CC0; downloads a chosen resolution, glTF + textures for models), `local_folder.rs` (walks a folder, reads `LICENSE*`/`license.txt`/`License.txt` for the pack's license when present, otherwise items come back `license: unknown` and the import step asks). HTTP with `reqwest::blocking`, base URL from `INFINABOX_POLYHAVEN_BASE`, timeouts, size caps, no redirect to other hosts. A fake server (a small module inside `library/tests`) written from the documented shape. Tests.

### GN: Generation providers
`generate/`: Cloudflare Workers AI (`POST {base}/client/v4/accounts/{account}/ai/run/{model}`, `Authorization: Bearer <token>`, JSON `{prompt, ...}`; response either JSON `{"result": {"image": "<base64>"}, "success": true}` or raw image bytes depending on model; default model `@cf/black-forest-labs/flux-1-schnell`, configurable), Fish Audio (`POST {base}/v1/tts`, `Authorization: Bearer <key>`, JSON `{text, reference_id?, format: "mp3"}`, response audio bytes), ElevenLabs (sound effects `POST {base}/v1/sound-generation` with header `xi-api-key`, JSON `{text, duration_seconds?, prompt_influence?}` → audio bytes; music `POST {base}/v1/music` JSON `{prompt, music_length_ms}` → audio bytes). Each provider states its assumed shape in a header comment. Errors mapped to plain messages (bad key, rate limit, content refused, network); prompts get the Style Guide text appended when given; `postprocess.rs`: `resize`, `palette_snap` (median-cut palette of N colors), `clear_background` (flood fill from the four corners over near-uniform pixels → transparent), all with the `image` crate. Fake HTTP servers per provider in tests; credentials passed in by the caller (the trait takes `&dyn SecretStore`).

### SK: Secrets
`secrets.rs`: `SecretName`, `SecretStore`, `KeychainStore` (keyring 3, service `infinabox`), `MemoryStore`; never logs values; errors say "couldn't reach the system keychain" plainly. `redact.rs`: add patterns for Cloudflare API tokens, Fish Audio and ElevenLabs keys (`xi-…`/32-hex style, documented as heuristics), OpenAI `sk-…`/`sk-proj-…`, Anthropic `sk-ant-…` (check what is already there), with tests that the new patterns redact and don't over-redact ordinary text.

### RL: API runtime loop and tools
`agent/api/`: `ApiRuntime` implementing `AgentRuntime` over a `ChatBackend`: builds the system prompt with `prompt::system_prompt`, runs the loop (limits per the decision table), emits `SessionStarted`, `AssistantText`, `ToolUse`/`ToolResult`, `FilesChanged` (from the file tools), `PlanProposed` (from `propose_plan`), `TurnCompleted`, `Error` (auth, rate limit, network, `Cancelled` on `cancel`). No provider-side session resume: the runtime keeps the conversation by loading earlier `ChatRecord`s of the thread from `chat_store` (user text + assistant text + a one-line tool summary) into the first messages. `tools.rs`: the sandboxed file tools. `mcp.rs`: connects to the InfinaBox MCP server through `crate::mcp_client` (read it; extend within `agent/api/mcp.rs` if needed rather than editing it) using `TurnRequest.mcp`, lists its tools, forwards calls. Tests with a scripted fake `ChatBackend` and the real MCP server binary path (`infinabox-cli mcp-server`) where possible.

### RB: API backends
`agent/api/openai.rs`: OpenAI-compatible `POST {base}/chat/completions` (tools as `tools:[{type:"function",function:{name,description,parameters}}]`, results as `role:"tool"` messages, `tool_calls` with JSON-string `arguments`), optional bearer key, works for Ollama (`http://localhost:11434/v1`) and LM Studio; `anthropic.rs`: `POST https://api.anthropic.com/v1/messages` with `x-api-key`, `anthropic-version: 2023-06-01`, `tools` with `input_schema`, `tool_use`/`tool_result` content blocks, `system` string. Base URL overridable. Errors classified (401 → not authenticated, 429 → rate limited, connection refused → "couldn't reach the model at <url>"). Tests against local fake servers; the Anthropic one is also tried for real if `ANTHROPIC_API_KEY` happens to be set (an `#[ignore]` test) — report whether it ran.

### PJ: Producer
`producer.rs`: the journey (stages and criteria written out in the file with their automatic signal or "manual"), `compute` (reads the project: cards, assets/licenses, snapshots via git, the presence of the main scene, `.ibproject/journey.json` for manual ticks and for the last successful headless boot recorded by `record_boot(project, ok)` which X1 calls after a clean game run), `set_manual`, `next_step`. Every criterion has `evidence` text that states what was actually found ("3 mechanic cards"). Tests over real temp projects made from templates.

### T4–T7: Templates
Same brief as Phase B's template tasks (see that plan's T1–T3): boots in real Godot with zero errors, scripted play-test, screenshots viewed, AGENTS.md/CLAUDE.md, mechanic cards under `.ibproject/context/mechanics/` with the Phase C front-matter (`type`, `title`, `status`, `links`, `implemented_in`), `template.json` (`dimension` `"2d"` or `"3d"`), `.uid` files shipped, input map generated by Godot, no third-party assets.
- **T4 puzzle-2d:** push-block puzzle (Sokoban-style) with 5 short levels defined as text grids in one script, move counter, undo (Z), restart (R), level-complete and game-complete screens.
- **T5 visualnovel-2d:** dialogue scenes from a simple text script file (`story/*.txt` or a `.json`), characters as shapes/colored placeholders with names, choices that branch, an ending screen; a start screen.
- **T6 explorer-3d:** third-person character on a small island/level, orbit camera, jump, collectibles, a goal, primitive meshes and simple materials, a directional light, a start screen and an ending.
- **T7 firstperson-3d:** first-person walk/look (mouse look), a small maze or rooms, pick-up keys/doors or collectibles, a goal and an ending screen.
For 3D: Godot renders with the Compatibility renderer under the software GL here; verify `--headless` boot and take screenshots under `xvfb-run`.

### FX / FA / FP / FK: Frontend
- **FX Context:** tabs Cards (typed list, filter by type/status/search, card editor = front-matter form + `MarkdownEditor` body, New card by type), Board (Tasks/Bugs columns, drag a card to change status, New task/bug), Map (read-only ReactFlow of card links, broken links flagged), Graphs (existing). Uses `context_*` commands.
- **FA Assets:** `AssetsSection` with tabs Project (grid/list of assets, filters by kind, license badge, health summary and details, credits copy/open), Library (provider picker, search, results with license and author, Import), Generate (provider cards with "Connect" using `credential_*`, forms for image/voice/sound/music, the preview with Accept/Discard, style-guide notice), plus the Preview drawer (image with sprite-sheet frame slicing and animation controls, audio waveform + play/pause, glTF orbit viewer with `three`, lazily loaded).
- **FP Journey + roles:** `JourneyPanel` (stages, checklists with evidence, manual ticks, next step, "Ask the Producer") and `RolePicker`.
- **FK Connect:** the connect panel handles `anthropic-api`, `openai-api` and `local-model` (key entry stored through `credential_set`, model name, base URL for local, "Test", "Use this one"), never showing a saved key back.

## Wave 2 tasks

- **X1:** context, journey and role commands; `agent_send` gains `role`; call `producer::record_boot` after a clean game run.
- **X2:** assets, library, generate and credentials commands (temp store for previews under the app data dir, cleaned on accept/discard and at startup), `SecretStore` as managed state.
- **X3:** `connect.rs` detection/test/recommend for the three new providers, `AppSettings.models`, runtime selection in `agent.rs` (`ApiRuntime` with the right backend and key from the keychain), `agent_status`.
- **X4:** onboarding for 2D and 3D: `choose_template` over all templates, the interview offers 2D/3D (a question when the idea doesn't decide it), the "games start in 2D" note removed, keyword lists for the new templates, `DashboardSection` copy.

## Wave 3: integration
Merge, full verification after each merge; wire `AssetsSection`, `JourneyPanel` and `RolePicker` into `App.tsx`/`StudioSection`/Home; E2E scenarios with fake providers (fake servers started by the harness, base URLs passed by env): **context** (create/edit cards, board drag, map), **assets** (import a local file, license card, health, credits, previews rendered), **generate** (connect fake providers via the UI, generate image/voice/sfx/music, accept, asset appears with a "generated" license), **runtimes** (select Local model pointing at a fake OpenAI-compatible server scripted to call `write_file` then answer; the chat turn edits a file and is snapshotted), **templates** (each new template scaffolds, boots, and runs in the Play panel), **journey** (ticks and evidence), plus the earlier suites still green with the real AI. Docs: `CLAUDE.md`, this plan's status.

**Exit (engineering):** all of the above passes; **product exit** (a tester takes a prototype to a complete short game) needs real people and is recorded as not yet run.
