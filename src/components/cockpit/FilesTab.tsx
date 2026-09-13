import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { FolderOpen } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { FileTree } from "@/components/cockpit/FileTree";
import { MarkdownEditor } from "@/components/cockpit/MarkdownEditor";
import { MarkdownPreview } from "@/components/cockpit/MarkdownPreview";
import type { FileEntry } from "@/types/fs";

interface FilesTabProps {
  projectPath: string | null;
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

function FileInspector({ path }: { path: string }) {
  const [state, setState] = useState<InspectorState>({ status: "loading" });
  const [draft, setDraft] = useState("");
  const [saveError, setSaveError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

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

  const editable = isMarkdown(path);
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
      <div className="flex h-8 shrink-0 items-center justify-between border-b border-border px-3">
        <div className="flex min-w-0 items-center gap-2">
          <span className="truncate text-sm text-foreground/90" title={path}>
            {fileName(path)}
          </span>
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

      {state.status === "binary" && (
        <div className="flex flex-1 items-center justify-center">
          <p className="text-sm text-muted-foreground">
            Can't preview this file — it looks like binary content.
          </p>
        </div>
      )}

      {state.status === "error" && (
        <div className="flex flex-1 items-center justify-center">
          <p className="text-sm text-destructive">{state.message}</p>
        </div>
      )}

      {state.status === "ready" && editable && (
        <Tabs defaultValue="edit" className="min-h-0 flex-1 flex-col gap-0">
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
              key={path}
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

      {state.status === "ready" && !editable && (
        <ScrollArea className="min-h-0 flex-1">
          <pre className="whitespace-pre-wrap p-3 font-mono text-xs text-foreground/90">
            {state.savedContent}
          </pre>
        </ScrollArea>
      )}
    </div>
  );
}

export function FilesTab({ projectPath }: FilesTabProps) {
  const [tree, setTree] = useState<TreeState>({ status: "empty" });
  const [selectedPath, setSelectedPath] = useState<string | null>(null);

  useEffect(() => {
    setSelectedPath(null);

    if (!projectPath) {
      setTree({ status: "empty" });
      return;
    }

    let cancelled = false;
    setTree({ status: "loading" });

    async function load() {
      try {
        const entries = await invoke<FileEntry[]>("list_directory", { path: projectPath });
        if (!cancelled) setTree({ status: "ready", entries });
      } catch (err) {
        if (!cancelled) {
          setTree({
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
  }, [projectPath]);

  return (
    <div className="flex min-h-0 flex-1">
      <div className="flex h-full w-[220px] shrink-0 flex-col border-r border-border">
        <ScrollArea className="min-h-0 flex-1">
          <div className="py-1">
            {tree.status === "empty" && (
              <p className="px-3 py-2 text-sm text-muted-foreground">No project open.</p>
            )}
            {tree.status === "loading" && (
              <p className="px-3 py-2 text-sm text-muted-foreground">loading…</p>
            )}
            {tree.status === "error" && (
              <p className="px-3 py-2 text-sm text-destructive">{tree.message}</p>
            )}
            {tree.status === "ready" && (
              <FileTree
                entries={tree.entries}
                selectedPath={selectedPath}
                onSelectFile={(entry) => setSelectedPath(entry.path)}
              />
            )}
          </div>
        </ScrollArea>
      </div>
      {selectedPath ? (
        <FileInspector key={selectedPath} path={selectedPath} />
      ) : (
        <div className="flex flex-1 flex-col items-center justify-center gap-2 text-center">
          <div className="flex size-10 items-center justify-center border border-border">
            <FolderOpen className="size-5 text-muted-foreground" />
          </div>
          <p className="max-w-xs text-sm text-muted-foreground">Select a file to inspect.</p>
        </div>
      )}
    </div>
  );
}
