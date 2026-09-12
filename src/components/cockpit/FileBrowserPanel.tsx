import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ScrollArea } from "@/components/ui/scroll-area";
import { FileTree } from "@/components/cockpit/FileTree";
import type { FileEntry } from "@/types/fs";

type LoadState =
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "ready"; entries: FileEntry[] };

interface FileBrowserPanelProps {
  projectPath: string | null;
  selectedPath?: string | null;
  onSelectFile?: (entry: FileEntry) => void;
}

export function FileBrowserPanel({
  projectPath,
  selectedPath,
  onSelectFile,
}: FileBrowserPanelProps) {
  const [state, setState] = useState<LoadState>({ status: "loading" });

  useEffect(() => {
    if (!projectPath) {
      return;
    }

    let cancelled = false;
    setState({ status: "loading" });

    async function load() {
      try {
        const entries = await invoke<FileEntry[]>("list_directory", {
          path: projectPath,
        });
        if (!cancelled) {
          setState({ status: "ready", entries });
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
  }, [projectPath]);

  return (
    <div className="flex h-full w-[240px] shrink-0 flex-col border border-border bg-card">
      <div className="flex h-8 shrink-0 items-center border-b border-border px-3">
        <span className="text-xs font-medium tracking-wide text-muted-foreground uppercase">
          Files
        </span>
      </div>
      <ScrollArea className="min-h-0 flex-1">
        <div className="py-1">
          {state.status === "loading" && (
            <p className="px-3 py-1 text-sm text-muted-foreground">
              loading…
            </p>
          )}
          {state.status === "error" && (
            <p className="px-3 py-1 text-sm text-destructive">
              {state.message}
            </p>
          )}
          {state.status === "ready" && (
            <FileTree
              entries={state.entries}
              selectedPath={selectedPath}
              onSelectFile={onSelectFile}
            />
          )}
        </div>
      </ScrollArea>
    </div>
  );
}
