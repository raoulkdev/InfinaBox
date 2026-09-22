import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AlertCircle, ChevronRight, FileWarning, FilePlus2, FolderOpen, FolderPlus } from "lucide-react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Skeleton } from "@/components/ui/skeleton";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { CodeEditor } from "@/components/cockpit/CodeEditor";
import { FileGrid } from "@/components/cockpit/FileGrid";
import { FileList } from "@/components/cockpit/FileList";
import { GraphEditor } from "@/components/cockpit/GraphEditor";
import { MarkdownEditor } from "@/components/cockpit/MarkdownEditor";
import { ResizablePanelGroup } from "@/components/cockpit/ResizablePanelGroup";
import { isMissingDirectoryError } from "@/lib/backend-errors";
import { onProjectFilesChanged } from "@/lib/fs-watch";
import { cn } from "@/lib/utils";
import type { FileEntry } from "@/types/fs";

interface FileBrowserProps {
  label: string;
  /** Fully-resolved absolute path to browse, or `null` when there's
   * nothing to show yet (no project open). Unlike a plain "open project"
   * root, this path is allowed not to exist on disk yet — e.g. Documents'
   * `.ibproject/docs` before the project has adopted the convention — the
   * first New Folder/New Document click creates it. */
  rootPath: string | null;
  className?: string;
  /** Documents' browser: a "New Document" with no extension typed becomes
   * `.md` automatically, since that's the one text format this app can
   * actually edit/preview. Files' general browser leaves names as typed. */
  forceMdExtension?: boolean;
  /** Graphs' browser: a "New Document" with no extension typed becomes
   * `.graph.json` automatically — `.graph.json` files always open in the
   * ReactFlow-backed GraphEditor instead of the markdown editor, in any
   * FileBrowser instance, not just this one. */
  graphExtension?: boolean;
  /** "grid" (default): Finder icon tiles above the inspector — the Files
   * panel. "list": a compact row list in a narrow column beside the
   * inspector, which then gets most of the width — the Design tab, where
   * the editor is the point and the doc list is just navigation. */
  variant?: "grid" | "list";
  /** Build tab's "mini VS Code" window: every readable text file opens in
   * the CodeMirror-backed `CodeEditor`, regardless of extension, instead of
   * MarkdownEditor/GraphEditor's format-specific handling — this browser is
   * a general source viewer, not a docs editor. A file that can't be read
   * as text still can't be opened, same as everywhere else. */
  codeEditor?: boolean;
}

type TreeState =
  | { status: "empty" }
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "ready"; entries: FileEntry[] };

type InspectorState =
  | { status: "loading" }
  | { status: "binary" }
  | { status: "error"; message: string }
  | { status: "ready"; savedContent: string };

function isMarkdown(path: string): boolean {
  return /\.md$/i.test(path);
}

function isGraphDoc(path: string): boolean {
  return /\.graph\.json$/i.test(path);
}

function fileName(path: string): string {
  return path.split("/").pop() ?? path;
}

// Locked down by a real Rust test (see src-tauri/src/commands/fs.rs,
// read_file_error_message_for_binary_content_contains_expected_substring) —
// this is std::fs::read_to_string's actual, real error text for non-UTF-8
// content, not a guess.
function isLikelyBinaryError(message: string): boolean {
  return message.toLowerCase().includes("stream did not contain valid utf-8");
}

/** Finds the entry at `path` anywhere in the (already fully-loaded,
 * recursive) tree — `list_directory` returns the whole tree in one call,
 * so navigating into a folder is just walking this in-memory structure,
 * not a new backend round trip. */
function findEntry(entries: FileEntry[], path: string): FileEntry | undefined {
  for (const entry of entries) {
    if (entry.path === path) return entry;
    if (entry.children) {
      const found = findEntry(entry.children, path);
      if (found) return found;
    }
  }
  return undefined;
}

/** Breadcrumb trail from `root` down to `current`, e.g. `docs / gdd`. */
function pathSegments(root: string, current: string): { name: string; path: string }[] {
  const segments = [{ name: fileName(root), path: root }];
  const relative = current.slice(root.length).split("/").filter(Boolean);
  let acc = root;
  for (const part of relative) {
    acc = `${acc}/${part}`;
    segments.push({ name: part, path: acc });
  }
  return segments;
}

function FileInspector({ path, codeEditor }: { path: string; codeEditor?: boolean }) {
  const [state, setState] = useState<InspectorState>({ status: "loading" });
  const [draft, setDraft] = useState("");
  const [saveError, setSaveError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  // Bumped only when an external change (see the effect below) actually
  // replaces the loaded content — used as part of the editor's `key` so
  // MarkdownEditor/GraphEditor (which only read their `initialValue` prop
  // on mount) remount and pick up the new text, without remounting on
  // every keystroke the user makes locally.
  const [version, setVersion] = useState(0);

  useEffect(() => {
    let cancelled = false;
    setState({ status: "loading" });
    setSaveError(null);

    async function load() {
      try {
        const contents = await invoke<string>("read_file", { path });
        if (!cancelled) {
          setState({ status: "ready", savedContent: contents });
          setDraft(contents);
        }
      } catch (err) {
        if (!cancelled) {
          const message = err instanceof Error ? err.message : String(err);
          setState(
            isLikelyBinaryError(message)
              ? { status: "binary" }
              : { status: "error", message },
          );
        }
      }
    }

    void load();
    return () => {
      cancelled = true;
    };
  }, [path]);

  // Kept current every render so the fs-change listener below (subscribed
  // once, on mount) always sees the latest draft/state without having to
  // resubscribe on every keystroke.
  const stateRef = useRef(state);
  stateRef.current = state;
  const draftRef = useRef(draft);
  draftRef.current = draft;

  useEffect(() => {
    return onProjectFilesChanged(() => {
      void (async () => {
        const current = stateRef.current;
        // Only reload while there's nothing unsaved — a change made
        // outside the app (another editor, a git checkout) must never
        // silently clobber an in-progress edit here. The user's own Save
        // already caused this exact change, so there's nothing to pick up
        // once they've saved and gone back to "clean" anyway.
        if (current.status !== "ready" || draftRef.current !== current.savedContent) return;
        try {
          const contents = await invoke<string>("read_file", { path });
          if (contents === current.savedContent) return;
          setState({ status: "ready", savedContent: contents });
          setDraft(contents);
          setVersion((v) => v + 1);
        } catch {
          // Most likely the file was deleted or moved externally — leave
          // the last-known content showing rather than surprising the user
          // with an error while they're looking at something else; picking
          // a different file and back will surface the real state.
        }
      })();
    });
  }, [path]);

  const isMd = isMarkdown(path);
  const isGraph = isGraphDoc(path);
  const editable = codeEditor || isMd || isGraph;
  const dirty = editable && state.status === "ready" && draft !== state.savedContent;

  async function save() {
    if (state.status !== "ready" || saving) return;
    setSaving(true);
    setSaveError(null);
    try {
      await invoke("write_file", { path, contents: draft });
      setState({ status: "ready", savedContent: draft });
    } catch (err) {
      setSaveError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="flex h-full min-w-0 flex-1 flex-col">
      <div data-tauri-drag-region className="flex h-8 shrink-0 items-center justify-between px-3">
        <div className="flex min-w-0 items-center gap-2">
          <Tooltip>
            <TooltipTrigger asChild>
              <span className="truncate text-sm text-foreground/90">{fileName(path)}</span>
            </TooltipTrigger>
            <TooltipContent>{path}</TooltipContent>
          </Tooltip>
          {dirty && <Badge variant="outline">unsaved</Badge>}
        </div>
        {editable && (
          <Button
            size="xs"
            variant="secondary"
            onClick={() => void save()}
            disabled={state.status !== "ready" || !dirty || saving}
          >
            {saving ? "Saving…" : "Save"}
          </Button>
        )}
      </div>

      {/* Everything below the header row is genuinely interactive content
       * (editors, scrollable text) — unlike the header, there's no empty
       * "grab the top of the block" space worth preserving here. No
       * `pointer-events-auto` needed: `data-tauri-drag-region` never sets
       * CSS pointer-events (see App.tsx's top comment), so there's nothing
       * to opt back into. */}
      <div className="flex min-h-0 flex-1 flex-col">
        {saveError && (
          <Alert variant="destructive" className="m-3 w-auto shrink-0">
            <AlertCircle />
            <AlertDescription>Save failed: {saveError}</AlertDescription>
          </Alert>
        )}

        {state.status === "loading" && (
          <div className="flex flex-col gap-2 p-3">
            <Skeleton className="h-4 w-full" />
            <Skeleton className="h-4 w-5/6" />
            <Skeleton className="h-4 w-4/6" />
          </div>
        )}

        {state.status === "binary" && codeEditor && (
          <div className="flex flex-1 flex-col items-center justify-center gap-2 text-center">
            <div className="flex size-10 items-center justify-center rounded-lg border border-border">
              <FileWarning className="size-5 text-muted-foreground" />
            </div>
            <h2 className="text-sm font-medium tracking-tight">Not built yet</h2>
            <p className="max-w-xs text-sm text-muted-foreground">
              Binary files can't be opened in the editor yet.
            </p>
          </div>
        )}

        {state.status === "binary" && !codeEditor && (
          <div className="flex flex-1 items-center justify-center">
            <p className="text-sm text-muted-foreground">
              Can't preview this file — it looks like binary content.
            </p>
          </div>
        )}

        {state.status === "error" && (
          <div className="flex flex-1 items-center justify-center p-3">
            <Alert variant="destructive" className="w-auto">
              <AlertCircle />
              <AlertDescription>{state.message}</AlertDescription>
            </Alert>
          </div>
        )}

        {/* Markdown always gets the WYSIWYG editor, even in the Build
         * tab's code-editor mode — CodeMirror is for the general case,
         * not a replacement for the one format this app has a real
         * rich editor for. */}
        {state.status === "ready" && isMd && (
          <div className="min-h-0 flex-1 overflow-hidden">
            <MarkdownEditor
              key={`${path}:${version}`}
              initialValue={state.savedContent}
              onChange={setDraft}
              onSave={() => void save()}
            />
          </div>
        )}

        {state.status === "ready" && codeEditor && !isMd && (
          <div className="min-h-0 flex-1 overflow-hidden">
            <CodeEditor
              key={`${path}:${version}`}
              path={path}
              initialValue={state.savedContent}
              onChange={setDraft}
              onSave={() => void save()}
            />
          </div>
        )}

        {state.status === "ready" && !codeEditor && isGraph && (
          <div className="min-h-0 flex-1 overflow-hidden">
            <GraphEditor
              key={`${path}:${version}`}
              initialValue={state.savedContent}
              onChange={setDraft}
              onSave={() => void save()}
            />
          </div>
        )}

        {state.status === "ready" && !editable && (
          <ScrollArea className="min-h-0 flex-1">
            <pre className="whitespace-pre-wrap p-3 font-mono text-xs text-foreground/90">
              {state.savedContent}
            </pre>
          </ScrollArea>
        )}
      </div>
    </div>
  );
}

/** A Finder-style file browser (breadcrumb + icon grid) paired with an
 * inspector below — the shared engine behind both the Files panel (rooted
 * at the whole project) and the Design tab (rooted at `.ibproject/docs`).
 * Owns create/delete: New Folder / New Document in the toolbar, Delete via
 * right-click on any tile. */
export function FileBrowser({
  label,
  rootPath,
  className,
  forceMdExtension,
  graphExtension,
  variant = "grid",
  codeEditor,
}: FileBrowserProps) {
  const [tree, setTree] = useState<TreeState>({ status: "empty" });
  const [currentPath, setCurrentPath] = useState<string | null>(null);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [refreshToken, setRefreshToken] = useState(0);
  const [creating, setCreating] = useState<"file" | "folder" | null>(null);
  const [newName, setNewName] = useState("");
  const [createError, setCreateError] = useState<string | null>(null);

  useEffect(() => {
    setSelectedPath(null);
    setCurrentPath(rootPath);

    if (!rootPath) {
      setTree({ status: "empty" });
      return;
    }

    let cancelled = false;
    setTree({ status: "loading" });

    async function load() {
      try {
        const entries = await invoke<FileEntry[]>("list_directory", { path: rootPath });
        if (!cancelled) setTree({ status: "ready", entries });
      } catch (err) {
        if (cancelled) return;
        const message = err instanceof Error ? err.message : String(err);
        // A missing root (e.g. `.ibproject/docs` before a project has
        // adopted the convention) is a normal, empty first-run state, not
        // a real error — the New Folder/New Document form below can
        // create it on the fly.
        setTree(
          isMissingDirectoryError(message) ? { status: "ready", entries: [] } : { status: "error", message },
        );
      }
    }

    void load();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- refreshToken is an intentional manual re-fetch trigger after create/delete, not a value read by this effect.
  }, [rootPath, refreshToken]);

  // A change made outside this app (another editor, a git checkout, a
  // build script) re-lists the tree the exact same way a manual
  // create/delete already does — bumping the same `refreshToken`.
  useEffect(() => onProjectFilesChanged(() => setRefreshToken((t) => t + 1)), []);

  function navigateTo(path: string) {
    setSelectedPath(null);
    setCreating(null);
    setCurrentPath(path);
  }

  function refresh() {
    setRefreshToken((t) => t + 1);
  }

  async function confirmCreate() {
    if (!creating || !currentPath) return;
    const trimmed = newName.trim();
    if (!trimmed) return;
    const needsExtension = creating === "file" && !trimmed.includes(".");
    const name = needsExtension
      ? forceMdExtension
        ? `${trimmed}.md`
        : graphExtension
          ? `${trimmed}.graph.json`
          : trimmed
      : trimmed;
    const path = `${currentPath}/${name}`;

    try {
      await invoke(creating === "file" ? "create_file" : "create_directory", { path });
      setCreating(null);
      setNewName("");
      setCreateError(null);
      refresh();
    } catch (err) {
      setCreateError(err instanceof Error ? err.message : String(err));
    }
  }

  async function handleDelete(entry: FileEntry) {
    try {
      await invoke("delete_path", { path: entry.path });
    } catch {
      // The entry disappearing from the tree on refresh is feedback enough
      // for a failed delete of something already gone; a real permissions
      // failure is rare enough not to warrant its own UI here.
    }
    if (selectedPath === entry.path) setSelectedPath(null);
    if (currentPath === entry.path || currentPath?.startsWith(`${entry.path}/`)) {
      setCurrentPath(rootPath);
    }
    refresh();
  }

  const currentEntries =
    tree.status === "ready" && currentPath
      ? currentPath === rootPath
        ? tree.entries
        : (findEntry(tree.entries, currentPath)?.children ?? [])
      : [];

  const EntryList = variant === "list" ? FileList : FileGrid;

  const browseList = (
    <div className="flex min-h-0 flex-1 flex-col">
      {tree.status === "ready" && rootPath && currentPath && (
        <div className="flex h-8 shrink-0 items-center gap-0.5 overflow-x-auto px-1">
          {pathSegments(rootPath, currentPath).map((segment, i, all) => (
            <div key={segment.path} className="flex shrink-0 items-center gap-0.5">
              {i > 0 && <ChevronRight className="size-3 shrink-0 text-muted-foreground" />}
              <Button
                type="button"
                variant="ghost"
                size="xs"
                disabled={i === all.length - 1}
                onClick={() => navigateTo(segment.path)}
                className="h-6 px-1.5 font-normal disabled:text-foreground disabled:opacity-100"
              >
                {segment.name}
              </Button>
            </div>
          ))}
        </div>
      )}

      {creating && (
        <div className="flex shrink-0 items-center gap-1.5 border-b border-border bg-accent/40 px-2 py-1.5">
          <Input
            autoFocus
            value={newName}
            placeholder={creating === "folder" ? "New folder name" : "New file name"}
            onChange={(e) => {
              setNewName(e.target.value);
              setCreateError(null);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") void confirmCreate();
              if (e.key === "Escape") setCreating(null);
            }}
            className="h-7"
          />
          <Button type="button" size="xs" onClick={() => void confirmCreate()}>
            Create
          </Button>
          <Button type="button" size="xs" variant="ghost" onClick={() => setCreating(null)}>
            Cancel
          </Button>
        </div>
      )}
      {createError && (
        <Alert variant="destructive" className="m-2 w-auto shrink-0">
          <AlertCircle />
          <AlertDescription>{createError}</AlertDescription>
        </Alert>
      )}

      <ScrollArea className="min-h-0 flex-1">
        {tree.status === "empty" && (
          <p className="px-3 py-2 text-sm text-muted-foreground">No project open.</p>
        )}
        {tree.status === "loading" && (
          <div className="flex flex-col gap-2 p-3">
            <Skeleton className="h-4 w-full" />
            <Skeleton className="h-4 w-4/5" />
            <Skeleton className="h-4 w-3/5" />
            <Skeleton className="h-4 w-4/5" />
          </div>
        )}
        {tree.status === "error" && (
          <Alert variant="destructive" className="m-3 w-auto">
            <AlertCircle />
            <AlertDescription>{tree.message}</AlertDescription>
          </Alert>
        )}
        {tree.status === "ready" && (
          <EntryList
            entries={currentEntries}
            selectedPath={selectedPath}
            onOpenFolder={navigateTo}
            onSelectFile={setSelectedPath}
            onDeleteEntry={(entry) => void handleDelete(entry)}
          />
        )}
      </ScrollArea>
    </div>
  );

  const inspectorArea = selectedPath ? (
    <FileInspector key={selectedPath} path={selectedPath} codeEditor={codeEditor} />
  ) : (
    <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 text-center">
      <div className="flex size-10 items-center justify-center rounded-lg border border-border">
        <FolderOpen className="size-5 text-muted-foreground" />
      </div>
      <p className="max-w-xs text-sm text-muted-foreground">Select a file to inspect.</p>
    </div>
  );

  const header = (
    <div data-tauri-drag-region className="flex h-9 shrink-0 items-center justify-between px-3">
      <span className="text-xs font-medium tracking-wide text-muted-foreground">{label}</span>
      {tree.status === "ready" && currentPath && (
        <div className="flex items-center gap-1">
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                type="button"
                variant="ghost"
                size="icon-xs"
                onClick={() => {
                  setCreating("folder");
                  setNewName("");
                  setCreateError(null);
                }}
              >
                <FolderPlus />
              </Button>
            </TooltipTrigger>
            <TooltipContent>New Folder</TooltipContent>
          </Tooltip>
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                type="button"
                variant="ghost"
                size="icon-xs"
                onClick={() => {
                  setCreating("file");
                  setNewName("");
                  setCreateError(null);
                }}
              >
                <FilePlus2 />
              </Button>
            </TooltipTrigger>
            <TooltipContent>
              {forceMdExtension ? "New Document" : graphExtension ? "New Graph" : "New File"}
            </TooltipContent>
          </Tooltip>
        </div>
      )}
    </div>
  );

  // "list" (Design): the doc list and the inspector read as two separate
  // blocks side by side, matching every other panel in the app — resizable
  // and reorderable against each other via the shared `ResizablePanelGroup`
  // system, persisted per `label` so Documents/Business/Graphs/... each
  // remember their own split independently. "grid" (Files): one block,
  // Finder-style — icon grid on top, inspector below, separated by a
  // hairline divider inside the same card.
  if (variant === "list") {
    return (
      <ResizablePanelGroup
        className={className}
        storageKey={`filebrowser.${label.toLowerCase().replace(/[^a-z0-9]+/g, "-")}`}
        panels={[
          {
            id: "list",
            defaultPercent: 25,
            minPercent: 15,
            content: (
              <div className="flex h-full min-w-0 flex-col overflow-hidden rounded-xl border border-border bg-card">
                {header}
                {browseList}
              </div>
            ),
          },
          {
            id: "inspector",
            defaultPercent: 75,
            minPercent: 25,
            content: (
              <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-xl border border-border bg-card">
                {inspectorArea}
              </div>
            ),
          },
        ]}
      />
    );
  }

  return (
    <div
      className={cn(
        "flex h-full min-w-0 flex-1 flex-col overflow-hidden rounded-xl border border-border bg-card",
        className,
      )}
    >
      {header}
      <div className="flex min-h-0 flex-1 flex-col border-b border-border">{browseList}</div>
      {inspectorArea}
    </div>
  );
}
