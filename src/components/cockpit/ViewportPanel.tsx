import { useEffect, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Box, FileWarning } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { MarkdownEditor } from "@/components/cockpit/MarkdownEditor";
import { MarkdownPreview } from "@/components/cockpit/MarkdownPreview";

type FileLoadState =
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "ready"; savedContent: string };

interface ViewportPanelProps {
  selectedPath: string | null;
}

function fileName(path: string): string {
  return path.split("/").pop() ?? path;
}

function isMarkdown(path: string): boolean {
  return /\.md$/i.test(path);
}

/** Toolbar row shared by the empty state, the unsupported-file state, and
 * the editor state — matches the h-8 header row used by every other panel
 * (FileBrowserPanel, AgentPanel). */
function ViewportHeader({ children }: { children: ReactNode }) {
  return (
    <div className="flex h-8 shrink-0 items-center justify-between border-b border-border px-3">
      {children}
    </div>
  );
}

function ViewportShell({ children }: { children: ReactNode }) {
  return (
    <div className="flex h-full min-w-0 flex-1 flex-col border border-border bg-card">
      {children}
    </div>
  );
}

export function ViewportPanel({ selectedPath }: ViewportPanelProps) {
  const [state, setState] = useState<FileLoadState>({ status: "loading" });
  const [draft, setDraft] = useState("");
  const [saveError, setSaveError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!selectedPath || !isMarkdown(selectedPath)) {
      return;
    }

    let cancelled = false;
    setState({ status: "loading" });
    setSaveError(null);

    async function load() {
      try {
        const contents = await invoke<string>("read_file", {
          path: selectedPath,
        });
        if (!cancelled) {
          setState({ status: "ready", savedContent: contents });
          setDraft(contents);
        }
      } catch (err) {
        if (!cancelled) {
          setState({
            status: "error",
            message: err instanceof Error ? err.message : String(err),
          });
        }
      }
    }

    void load();

    return () => {
      cancelled = true;
    };
  }, [selectedPath]);

  const dirty = state.status === "ready" && draft !== state.savedContent;

  async function save() {
    if (!selectedPath || state.status !== "ready" || saving) return;
    setSaving(true);
    setSaveError(null);
    try {
      await invoke("write_file", { path: selectedPath, contents: draft });
      setState({ status: "ready", savedContent: draft });
    } catch (err) {
      setSaveError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  }

  // Empty state — nothing selected yet.
  if (!selectedPath) {
    return (
      <ViewportShell>
        <ViewportHeader>
          <span className="truncate text-sm text-foreground/90">
            project.godot
          </span>
        </ViewportHeader>
        <div className="flex flex-1 flex-col items-center justify-center gap-2 text-center">
          <div className="flex size-10 items-center justify-center border border-border">
            <Box className="size-5 text-muted-foreground" />
          </div>
          <h2 className="text-lg font-medium tracking-tight">Project</h2>
          <p className="max-w-xs text-sm text-muted-foreground">
            Build activity, previews, and diagnostics for the open project
            will appear here once the Godot handoff is wired up.
          </p>
        </div>
      </ViewportShell>
    );
  }

  // A file is selected, but it isn't something this editor understands.
  if (!isMarkdown(selectedPath)) {
    return (
      <ViewportShell>
        <ViewportHeader>
          <span
            className="truncate text-sm text-foreground/90"
            title={selectedPath}
          >
            {fileName(selectedPath)}
          </span>
        </ViewportHeader>
        <div className="flex flex-1 flex-col items-center justify-center gap-2 text-center">
          <div className="flex size-10 items-center justify-center border border-border">
            <FileWarning className="size-5 text-muted-foreground" />
          </div>
          <h2 className="text-lg font-medium tracking-tight">
            Can't edit this file type yet
          </h2>
          <p className="max-w-xs text-sm text-muted-foreground">
            The GDD editor only opens markdown (.md) files for now. Select a
            .md file from the project's docs folder.
          </p>
        </div>
      </ViewportShell>
    );
  }

  return (
    <ViewportShell>
      <ViewportHeader>
        <div className="flex min-w-0 items-center gap-2">
          <span
            className="truncate text-sm text-foreground/90"
            title={selectedPath}
          >
            {fileName(selectedPath)}
          </span>
          {dirty && <Badge variant="outline">unsaved</Badge>}
        </div>
        <Button
          size="xs"
          variant="secondary"
          onClick={() => void save()}
          disabled={state.status !== "ready" || !dirty || saving}
        >
          {saving ? "Saving…" : "Save"}
        </Button>
      </ViewportHeader>

      {saveError && (
        <p className="shrink-0 border-b border-border bg-destructive/10 px-3 py-1.5 text-xs text-destructive">
          Save failed: {saveError}
        </p>
      )}

      {state.status === "loading" && (
        <div className="flex flex-1 items-center justify-center">
          <p className="text-sm text-muted-foreground">loading…</p>
        </div>
      )}

      {state.status === "error" && (
        <div className="flex flex-1 items-center justify-center">
          <p className="text-sm text-destructive">{state.message}</p>
        </div>
      )}

      {state.status === "ready" && (
        <Tabs
          defaultValue="edit"
          className="min-h-0 flex-1 flex-col gap-0"
        >
          <TabsList variant="line" className="mx-2 mt-1 h-7 w-fit self-start">
            <TabsTrigger value="edit">Edit</TabsTrigger>
            <TabsTrigger value="preview">Preview</TabsTrigger>
          </TabsList>
          <TabsContent
            value="edit"
            forceMount
            className="min-h-0 flex-1 overflow-hidden data-[state=inactive]:hidden"
          >
            <MarkdownEditor
              key={selectedPath}
              initialValue={state.savedContent}
              onChange={setDraft}
              onSave={() => void save()}
            />
          </TabsContent>
          <TabsContent
            value="preview"
            forceMount
            className="min-h-0 flex-1 overflow-auto data-[state=inactive]:hidden"
          >
            <MarkdownPreview content={draft} />
          </TabsContent>
        </Tabs>
      )}
    </ViewportShell>
  );
}
