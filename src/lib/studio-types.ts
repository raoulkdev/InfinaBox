// Phase A contract types (frozen — see
// docs/superpowers/plans/2026-09-25-phase-a-foundations.md, "Wave 0
// contracts"). Each mirrors a serde type in crates/core or src-tauri; change
// both sides together, and only through the plan's lead.

// --- Agent (crates/core/src/agent/types.rs) ---

export type AgentErrorKind =
  | "not_installed"
  | "not_authenticated"
  | "rate_limited"
  | "process_failed"
  | "other";

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
  | { type: "error"; kind: AgentErrorKind; message: string };

export interface RuntimeStatus {
  name: string;
  installed: boolean;
  version: string | null;
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
  | { kind: "user"; text: string; at: number }
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
