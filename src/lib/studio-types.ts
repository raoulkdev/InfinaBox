// Contract types (frozen — see the "Wave 0 contracts" sections of
// docs/superpowers/plans/2026-09-25-phase-a-foundations.md and
// docs/superpowers/plans/2026-09-28-phase-b-first-run.md). Each mirrors a
// serde type in crates/core or src-tauri; change both sides together, and
// only through the plan's lead.

// --- Agent (crates/core/src/agent/types.rs) ---

export type AgentErrorKind =
  | "not_installed"
  | "not_authenticated"
  | "rate_limited"
  | "process_failed"
  | "other"
  /** The person pressed Stop: a neutral note, not an error. */
  | "cancelled";

export interface Usage {
  input_tokens: number | null;
  output_tokens: number | null;
}

export type AgentEvent =
  | { type: "session_started"; provider_session_id: string; model: string | null }
  | { type: "assistant_text"; text: string }
  | { type: "tool_use"; id: string; name: string; summary: string }
  | { type: "tool_result"; id: string; ok: boolean; summary: string }
  | { type: "files_changed"; paths: string[] }
  | { type: "turn_completed"; is_error: boolean; duration_ms: number | null; usage: Usage | null }
  | { type: "error"; kind: AgentErrorKind; message: string }
  /** The agent proposed a plan and is waiting for the person to approve it. */
  | { type: "plan_proposed"; title: string; steps: string[] };

/** Why a message was sent. */
export type MessageOrigin = "user" | "plan_approval" | "auto_fix" | "first_build";

export type PlanPolicy = "always_plan" | "small_changes_direct";

export interface RuntimeStatus {
  name: string;
  installed: boolean;
  version: string | null;
  /** From the CLI's own sign-in status; null when it can't tell. */
  logged_in: boolean | null;
}

// --- Chat store (crates/core/src/chat_store.rs) ---

export interface ThreadSummary {
  id: string;
  title: string;
  /** Unix seconds. */
  created_at: number;
  provider: string;
  provider_session_id: string | null;
}

export type ChatRecord =
  | { kind: "user"; text: string; at: number; origin?: MessageOrigin }
  | { kind: "event"; event: AgentEvent; at: number };

export interface LoadedThread {
  thread: ThreadSummary;
  records: ChatRecord[];
}

// --- Godot (crates/core/src/godot/types.rs) ---

export interface GodotStatus {
  installed: boolean;
  version: string | null;
  path: string | null;
  managed: boolean;
}

export type GameState = "stopped" | "starting" | "running" | "crashed";

export type OutputStream = "stdout" | "stderr";

export interface GameOutputLine {
  stream: OutputStream;
  text: string;
}

export interface GameError {
  message: string;
  file: string | null;
  line: number | null;
  raw: string;
}

export interface InstallProgress {
  downloaded_bytes: number;
  total_bytes: number | null;
  phase: string;
}

// --- Snapshots (crates/core/src/snapshot.rs) ---

export interface Snapshot {
  /** Commit sha. */
  id: string;
  title: string;
  /** Unix seconds. */
  timestamp: number;
  thread_id: string | null;
  turn: number | null;
  files_changed: number;
}

// --- Event payloads (src-tauri/src/commands/*.rs) ---

export interface AgentEventPayload {
  threadId: string;
  event: AgentEvent;
}

export interface TurnFinishedPayload {
  threadId: string;
  snapshot: Snapshot | null;
}

export interface GameStatePayload {
  state: GameState;
}

// --- Phase B: connect your AI (crates/core/src/connect.rs, app_settings.rs) ---

export type ProviderId =
  | "claude-code"
  | "codex"
  // Phase C (task X3 adds them on the Rust side).
  | "anthropic-api"
  | "openai-api"
  | "local-model";

export interface ProviderInfo {
  id: ProviderId;
  name: string;
  installed: boolean;
  version: string | null;
  /** From the CLI's own sign-in status; null when it can't tell. */
  logged_in: boolean | null;
  /** Who it suits and what it needs, in plain words. */
  blurb: string;
  /** The exact command the installer runs, shown before it runs. */
  install_command: string | null;
  /** Why it can't be installed from here (e.g. needs Node.js), if so. */
  install_blocker: string | null;
  login_command: string;
  docs_url: string;
}

export interface AppSettings {
  ai_provider: ProviderId | null;
  /** A Godot binary chosen in Settings instead of the managed one. */
  godot_path: string | null;
  first_run_done: boolean;
  /** Per-provider model settings, keyed by provider id. Never holds a key. */
  models: Record<string, ModelConfig>;
}

export interface ModelConfig {
  /** Up to and including /v1 for OpenAI-compatible servers. */
  base_url: string | null;
  model: string | null;
}

/** A real one-line test turn ("say hello"). */
export interface ConnectionTest {
  ok: boolean;
  reply: string | null;
  error_kind: AgentErrorKind | null;
  message: string | null;
  duration_ms: number;
}

export type ConnectAction = "install" | "login";

// --- Phase B: project settings (crates/core/src/project_settings.rs) ---

export interface ProjectSettings {
  plan_policy: PlanPolicy;
  /** "Teach me": explanations grow into short lessons. */
  teach: boolean;
  /** Send the running game's errors back to the AI automatically. */
  auto_fix: boolean;
}

// --- Phase B: onboarding (crates/core/src/onboarding.rs, scaffold.rs) ---

export interface TemplateInfo {
  id: string;
  name: string;
  description: string;
  /** "2d" or "3d". */
  dimension: string;
  controls: string;
  features: string[];
  keywords: string[];
}

export interface InterviewAnswers {
  idea: string;
  /** A template id, or "other". */
  genre: string;
  genre_other: string | null;
  feel: string[];
  look: string;
  references: string;
  session_length: string;
  name: string;
}

export interface TemplateChoice {
  template_id: string;
  reason: string;
}

export interface OnboardingPreview {
  choice: TemplateChoice;
  project_name: string;
  /** Context cards that will be written, relative to .ibproject/context/. */
  cards: string[];
  first_build_steps: string[];
}

export interface CreatedProject {
  path: string;
  thread_id: string;
  first_build_message: string;
}

/** A turn the app starts as soon as Studio shows the project (the
 * onboarding's first build). Frontend-only. */
export interface PendingTurn {
  threadId: string;
  message: string;
  origin: MessageOrigin;
  /** The specialist to ask as (the journey's "Ask the Producer"). */
  role?: Role;
}

// --- Phase B event payloads ---

export interface ConnectOutputPayload {
  data: string;
}

export interface ConnectExitPayload {
  action: ConnectAction;
  provider: ProviderId;
  success: boolean;
  code: number | null;
}

export type AutoFixPhase = "fixing" | "gave_up" | "idle";

export interface AutoFixStatePayload {
  projectPath: string;
  threadId: string;
  state: AutoFixPhase;
  attempt: number;
  maxAttempts: number;
}

// --- Phase C: roles (crates/core/src/agent/types.rs) ---

/** How hard the AI thinks about a message. Not every AI offers every level. */
export type Effort = "low" | "medium" | "high" | "xhigh" | "max";

export type Role =
  | "director"
  | "designer"
  | "programmer"
  | "artist"
  | "sound"
  | "qa"
  | "producer"
  | "marketer";

// --- Phase C: Context cards (crates/core/src/context_cards.rs) ---

export type CardType =
  | "concept"
  | "mechanic"
  | "character"
  | "level"
  | "story"
  | "asset"
  | "style-guide"
  | "task"
  | "playtest"
  | "other";

export interface CardMeta {
  type: CardType | null;
  title: string | null;
  status: string | null;
  /** Paths of other cards, relative to the context folder. */
  links: string[];
  /** Project files that implement this card. */
  implemented_in: string[];
  tags: string[];
  /** Every other front-matter key, kept across rewrites. */
  extra: Record<string, string[]>;
}

export interface Card {
  /** Relative to the context folder, "/"-separated. */
  path: string;
  meta: CardMeta;
  body: string;
  /** The raw header text when it couldn't be read; meta is then empty. */
  header_error: string | null;
}

export interface CardSummary {
  path: string;
  card_type: CardType;
  title: string;
  status: string | null;
  links: string[];
  broken_links: string[];
  backlinks: string[];
  /** The page's icon (an emoji), if it has one. */
  icon: string | null;
}

export interface BoardColumn {
  status: string;
  cards: CardSummary[];
}

export interface Board {
  columns: BoardColumn[];
}

export interface LinkEdge {
  from: string;
  to: string;
  broken: boolean;
}

export interface LinkGraph {
  nodes: CardSummary[];
  edges: LinkEdge[];
}

// --- Phase C: assets (crates/core/src/assets.rs, library/) ---

export type AssetKind = "image" | "audio" | "model3d" | "font" | "other";

export interface LicenseInfo {
  /** SPDX-style when known ("CC0-1.0"), else what the pack said. */
  name: string | null;
  /** "Poly Haven", "My own files: <pack>", "Generated". */
  source: string | null;
  author: string | null;
  url: string | null;
  /** Provider and model when generated. */
  generated_by: string | null;
}

export interface AssetInfo {
  /** Project-relative, "/"-separated. */
  path: string;
  kind: AssetKind;
  size_bytes: number;
  width: number | null;
  height: number | null;
  license: LicenseInfo | null;
  /** The Asset card's path relative to the context folder. */
  card: string | null;
  used_by: string[];
}

export interface MissingRef {
  from: string;
  to: string;
}

export interface Oversized {
  path: string;
  width: number | null;
  height: number | null;
  size_bytes: number;
}

export interface HealthReport {
  unused: string[];
  missing_refs: MissingRef[];
  oversized: Oversized[];
  unlicensed: string[];
}

export interface FilePayload {
  mime: string;
  base64: string;
  /** True when the file was over the cap and base64 is empty. */
  truncated: boolean;
  size: number;
}

export interface LibraryProviderInfo {
  id: string;
  name: string;
  blurb: string;
  /** True when "search" takes a folder path (My own files). */
  needs_folder: boolean;
  site_url: string | null;
}

export interface LibraryQuery {
  text: string;
  kind: AssetKind | null;
  limit: number | null;
}

export interface LibraryItem {
  provider: string;
  id: string;
  title: string;
  kind: AssetKind;
  license: LicenseInfo;
  thumbnail_url: string | null;
  page_url: string | null;
}

// --- Phase C: generation and credentials (crates/core/src/generate/, secrets.rs) ---

export type GenKind = "image" | "voice" | "sfx" | "music";

export type SecretName =
  | "cloudflare_account_id"
  | "cloudflare_api_token"
  | "fish_audio_api_key"
  | "eleven_labs_api_key"
  | "anthropic_api_key"
  | "open_ai_api_key";

export interface GenOptions {
  model: string | null;
  width: number | null;
  height: number | null;
  seed: number | null;
  pixel_art: boolean;
  transparent: boolean;
  duration_seconds: number | null;
  voice_id: string | null;
  looping: boolean;
}

export interface GenRequest {
  kind: GenKind;
  prompt: string;
  options: GenOptions;
  /** The Style Guide card's text, appended to the prompt. */
  style_guide: string | null;
}

export interface GenProviderInfo {
  id: string;
  name: string;
  kinds: GenKind[];
  needs: SecretName[];
  blurb: string;
  signup_url: string;
  /** Whether every secret in `needs` is set (from the keychain). */
  connected: boolean;
}

/** A generated file waiting to be accepted or discarded. */
export interface GenPreview {
  temp_id: string;
  mime: string;
  base64: string;
  extension: string;
  provider: string;
  model: string;
  prompt_used: string;
  duration_ms: number;
}

export interface CredentialStatus {
  name: SecretName;
  connected: boolean;
}

// --- Phase C: the Producer journey (crates/core/src/producer.rs) ---

export type Stage = "idea" | "prototype" | "vertical_slice" | "alpha" | "beta" | "launch";

export type Signal = "auto" | "manual";

export interface Criterion {
  id: string;
  title: string;
  signal: Signal;
  done: boolean;
  /** What was found, in plain words, or why it isn't done. */
  evidence: string;
}

export interface StageStatus {
  stage: Stage;
  title: string;
  criteria: Criterion[];
  complete: boolean;
}

export interface NextStep {
  criterion_id: string;
  title: string;
  /** A ready-to-send message for the Producer role. */
  ask: string;
}

export interface Journey {
  current: Stage;
  stages: StageStatus[];
  next_step: NextStep | null;
}
