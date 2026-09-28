import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  AgentEventPayload,
  AppSettings,
  AutoFixStatePayload,
  ConnectAction,
  ConnectExitPayload,
  ConnectOutputPayload,
  ConnectionTest,
  CreatedProject,
  InterviewAnswers,
  MessageOrigin,
  OnboardingPreview,
  ProjectSettings,
  ProviderId,
  ProviderInfo,
  TemplateInfo,
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

// Typed wrappers for every Studio command and event (frozen contract — see
// the Phase A and Phase B plans). Studio components call these, never raw
// `invoke`/`listen`, so a name or argument typo is caught here once instead
// of failing silently at runtime (Tauri's invoke has no compile-time
// checking).

// --- Agent + chat ---

/** The selected AI's status. */
export const agentStatus = () => invoke<RuntimeStatus>("agent_status");

/** Resolves as soon as the turn has started; everything that happens in it
 * arrives through `onAgentEvent`, then `onAgentTurnFinished`. `origin`
 * defaults to "user". */
export const agentSend = (
  projectPath: string,
  threadId: string,
  message: string,
  origin?: MessageOrigin,
) => invoke<void>("agent_send", { projectPath, threadId, message, origin: origin ?? null });

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

// --- Connect your AI ---

export const aiProviders = () => invoke<ProviderInfo[]>("ai_providers");

export const aiRecommended = () => invoke<ProviderId>("ai_recommended");

/** A real one-line turn with the provider; takes a few seconds. */
export const aiTestConnection = (provider: ProviderId) =>
  invoke<ConnectionTest>("ai_test_connection", { provider });

/** Runs the provider's installer or sign-in in a terminal: output arrives
 * through `onConnectOutput`, the end through `onConnectExit`. */
export const connectRun = (provider: ProviderId, action: ConnectAction, rows: number, cols: number) =>
  invoke<void>("connect_run", { provider, action, rows, cols });

export const connectWrite = (data: string) => invoke<void>("connect_write", { data });

export const connectResize = (rows: number, cols: number) =>
  invoke<void>("connect_resize", { rows, cols });

export const connectCancel = () => invoke<void>("connect_cancel");

// --- Settings ---

export const appSettingsGet = () => invoke<AppSettings>("app_settings_get");

export const appSettingsSet = (settings: AppSettings) =>
  invoke<AppSettings>("app_settings_set", { settings });

export const projectSettingsGet = (projectPath: string) =>
  invoke<ProjectSettings>("project_settings_get", { projectPath });

export const projectSettingsSet = (projectPath: string, settings: ProjectSettings) =>
  invoke<ProjectSettings>("project_settings_set", { projectPath, settings });

export const godotOpenEditor = (projectPath: string) =>
  invoke<void>("godot_open_editor", { projectPath });

// --- Onboarding ---

export const onboardingTemplates = () => invoke<TemplateInfo[]>("onboarding_templates");

export const onboardingPreview = (answers: InterviewAnswers) =>
  invoke<OnboardingPreview>("onboarding_preview", { answers });

/** Creates the game in `<parentDir>/<answers.name>`. */
export const onboardingCreate = (parentDir: string, answers: InterviewAnswers, templateId: string) =>
  invoke<CreatedProject>("onboarding_create", { parentDir, answers, templateId });

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

export const onConnectOutput = (cb: (p: ConnectOutputPayload) => void) =>
  subscribe("connect-output", cb);

export const onConnectExit = (cb: (p: ConnectExitPayload) => void) => subscribe("connect-exit", cb);

export const onAutofixState = (cb: (p: AutoFixStatePayload) => void) =>
  subscribe("autofix-state", cb);
