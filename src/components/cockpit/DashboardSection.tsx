import { useCallback, useEffect, useState, type ReactNode } from "react";
import { AnimatePresence, motion } from "motion/react";
import {
  AlertCircle,
  Check,
  FolderOpen,
  FolderPlus,
  MoreHorizontal,
  RefreshCw,
  Sparkles,
  Trash2,
} from "lucide-react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Skeleton } from "@/components/ui/skeleton";
import { ConnectAiPanel } from "@/components/connect/ConnectAiPanel";
import { signedInLabel } from "@/components/connect/connect-format";
import { OnboardingFlow } from "@/components/onboarding/OnboardingFlow";
import { GodotInstallCard } from "@/components/studio/play/GodotInstallCard";
import { shortGodotVersion } from "@/components/studio/play/play-format";
import { fadeRise, fadeTransition } from "@/lib/motion";
import {
  pickAndOpenExistingProject,
  pickProjectFolder,
  projectFolderName,
} from "@/lib/project-picker";
import {
  loadRecentProjects,
  recordProjectOpened,
  removeRecentProject,
  type RecentProject,
} from "@/lib/recent-projects";
import { aiProviders, appSettingsGet, appSettingsSet, godotStatus, projectCreate } from "@/lib/studio-api";
import type {
  AppSettings,
  CreatedProject,
  GodotStatus,
  PendingTurn,
  ProviderInfo,
} from "@/lib/studio-types";

interface DashboardSectionProps {
  /** `pendingTurn`: a turn Studio starts as soon as it shows the project
   * (the onboarding's first build). */
  onOpenProject: (path: string, pendingTurn?: PendingTurn) => void;
}

/** A backend value that's still loading, arrived, or failed with the
 * backend's own message (shown as-is, never replaced by a guess). */
type Loadable<T> =
  | { status: "loading" }
  | { status: "ready"; value: T }
  | { status: "error"; message: string };

function errorText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

async function load<T>(call: () => Promise<T>): Promise<Loadable<T>> {
  try {
    return { status: "ready", value: await call() };
  } catch (err) {
    return { status: "error", message: errorText(err) };
  }
}

/** Home. Until the first game is made (`first_run_done`), a three-step
 * checklist (spec §6.2): connect your AI, set up Godot, make your first game
 * through the onboarding interview. After that: your games, "New game",
 * and a compact AI + Godot status row. Every status comes from the real
 * backend checks; if one fails, its error is shown instead. */
export function DashboardSection({ onOpenProject }: DashboardSectionProps) {
  const [projects, setProjects] = useState<RecentProject[]>([]);
  const [view, setView] = useState<"home" | "onboarding">("home");
  const [settings, setSettings] = useState<Loadable<AppSettings>>({ status: "loading" });
  const [providers, setProviders] = useState<Loadable<ProviderInfo[]>>({ status: "loading" });
  const [godot, setGodot] = useState<Loadable<GodotStatus>>({ status: "loading" });
  const [connectOpen, setConnectOpen] = useState(false);
  const [godotOpen, setGodotOpen] = useState(false);

  const [newProjectOpen, setNewProjectOpen] = useState(false);
  const [newProjectName, setNewProjectName] = useState("");
  const [newProjectLocation, setNewProjectLocation] = useState<string | null>(null);
  const [creatingProject, setCreatingProject] = useState(false);
  const [createProjectError, setCreateProjectError] = useState<string | null>(null);
  const [openError, setOpenError] = useState<string | null>(null);

  const refreshAi = useCallback(async () => {
    const [s, p] = await Promise.all([load(appSettingsGet), load(aiProviders)]);
    setSettings(s);
    setProviders(p);
  }, []);

  const refreshGodot = useCallback(async () => {
    setGodot({ status: "loading" });
    setGodot(await load(godotStatus));
  }, []);

  useEffect(() => {
    setProjects(loadRecentProjects());
    void refreshAi();
    void refreshGodot();
  }, [refreshAi, refreshGodot]);

  async function handleOpenExistingFolder() {
    const result = await pickAndOpenExistingProject();
    if (result.status === "cancelled") return;
    if (result.status === "invalid") {
      setOpenError(
        `"${projectFolderName(result.path)}" isn't an InfinaBox project — it has no .ibproject/.ibx file.`,
      );
      return;
    }
    setOpenError(null);
    setProjects(recordProjectOpened(result.path));
    onOpenProject(result.path);
  }

  function handleOpenExisting(path: string) {
    setProjects(recordProjectOpened(path));
    onOpenProject(path);
  }

  function handleRemove(path: string) {
    setProjects(removeRecentProject(path));
  }

  function openNewProjectDialog() {
    setOpenError(null);
    setNewProjectOpen(true);
  }

  function handleNewProjectOpenChange(open: boolean) {
    setNewProjectOpen(open);
    if (!open) {
      setNewProjectName("");
      setNewProjectLocation(null);
      setCreateProjectError(null);
    }
  }

  async function handleChooseLocation() {
    const folder = await pickProjectFolder();
    if (folder) setNewProjectLocation(folder);
  }

  // All the real work lives in `project_create` (core's `scaffold`): name
  // validation, the blank 2D Godot template, the InfinaBox addon, the
  // `.ibproject/` marker (`.ibx`, which is what `isInfinaBoxProject`
  // checks), a fresh repository, and the first snapshot. If any step fails
  // it removes what it wrote, so the user can fix the name and retry. Its
  // errors are plain sentences meant for users ("the project name can't
  // start with a dot"), so they're shown as-is.
  async function handleCreateProject() {
    const name = newProjectName.trim();
    // `creatingProject` also guards Enter in the name field, which isn't
    // disabled while a create is in flight the way the button is.
    if (!newProjectLocation || !name || creatingProject) return;
    setCreatingProject(true);
    setCreateProjectError(null);
    let path: string;
    try {
      path = await projectCreate(newProjectLocation, name);
    } catch (err) {
      setCreateProjectError(errorText(err));
      setCreatingProject(false);
      return;
    }
    setCreatingProject(false);
    handleNewProjectOpenChange(false);
    setProjects(recordProjectOpened(path));
    onOpenProject(path);
  }

  // The interview made a game: the first run is over, and Studio starts
  // the first build as soon as it shows the project.
  async function handleCreated(created: CreatedProject) {
    // A fresh read keeps the other fields (the chosen AI, a Godot path) as
    // they are now. If it can't be read, the last good copy is used; with
    // neither, nothing is written rather than defaults over real settings.
    const current = await appSettingsGet().catch(() =>
      settings.status === "ready" ? settings.value : null,
    );
    if (current) {
      try {
        const saved = await appSettingsSet({ ...current, first_run_done: true });
        setSettings({ status: "ready", value: saved });
      } catch {
        // Opening the new game matters more; the checklist just shows
        // again next time.
      }
    }
    setView("home");
    setProjects(recordProjectOpened(created.path));
    onOpenProject(created.path, {
      threadId: created.thread_id,
      message: created.first_build_message,
      origin: "first_build",
    });
  }

  const chosenId = settings.status === "ready" ? settings.value.ai_provider : null;
  const chosen =
    chosenId && providers.status === "ready"
      ? (providers.value.find((p) => p.id === chosenId) ?? null)
      : null;
  const aiReady = chosen !== null && chosen.installed && chosen.logged_in === true;
  const godotReady = godot.status === "ready" && godot.value.installed;
  // A settings read that failed shows the checklist too: it's the screen
  // that gets someone going, and every step says what it found.
  const firstRun = settings.status === "error" || (settings.status === "ready" && !settings.value.first_run_done);

  const mode = view === "onboarding" ? "onboarding" : settings.status === "loading" ? "loading" : firstRun ? "first-run" : "home";

  const connectPanel = (
    <ConnectAiPanel
      onConnected={() => {
        void refreshAi();
      }}
    />
  );

  const godotSetup = (
    <GodotInstallCard
      onInstalled={(status) => {
        setGodot({ status: "ready", value: status });
        setGodotOpen(false);
      }}
    />
  );

  const recentProjects = (
    <RecentProjectsGrid projects={projects} onOpen={handleOpenExisting} onRemove={handleRemove} />
  );

  return (
    <div className="flex h-full min-w-0 flex-1 flex-col overflow-hidden">
      {/* Home has no Sidebar and no other block header, so it had no real
       * drag space of its own — this is the same invisible, real-estate
       * header strip every other block uses (see e.g. TerminalPanel's
       * "Agent" header), just with no label since Home doesn't need one. */}
      <div data-tauri-drag-region className="h-9 shrink-0" />
      <AnimatePresence mode="wait" initial={false}>
        {mode === "onboarding" ? (
          <motion.div
            key="onboarding"
            {...fadeRise}
            transition={fadeTransition}
            className="min-h-0 flex-1 overflow-auto"
          >
            <OnboardingFlow onCreated={(c) => void handleCreated(c)} onCancel={() => setView("home")} />
          </motion.div>
        ) : (
          <motion.div
            key={mode}
            {...fadeRise}
            transition={fadeTransition}
            className="flex min-h-0 flex-1 flex-col"
          >
            <ScrollArea className="min-h-0 flex-1">
              <div className="mx-auto flex max-w-4xl flex-col gap-6 px-6 py-10">
                {openError && (
                  <Alert variant="destructive">
                    <AlertCircle />
                    <AlertDescription>{openError}</AlertDescription>
                  </Alert>
                )}

                {mode === "loading" && (
                  <div className="flex flex-col gap-3">
                    <Skeleton className="h-8 w-64" />
                    <Skeleton className="h-40" />
                  </div>
                )}

                {mode === "first-run" && (
                  <>
                    <div className="flex flex-col gap-1">
                      <h1 className="text-xl font-semibold tracking-tight">Welcome to InfinaBox</h1>
                      <p className="text-sm text-muted-foreground">
                        Three steps and you'll be playing a game that's yours.
                      </p>
                    </div>

                    {settings.status === "error" && (
                      <Alert variant="destructive">
                        <AlertCircle />
                        <AlertDescription className="break-words">
                          Couldn't read your settings: {settings.message}
                        </AlertDescription>
                      </Alert>
                    )}

                    <ol className="flex flex-col gap-3" data-testid="first-run-checklist">
                      <ChecklistStep
                        number={1}
                        title="Connect your AI"
                        done={aiReady}
                        testId="step-connect"
                        summary={chosen ? `Using ${chosen.name} — signed in.` : undefined}
                        onChange={() => setConnectOpen(true)}
                        expanded={!aiReady || connectOpen}
                      >
                        {connectPanel}
                        {aiReady && (
                          <Button
                            type="button"
                            size="sm"
                            variant="ghost"
                            className="self-start"
                            onClick={() => setConnectOpen(false)}
                          >
                            Done
                          </Button>
                        )}
                      </ChecklistStep>

                      <ChecklistStep
                        number={2}
                        title="Set up Godot"
                        done={godotReady}
                        testId="step-godot"
                        summary={
                          godot.status === "ready" && godot.value.version
                            ? `Godot ${shortGodotVersion(godot.value.version)} is ready.`
                            : "Godot is ready."
                        }
                        expanded={!godotReady}
                      >
                        <GodotStep godot={godot} onRetry={() => void refreshGodot()}>
                          {godotSetup}
                        </GodotStep>
                      </ChecklistStep>

                      <ChecklistStep
                        number={3}
                        title="Make your first game"
                        done={false}
                        testId="step-first-game"
                        expanded
                      >
                        <p className="text-sm text-muted-foreground">
                          Answer a few quick questions about your idea. Your AI then builds a first
                          version you can play.
                        </p>
                        {!(aiReady && godotReady) && (
                          <p className="text-xs text-muted-foreground">
                            Finish steps 1 and 2 first — your AI builds the game, and Godot runs it.
                          </p>
                        )}
                        <Button
                          type="button"
                          className="self-start"
                          data-testid="make-first-game"
                          variant={aiReady && godotReady ? "default" : "secondary"}
                          onClick={() => setView("onboarding")}
                        >
                          <Sparkles data-icon="inline-start" />
                          Make my first game
                        </Button>
                      </ChecklistStep>
                    </ol>

                    <div className="flex flex-wrap items-center gap-2 text-sm text-muted-foreground">
                      <span>Already have a project?</span>
                      <Button
                        type="button"
                        size="sm"
                        variant="secondary"
                        data-testid="open-project"
                        onClick={() => void handleOpenExistingFolder()}
                      >
                        <FolderOpen data-icon="inline-start" />
                        Open project
                      </Button>
                      <span>or</span>
                      <Button
                        type="button"
                        size="sm"
                        variant="ghost"
                        data-testid="new-project"
                        onClick={openNewProjectDialog}
                      >
                        <FolderPlus data-icon="inline-start" />
                        Start with an empty project
                      </Button>
                    </div>

                    {projects.length > 0 && (
                      <Card title={`Your games · ${projects.length}`}>{recentProjects}</Card>
                    )}
                  </>
                )}

                {mode === "home" && (
                  <>
                    <Card
                      title={`Your games${projects.length > 0 ? ` · ${projects.length}` : ""}`}
                      actions={
                        <>
                          <Button
                            type="button"
                            size="sm"
                            variant="secondary"
                            data-testid="open-project"
                            onClick={() => void handleOpenExistingFolder()}
                          >
                            <FolderOpen />
                            Open project
                          </Button>
                          <Button
                            type="button"
                            size="sm"
                            variant="secondary"
                            data-testid="new-project"
                            onClick={openNewProjectDialog}
                          >
                            <FolderPlus />
                            Empty project
                          </Button>
                          <Button
                            type="button"
                            size="sm"
                            data-testid="new-game"
                            onClick={() => setView("onboarding")}
                          >
                            <Sparkles />
                            New game
                          </Button>
                        </>
                      }
                    >
                      {projects.length === 0 ? (
                        <p className="py-6 text-center text-sm text-muted-foreground">
                          No games here yet — make a new one, or open one you already have.
                        </p>
                      ) : (
                        recentProjects
                      )}
                    </Card>

                    <Card title="Setup">
                      <div className="flex flex-col gap-2">
                        <StatusRow
                          label="AI"
                          testId="status-ai"
                          value={<AiStatus settings={settings} providers={providers} chosen={chosen} />}
                          actionLabel={connectOpen ? "Close" : chosen ? "Change" : "Connect"}
                          onAction={() => setConnectOpen((o) => !o)}
                        />
                        <Expand open={connectOpen}>{connectPanel}</Expand>
                        <StatusRow
                          label="Godot"
                          testId="status-godot"
                          value={<GodotStatusText godot={godot} />}
                          actionLabel={
                            godot.status === "error"
                              ? "Check again"
                              : godot.status === "ready" && !godot.value.installed
                                ? godotOpen
                                  ? "Close"
                                  : "Set up"
                                : undefined
                          }
                          onAction={() =>
                            godot.status === "error" ? void refreshGodot() : setGodotOpen((o) => !o)
                          }
                        />
                        <Expand open={godotOpen && godot.status === "ready" && !godot.value.installed}>
                          {godotSetup}
                        </Expand>
                      </div>
                    </Card>
                  </>
                )}
              </div>
            </ScrollArea>
          </motion.div>
        )}
      </AnimatePresence>

      <Dialog open={newProjectOpen} onOpenChange={handleNewProjectOpenChange}>
        {/* `minmax(0,1fr)`: the dialog is a one-column grid, and a grid
            column's default minimum is its content's width — so a long
            location path would widen the column past the dialog's edge,
            dragging the Create button out of reach with it. */}
        <DialogContent className="grid-cols-[minmax(0,1fr)]">
          <DialogHeader>
            <DialogTitle>Empty project</DialogTitle>
            <DialogDescription>
              Creates a new folder with a blank Godot game, ready to open in Studio.
            </DialogDescription>
          </DialogHeader>
          <div className="flex flex-col gap-3">
            <div className="flex flex-col gap-1.5">
              <span className="text-xs font-medium tracking-wide text-muted-foreground">
                Location
              </span>
              <div className="flex items-center gap-2">
                {/* Wraps rather than truncates: the end of a path (the
                    folder actually chosen) is the part worth seeing. */}
                <span
                  data-testid="new-project-location"
                  title={newProjectLocation ?? undefined}
                  className="min-w-0 flex-1 rounded-lg border border-border bg-background px-2.5 py-1.5 text-sm wrap-anywhere text-foreground/90"
                >
                  {newProjectLocation ?? "No location chosen"}
                </span>
                <Button
                  type="button"
                  size="sm"
                  variant="secondary"
                  className="shrink-0"
                  data-testid="new-project-choose-location"
                  onClick={() => void handleChooseLocation()}
                >
                  Choose…
                </Button>
              </div>
            </div>
            <div className="flex flex-col gap-1.5">
              <span className="text-xs font-medium tracking-wide text-muted-foreground">Name</span>
              <Input
                autoFocus
                data-testid="new-project-name"
                value={newProjectName}
                placeholder="My Game"
                onChange={(e) => {
                  setNewProjectName(e.target.value);
                  setCreateProjectError(null);
                }}
                onKeyDown={(e) => {
                  if (e.key === "Enter") void handleCreateProject();
                }}
              />
            </div>
            {newProjectLocation && newProjectName.trim() && (
              <p className="text-xs wrap-anywhere text-muted-foreground/80">
                Will create{" "}
                <span className="font-mono">
                  {newProjectLocation}/{newProjectName.trim()}
                </span>
              </p>
            )}
            {createProjectError && (
              <Alert variant="destructive" data-testid="new-project-error">
                <AlertCircle />
                <AlertDescription className="wrap-anywhere">{createProjectError}</AlertDescription>
              </Alert>
            )}
          </div>
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => handleNewProjectOpenChange(false)}>
              Cancel
            </Button>
            <Button
              type="button"
              data-testid="new-project-create"
              disabled={!newProjectLocation || !newProjectName.trim() || creatingProject}
              onClick={() => void handleCreateProject()}
            >
              {creatingProject ? "Creating…" : "Create Project"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

/** The dashboard's card look: title row (with optional buttons) above content. */
function Card({ title, actions, children }: { title: string; actions?: ReactNode; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-3 rounded-xl border border-border bg-card p-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h2 className="text-sm font-medium tracking-wide text-muted-foreground">{title}</h2>
        {actions && <div className="flex flex-wrap items-center gap-2">{actions}</div>}
      </div>
      {children}
    </div>
  );
}

/** A panel revealed under a status row — a plain fade in and out, with no
 * height animation (see `widthTransition`'s comment on what animating a
 * layout property costs). */
function Expand({ open, children }: { open: boolean; children: ReactNode }) {
  return (
    <AnimatePresence initial={false}>
      {open && (
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={fadeTransition}
          className="rounded-lg border border-border p-3"
        >
          {children}
        </motion.div>
      )}
    </AnimatePresence>
  );
}

interface ChecklistStepProps {
  number: number;
  title: string;
  done: boolean;
  testId: string;
  /** One line shown in place of the content once the step is done. */
  summary?: string;
  /** Reopens a done step ("Change"). */
  onChange?: () => void;
  expanded: boolean;
  children: ReactNode;
}

function ChecklistStep({ number, title, done, testId, summary, onChange, expanded, children }: ChecklistStepProps) {
  return (
    <li
      data-testid={testId}
      data-done={done}
      className="flex flex-col gap-3 rounded-xl border border-border bg-card p-4"
    >
      <div className="flex items-center gap-3">
        <span
          className={`flex size-6 shrink-0 items-center justify-center rounded-full border text-xs font-medium ${
            done
              ? "border-emerald-500/50 bg-emerald-500/15 text-emerald-400"
              : "border-border text-muted-foreground"
          }`}
        >
          {done ? <Check className="size-3.5" /> : number}
        </span>
        <div className="flex min-w-0 flex-1 flex-col">
          <span className="text-sm font-medium">{title}</span>
          {done && summary && !expanded && (
            <span className="truncate text-xs text-muted-foreground">{summary}</span>
          )}
        </div>
        {done && onChange && !expanded && (
          <Button type="button" size="xs" variant="ghost" onClick={onChange}>
            Change
          </Button>
        )}
      </div>
      <AnimatePresence initial={false}>
        {expanded && (
          <motion.div
            key="content"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={fadeTransition}
            className="flex flex-col gap-3 sm:pl-9"
          >
            {children}
          </motion.div>
        )}
      </AnimatePresence>
    </li>
  );
}

/** Step 2's body: the real Godot status, or its error with a retry. The
 * install card is offered whenever Godot isn't known to be installed — a
 * failed status check doesn't stop an install from working. */
function GodotStep({
  godot,
  onRetry,
  children,
}: {
  godot: Loadable<GodotStatus>;
  onRetry: () => void;
  children: ReactNode;
}) {
  if (godot.status === "loading") return <Skeleton className="h-16" />;
  return (
    <>
      {godot.status === "error" && (
        <Alert variant="destructive">
          <AlertCircle />
          <AlertDescription className="break-words">
            <p>Couldn't check for Godot: {godot.message}</p>
            <Button type="button" size="xs" variant="ghost" onClick={onRetry}>
              <RefreshCw data-icon="inline-start" />
              Check again
            </Button>
          </AlertDescription>
        </Alert>
      )}
      <div className="rounded-lg border border-border">{children}</div>
    </>
  );
}

function StatusRow({
  label,
  value,
  actionLabel,
  onAction,
  testId,
}: {
  label: string;
  value: ReactNode;
  actionLabel?: string;
  onAction: () => void;
  testId: string;
}) {
  return (
    <div
      data-testid={testId}
      className="flex items-center justify-between gap-3 rounded-lg border border-border bg-background px-3 py-2"
    >
      <div className="flex min-w-0 items-center gap-3">
        <span className="w-12 shrink-0 text-xs text-muted-foreground">{label}</span>
        <span className="min-w-0 truncate text-sm text-foreground/90">{value}</span>
      </div>
      {actionLabel && (
        <Button type="button" size="xs" variant="ghost" className="shrink-0" onClick={onAction}>
          {actionLabel}
        </Button>
      )}
    </div>
  );
}

function AiStatus({
  settings,
  providers,
  chosen,
}: {
  settings: Loadable<AppSettings>;
  providers: Loadable<ProviderInfo[]>;
  chosen: ProviderInfo | null;
}) {
  if (settings.status === "loading" || providers.status === "loading") {
    return <Skeleton className="inline-block h-4 w-40 align-middle" />;
  }
  if (settings.status === "error") return <>Couldn't read your settings: {settings.message}</>;
  if (!settings.value.ai_provider) return <>None chosen yet</>;
  if (providers.status === "error") return <>Couldn't check it: {providers.message}</>;
  if (!chosen) return <>{settings.value.ai_provider} (not found)</>;
  if (!chosen.installed) return <>{chosen.name} · not installed</>;
  return (
    <>
      {chosen.name} · Signed in: {signedInLabel(chosen.logged_in)}
    </>
  );
}

function GodotStatusText({ godot }: { godot: Loadable<GodotStatus> }) {
  if (godot.status === "loading") return <Skeleton className="inline-block h-4 w-24 align-middle" />;
  if (godot.status === "error") return <>Couldn't check: {godot.message}</>;
  if (!godot.value.installed) return <>Not set up yet</>;
  return (
    <span title={godot.value.version ?? undefined}>
      {godot.value.version ? `Godot ${shortGodotVersion(godot.value.version)}` : "Installed"}
      {godot.value.managed ? "" : " · your own copy"}
    </span>
  );
}

function RecentProjectsGrid({
  projects,
  onOpen,
  onRemove,
}: {
  projects: RecentProject[];
  onOpen: (path: string) => void;
  onRemove: (path: string) => void;
}) {
  return (
    <motion.div layout className="grid grid-cols-2 gap-2 sm:grid-cols-3">
      <AnimatePresence>
        {projects.map((project) => (
          <motion.div
            key={project.path}
            layout
            initial={{ opacity: 0, scale: 0.95 }}
            animate={{ opacity: 1, scale: 1 }}
            exit={{ opacity: 0, scale: 0.95 }}
            transition={fadeTransition}
            className="group relative flex flex-col gap-1 rounded-lg border border-border bg-background p-3 text-left transition-colors hover:border-muted-foreground"
          >
            <button
              type="button"
              onClick={() => onOpen(project.path)}
              className="flex flex-col gap-1 text-left"
            >
              <span className="truncate text-sm font-medium text-foreground/90">
                {projectFolderName(project.path)}
              </span>
              <span className="truncate text-xs text-muted-foreground">{project.path}</span>
            </button>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-xs"
                  className="absolute top-2 right-2 opacity-0 group-hover:opacity-100"
                  onClick={(e) => e.stopPropagation()}
                >
                  <MoreHorizontal />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end">
                <DropdownMenuItem variant="destructive" onSelect={() => onRemove(project.path)}>
                  <Trash2 />
                  Remove from list
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </motion.div>
        ))}
      </AnimatePresence>
    </motion.div>
  );
}
