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

export type ProviderId = "claude-code" | "codex";

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
  /** A Godot binary chosen in Advanced settings instead of the managed one. */
  godot_path: string | null;
  first_run_done: boolean;
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
