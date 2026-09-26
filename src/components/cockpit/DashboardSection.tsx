import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AnimatePresence, motion } from "motion/react";
import {
  AlertCircle,
  FolderOpen,
  FolderPlus,
  MoreHorizontal,
  Settings2,
  Terminal,
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
import { NotBuiltYetSection } from "@/components/cockpit/NotBuiltYetSection";
import { fadeTransition } from "@/lib/motion";
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
import { projectCreate } from "@/lib/studio-api";

interface DashboardSectionProps {
  onOpenProject: (path: string) => void;
}

// The command-line tools InfinaBox's own terminal actually depends on —
// `git`/`gh` for the Overview/Changes tabs and PR workflows, `claude`/
// `codex` as the two agent CLIs this app is built around (see the
// "Agent auth model" note: there's no chat panel, the embedded terminal
// just runs whichever of these the user already has installed and signed
// in on their own machine). Checked against real PATH lookups — never
// InfinaBox-managed accounts — because InfinaBox has no way to install or
// authenticate any of these itself.
const CLI_TOOLS = ["claude", "codex", "git", "gh"];

type ToolState = { status: "loading" } | { status: "ready"; installed: boolean };

function useCliToolStatus() {
  const [tools, setTools] = useState<Record<string, ToolState>>(
    Object.fromEntries(CLI_TOOLS.map((name) => [name, { status: "loading" }])),
  );

  useEffect(() => {
    let cancelled = false;
    invoke<{ name: string; installed: boolean }[]>("check_cli_tools", { names: CLI_TOOLS })
      .then((results) => {
        if (cancelled) return;
        setTools(
          Object.fromEntries(
            results.map((r) => [r.name, { status: "ready", installed: r.installed }]),
          ),
        );
      })
      .catch(() => {
        // A failed check just leaves every row showing its loading
        // skeleton forever, which reads fine as "unknown" — not worth a
        // dedicated error state for a best-effort convenience panel.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return tools;
}

export function DashboardSection({ onOpenProject }: DashboardSectionProps) {
  const [projects, setProjects] = useState<RecentProject[]>([]);
  const tools = useCliToolStatus();

  const [newProjectOpen, setNewProjectOpen] = useState(false);
  const [newProjectName, setNewProjectName] = useState("");
  const [newProjectLocation, setNewProjectLocation] = useState<string | null>(null);
  const [creatingProject, setCreatingProject] = useState(false);
  const [createProjectError, setCreateProjectError] = useState<string | null>(null);
  const [openError, setOpenError] = useState<string | null>(null);

  useEffect(() => {
    setProjects(loadRecentProjects());
  }, []);

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
  // start with a dot"), so they're shown as-is. Documents/Graphs folders
  // aren't pre-created any more: every `.ibproject/<slug>` folder is
  // already created lazily on first "New Document" in its section.
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
      setCreateProjectError(err instanceof Error ? err.message : String(err));
      setCreatingProject(false);
      return;
    }
    setCreatingProject(false);
    handleNewProjectOpenChange(false);
    setProjects(recordProjectOpened(path));
    onOpenProject(path);
  }

  return (
    <div className="flex h-full min-w-0 flex-1 flex-col overflow-hidden">
      {/* Home has no Sidebar and no other block header, so it had no real
       * drag space of its own — this is the same invisible, real-estate
       * header strip every other block uses (see e.g. TerminalPanel's
       * "Agent" header), just with no label since Home doesn't need one. */}
      <div data-tauri-drag-region className="h-9 shrink-0" />
      <ScrollArea className="min-h-0 flex-1">
        <div className="mx-auto flex max-w-4xl flex-col gap-6 px-6 py-10">
          <motion.div
            initial={{ opacity: 0, y: 8 }}
            animate={{ opacity: 1, y: 0 }}
            transition={fadeTransition}
            className="flex flex-col gap-3 rounded-xl border border-border bg-card p-4"
          >
            <div className="flex items-center justify-between">
              <h2 className="text-sm font-medium tracking-wide text-muted-foreground">
                Projects {projects.length > 0 && <span>· {projects.length}</span>}
              </h2>
              <div className="flex items-center gap-2">
                <Button
                  type="button"
                  size="sm"
                  variant="secondary"
                  onClick={() => void handleOpenExistingFolder()}
                >
                  <FolderOpen />
                  Open Project
                </Button>
                <Button
                  type="button"
                  size="sm"
                  data-testid="new-project"
                  onClick={() => {
                    setOpenError(null);
                    setNewProjectOpen(true);
                  }}
                >
                  <FolderPlus />
                  New Project
                </Button>
              </div>
            </div>

            {openError && (
              <Alert variant="destructive">
                <AlertCircle />
                <AlertDescription>{openError}</AlertDescription>
              </Alert>
            )}

            <AnimatePresence mode="wait" initial={false}>
              {projects.length === 0 ? (
                <motion.p
                  key="empty"
                  initial={{ opacity: 0 }}
                  animate={{ opacity: 1 }}
                  exit={{ opacity: 0 }}
                  transition={fadeTransition}
                  className="py-6 text-center text-sm text-muted-foreground"
                >
                  No projects yet — open a folder to get started.
                </motion.p>
              ) : (
                <motion.div
                  key="grid"
                  layout
                  className="grid grid-cols-2 gap-2 sm:grid-cols-3"
                >
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
                          onClick={() => handleOpenExisting(project.path)}
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
                            <DropdownMenuItem
                              variant="destructive"
                              onSelect={() => handleRemove(project.path)}
                            >
                              <Trash2 />
                              Remove from list
                            </DropdownMenuItem>
                          </DropdownMenuContent>
                        </DropdownMenu>
                      </motion.div>
                    ))}
                  </AnimatePresence>
                </motion.div>
              )}
            </AnimatePresence>
          </motion.div>

          <motion.div
            initial={{ opacity: 0, y: 8 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ ...fadeTransition, delay: 0.05 }}
            className="flex flex-col gap-3 rounded-xl border border-border bg-card p-4"
          >
            <div>
              <h2 className="text-sm font-medium tracking-wide text-muted-foreground">
                Command-line tools
              </h2>
              <p className="text-xs text-muted-foreground/80">
                InfinaBox's terminal runs whatever's already installed and signed in on your
                machine — checked here, not managed by InfinaBox.
              </p>
            </div>
            <div className="flex flex-col gap-1">
              {CLI_TOOLS.map((name) => {
                const state = tools[name];
                return (
                  <div
                    key={name}
                    data-testid={`cli-tool-${name}`}
                    data-status={
                      state.status === "loading" ? "loading" : state.installed ? "installed" : "missing"
                    }
                    className="flex items-center justify-between rounded-lg border border-border bg-background px-3 py-2"
                  >
                    <div className="flex items-center gap-2">
                      <Terminal className="size-3.5 text-muted-foreground" />
                      <span className="font-mono text-sm text-foreground/90">{name}</span>
                    </div>
                    {state.status === "loading" ? (
                      <Skeleton className="h-4 w-16" />
                    ) : (
                      <span
                        className={
                          state.installed
                            ? "text-xs font-medium text-emerald-400"
                            : "text-xs text-muted-foreground"
                        }
                      >
                        {state.installed ? "Installed" : "Not found"}
                      </span>
                    )}
                  </div>
                );
              })}
            </div>
          </motion.div>

          <motion.div
            initial={{ opacity: 0, y: 8 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ ...fadeTransition, delay: 0.1 }}
            className="flex flex-col gap-3"
          >
            <h2 className="text-sm font-medium tracking-wide text-muted-foreground">Preferences</h2>
            <NotBuiltYetSection
              icon={Settings2}
              description="App-wide preferences arrive once there's something real to configure."
            />
          </motion.div>
        </div>
      </ScrollArea>

      <Dialog open={newProjectOpen} onOpenChange={handleNewProjectOpenChange}>
        {/* `minmax(0,1fr)`: the dialog is a one-column grid, and a grid
            column's default minimum is its content's width — so a long
            location path would widen the column past the dialog's edge,
            dragging the Create button out of reach with it. */}
        <DialogContent className="grid-cols-[minmax(0,1fr)]">
          <DialogHeader>
            <DialogTitle>New Project</DialogTitle>
            <DialogDescription>
              Creates a new folder with a blank 2D Godot game, ready to open in Studio.
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
