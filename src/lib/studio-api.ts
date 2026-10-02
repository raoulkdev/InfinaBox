import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  SkillInfo,
  TurnMode,
  AgentEventPayload,
  AssetInfo,
  Board,
  Card,
  CardMeta,
  CardSummary,
  CardType,
  CredentialStatus,
  FilePayload,
  GenPreview,
  GenProviderInfo,
  GenRequest,
  HealthReport,
  Journey,
  LibraryItem,
  LibraryProviderInfo,
  LibraryQuery,
  LicenseInfo,
  LinkGraph,
  Role,
  SecretName,
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
  Effort,
  ProviderInfo,
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
  role?: Role,
  model?: string | null,
  effort?: Effort | null,
  mode?: TurnMode,
) =>
  invoke<void>("agent_send", {
    projectPath,
    threadId,
    message,
    origin: origin ?? null,
    role: role ?? null,
    model: model ?? null,
    effort: effort ?? null,
    mode: mode ?? null,
  });

/** Saves a file attached to a message; returns its project-relative path. */
export const chatAttach = (projectPath: string, name: string, dataBase64: string) =>
  invoke<string>("chat_attach", { projectPath, name, dataBase64 });

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


export const onboardingPreview = (answers: InterviewAnswers) =>
  invoke<OnboardingPreview>("onboarding_preview", { answers });

/** Creates the game in `<parentDir>/<answers.name>`. */
export const onboardingCreate = (parentDir: string, answers: InterviewAnswers) =>
  invoke<CreatedProject>("onboarding_create", { parentDir, answers });

// --- Context cards (Phase C) ---

export const contextList = (projectPath: string) =>
  invoke<CardSummary[]>("context_list", { projectPath });

export const contextRead = (projectPath: string, path: string) =>
  invoke<Card>("context_read", { projectPath, path });

/** Creates or replaces a card; returns it as saved. */
export const contextWrite = (projectPath: string, path: string, meta: CardMeta, body: string) =>
  invoke<Card>("context_write", { projectPath, path, meta, body });

export const contextSetStatus = (projectPath: string, path: string, status: string) =>
  invoke<Card>("context_set_status", { projectPath, path, status });

/** Cards of `types` (all when empty) in status columns. */
export const contextBoard = (projectPath: string, types: CardType[]) =>
  invoke<Board>("context_board", { projectPath, types });

export const contextFolders = (projectPath: string) => invoke<string[]>("context_folders", { projectPath });
export const contextFolderIcons = (projectPath: string) =>
  invoke<Record<string, string>>("context_folder_icons", { projectPath });
export const contextSetFolderIcon = (projectPath: string, path: string, icon: string | null) =>
  invoke<void>("context_set_folder_icon", { projectPath, path, icon });
export const contextCreateFolder = (projectPath: string, path: string) =>
  invoke<void>("context_create_folder", { projectPath, path });
/** Moves or renames a note or folder; resolves to its new path. */
export const contextMove = (projectPath: string, from: string, to: string) =>
  invoke<string>("context_move", { projectPath, from, to });
export const contextDelete = (projectPath: string, path: string) =>
  invoke<void>("context_delete", { projectPath, path });
export const contextGraph = (projectPath: string) =>
  invoke<LinkGraph>("context_graph", { projectPath });

// --- Producer journey (Phase C) ---

export const journeyGet = (projectPath: string) => invoke<Journey>("journey_get", { projectPath });

export const journeySetManual = (projectPath: string, id: string, done: boolean) =>
  invoke<Journey>("journey_set_manual", { projectPath, id, done });

// --- Assets, libraries, generation, credentials (Phase C) ---

export const assetsScan = (projectPath: string) => invoke<AssetInfo[]>("assets_scan", { projectPath });

/** Copies a file into the project and records where it came from. */
export const assetsImport = (
  projectPath: string,
  source: string,
  destSubdir: string | null,
  title: string,
  license: LicenseInfo,
) => invoke<AssetInfo>("assets_import", { projectPath, source, destSubdir, title, license });

export const assetsHealth = (projectPath: string) => invoke<HealthReport>("assets_health", { projectPath });

/** The credits page as Markdown. */
export const assetsCredits = (projectPath: string) => invoke<string>("assets_credits", { projectPath });

/** A project file for preview (size-capped). */
export const assetsReadBase64 = (projectPath: string, path: string, maxBytes?: number) =>
  invoke<FilePayload>("assets_read_base64", { projectPath, path, maxBytes: maxBytes ?? null });

export const libraryProviders = () => invoke<LibraryProviderInfo[]>("library_providers");

export const librarySearch = (provider: string, query: LibraryQuery) =>
  invoke<LibraryItem[]>("library_search", { provider, query });

/** Downloads/copies the item into the project's assets; `license` overrides an unknown one. */
export const libraryImport = (
  projectPath: string,
  provider: string,
  item: LibraryItem,
  destSubdir?: string,
) => invoke<AssetInfo[]>("library_import", { projectPath, provider, item, destSubdir: destSubdir ?? null });

export const generateProviders = () => invoke<GenProviderInfo[]>("generate_providers");

export const generateRun = (projectPath: string, request: GenRequest) =>
  invoke<GenPreview>("generate_run", { projectPath, request });

export const generateAccept = (projectPath: string, tempId: string, title: string, destSubdir?: string) =>
  invoke<AssetInfo>("generate_accept", { projectPath, tempId, title, destSubdir: destSubdir ?? null });

export const generateDiscard = (tempId: string) => invoke<void>("generate_discard", { tempId });

/** Whether each name is set; a value is never returned. */
export const credentialStatus = (names: SecretName[]) =>
  invoke<CredentialStatus[]>("credential_status", { names });

export const credentialSet = (name: SecretName, value: string) =>
  invoke<void>("credential_set", { name, value });

export const credentialClear = (name: SecretName) => invoke<void>("credential_clear", { name });

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

export interface CardHit {
  path: string;
  title: string;
  snippet: string;
}

export const contextSearch = (projectPath: string, query: string) =>
  invoke<CardHit[]>("context_search", { projectPath, query });

export interface StoredBoard {
  id: string;
  json: string;
}

export const boardsReadAll = (projectPath: string) => invoke<StoredBoard[]>("boards_read_all", { projectPath });
export const boardWrite = (projectPath: string, id: string, json: string) =>
  invoke<void>("board_write", { projectPath, id, json });
export const boardDelete = (projectPath: string, id: string) => invoke<void>("board_delete", { projectPath, id });
export const boardSaveFile = (projectPath: string, name: string, dataBase64: string) =>
  invoke<string>("board_save_file", { projectPath, name, dataBase64 });

export const skillsList = (projectPath: string) => invoke<SkillInfo[]>("skills_list", { projectPath });
export const skillCreate = (projectPath: string, name: string) =>
  invoke<SkillInfo>("skill_create", { projectPath, name });
