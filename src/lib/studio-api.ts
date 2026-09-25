import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  AgentEventPayload,
  GameError,
  GameOutputLine,
  GameState,
  GameStatePayload,
  GodotStatus,
  InstallProgress,
  LoadedThread,
  RuntimeStatus,
  Snapshot,
  ThreadSummary,
  TurnFinishedPayload,
} from "@/lib/studio-types";

// Typed wrappers for every Phase A Studio command and event (frozen
// contract — see the Phase A plan). Studio components call these, never raw
// `invoke`/`listen`, so a name or argument typo is caught here once instead
// of failing silently at runtime (Tauri's invoke has no compile-time
// checking).

// --- Agent + chat ---

export const agentStatus = () => invoke<RuntimeStatus>("agent_status");

/** Resolves as soon as the turn has started; everything that happens in it
 * arrives through `onAgentEvent`, then `onAgentTurnFinished`. */
export const agentSend = (projectPath: string, threadId: string, message: string) =>
  invoke<void>("agent_send", { projectPath, threadId, message });

export const agentCancel = (threadId: string) => invoke<void>("agent_cancel", { threadId });

export const chatCreateThread = (projectPath: string, title: string) =>
  invoke<ThreadSummary>("chat_create_thread", { projectPath, title });

export const chatListThreads = (projectPath: string) =>
  invoke<ThreadSummary[]>("chat_list_threads", { projectPath });

export const chatLoadThread = (projectPath: string, threadId: string) =>
  invoke<LoadedThread>("chat_load_thread", { projectPath, threadId });

// --- Godot + game ---

export const godotStatus = () => invoke<GodotStatus>("godot_status");

/** Progress arrives through `onGodotInstallProgress`. */
export const godotInstall = () => invoke<GodotStatus>("godot_install");

export const gameRun = (projectPath: string) => invoke<void>("game_run", { projectPath });

export const gameStop = () => invoke<void>("game_stop");

/** The game's current state (the `game-state` event only reports changes). */
export const gameStatus = () => invoke<GameState>("game_status");

export const gameRecentErrors = (limit: number) =>
  invoke<GameError[]>("game_recent_errors", { limit });

// --- Snapshots ---

export const snapshotList = (projectPath: string, limit: number) =>
  invoke<Snapshot[]>("snapshot_list", { projectPath, limit });

export const snapshotCreate = (projectPath: string, title: string) =>
  invoke<Snapshot | null>("snapshot_create", { projectPath, title });

export const snapshotRestore = (projectPath: string, snapshotId: string) =>
  invoke<Snapshot>("snapshot_restore", { projectPath, snapshotId });

export const snapshotUndoLast = (projectPath: string) =>
  invoke<Snapshot | null>("snapshot_undo_last", { projectPath });

// --- Project ---

/** Resolves to the new project's path. */
export const projectCreate = (parentDir: string, name: string) =>
  invoke<string>("project_create", { parentDir, name });

// --- Events ---
// Same subscribe/unsubscribe shape as `onProjectFilesChanged` in
// `fs-watch.ts`: returns a plain cleanup function for a `useEffect`.

function subscribe<T>(event: string, callback: (payload: T) => void): () => void {
  let cancelled = false;
  const unlistenPromise = listen<T>(event, (e) => {
    if (!cancelled) callback(e.payload);
  });
  return () => {
    cancelled = true;
    void unlistenPromise.then((unlisten) => unlisten());
  };
}

export const onAgentEvent = (cb: (p: AgentEventPayload) => void) => subscribe("agent-event", cb);

export const onAgentTurnFinished = (cb: (p: TurnFinishedPayload) => void) =>
  subscribe("agent-turn-finished", cb);

export const onGodotInstallProgress = (cb: (p: InstallProgress) => void) =>
  subscribe("godot-install-progress", cb);

export const onGameState = (cb: (p: GameStatePayload) => void) => subscribe("game-state", cb);

export const onGameOutput = (cb: (p: GameOutputLine) => void) => subscribe("game-output", cb);

export const onGameError = (cb: (p: GameError) => void) => subscribe("game-error", cb);

export const onSnapshotsChanged = (cb: () => void) => subscribe<null>("snapshots-changed", () => cb());
