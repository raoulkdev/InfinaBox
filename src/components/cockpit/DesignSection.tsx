import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { FileText, FolderOpen } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { MarkdownEditor } from "@/components/cockpit/MarkdownEditor";
import { MarkdownPreview } from "@/components/cockpit/MarkdownPreview";
import { cn } from "@/lib/utils";
import type { FileEntry } from "@/types/fs";

interface DesignSectionProps {
  projectPath: string | null;
}

interface GddDoc {
  name: string;
  path: string;
}

type OutlineState =
  | { status: "empty" } // no project open at all
  | { status: "loading" }
  | { status: "no-folder" } // docs/gdd doesn't exist (or otherwise unreadable)
  | { status: "ready"; docs: GddDoc[] };

type DocState =
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "ready"; savedContent: string };

function docTitle(name: string): string {
  return name.replace(/\.md$/i, "");
}

/** Left-hand outline column — a flat list of `docs/gdd/*.md` documents,
 * not a general file browser (GDDs are the product's own stated
 * convention, not arbitrary project files). */
function GddOutline({
  state,
  selectedPath,
  onSelect,
}: {
  state: OutlineState;
  selectedPath: string | null;
  onSelect: (doc: GddDoc) => void;
}) {
  return (
    <div className="flex h-full w-[220px] shrink-0 flex-col border-r border-border bg-card">
      <div className="flex h-8 shrink-0 items-center border-b border-border px-3">
        <span className="text-xs font-medium tracking-wide text-muted-foreground uppercase">
          Outline
        </span>
      </div>
      <ScrollArea className="min-h-0 flex-1">
        <div className="py-1">
          {state.status === "empty" && (
            <p className="px-3 py-2 text-sm text-muted-foreground">
              No project open.
            </p>
          )}
          {state.status === "loading" && (
            <p className="px-3 py-2 text-sm text-muted-foreground">
              loading…
            </p>
          )}
          {state.status === "no-folder" && (
            <p className="px-3 py-2 text-sm text-muted-foreground">
              No docs/gdd folder found in this project yet.
            </p>
          )}
          {state.status === "ready" && state.docs.length === 0 && (
            <p className="px-3 py-2 text-sm text-muted-foreground">
              docs/gdd has no markdown documents yet.
            </p>
          )}
          {state.status === "ready" &&
            state.docs.map((doc) => {
              const selected = doc.path === selectedPath;
              return (
                <button
                  key={doc.path}
                  type="button"
                  onClick={() => onSelect(doc)}
                  className={cn(
                    "flex w-full items-center gap-1.5 px-3 py-1 text-left text-sm text-foreground/90 hover:bg-accent",
                    selected && "bg-accent text-foreground",
                  )}
                >
                  <FileText className="size-3.5 shrink-0 text-muted-foreground" />
                  <span className="truncate">{docTitle(doc.name)}</span>
                </button>
              );
            })}
        </div>
      </ScrollArea>
    </div>
  );
}

/** Right-hand content area — loads the selected doc's contents and hosts
 * the Edit/Preview tabs, ported from the old ViewportPanel markdown-editing
 * logic (now scoped to GDD docs instead of arbitrary files). */
function DocEditor({ doc }: { doc: GddDoc }) {
  const [state, setState] = useState<DocState>({ status: "loading" });
  const [draft, setDraft] = useState("");
  const [saveError, setSaveError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setState({ status: "loading" });
    setSaveError(null);

    async function load() {
      try {
        const contents = await invoke<string>("read_file", { path: doc.path });
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
  }, [doc.path]);

  const dirty = state.status === "ready" && draft !== state.savedContent;

  async function save() {
    if (state.status !== "ready" || saving) return;
    setSaving(true);
    setSaveError(null);
    try {
      await invoke("write_file", { path: doc.path, contents: draft });
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
          <span className="truncate text-sm text-foreground/90" title={doc.path}>
            {docTitle(doc.name)}
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

      {state.status === "error" && (
        <div className="flex flex-1 items-center justify-center">
          <p className="text-sm text-destructive">{state.message}</p>
        </div>
      )}

      {state.status === "ready" && (
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
              key={doc.path}
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
    </div>
  );
}

export function DesignSection({ projectPath }: DesignSectionProps) {
  const [outline, setOutline] = useState<OutlineState>(
    projectPath ? { status: "loading" } : { status: "empty" },
  );
  const [selectedDoc, setSelectedDoc] = useState<GddDoc | null>(null);

  useEffect(() => {
    setSelectedDoc(null);

    if (!projectPath) {
      setOutline({ status: "empty" });
      return;
    }

    let cancelled = false;
    setOutline({ status: "loading" });

    async function load() {
      try {
        const entries = await invoke<FileEntry[]>("list_directory", {
          path: `${projectPath}/docs/gdd`,
        });
        if (cancelled) return;
        const docs = entries
          .filter((entry) => !entry.is_dir && /\.md$/i.test(entry.name))
          .map((entry) => ({ name: entry.name, path: entry.path }));
        setOutline({ status: "ready", docs });
      } catch {
        // `list_directory` errors for a nonexistent path — the common case
        // for projects that haven't adopted the docs/gdd convention yet.
        if (!cancelled) setOutline({ status: "no-folder" });
      }
    }

    void load();

    return () => {
      cancelled = true;
    };
  }, [projectPath]);

  return (
    <div className="flex min-h-0 flex-1">
      <GddOutline state={outline} selectedPath={selectedDoc?.path ?? null} onSelect={setSelectedDoc} />
      {selectedDoc ? (
        <DocEditor doc={selectedDoc} />
      ) : (
        <div className="flex flex-1 flex-col items-center justify-center gap-2 text-center">
          <div className="flex size-10 items-center justify-center border border-border">
            <FolderOpen className="size-5 text-muted-foreground" />
          </div>
          <p className="max-w-xs text-sm text-muted-foreground">
            Select a document from the outline.
          </p>
        </div>
      )}
    </div>
  );
}
