import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AlertCircle, ChevronRight } from "lucide-react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Skeleton } from "@/components/ui/skeleton";
import { isEmptyRepoError, isMissingGitRepoError } from "@/lib/backend-errors";
import { onProjectFilesChanged } from "@/lib/fs-watch";
import { cn } from "@/lib/utils";
import type { CommitInfo, FileChange } from "@/types/git";

interface ChangesTabProps {
  projectPath: string | null;
}

type ChangesState =
  | { status: "empty" }
  | { status: "loading" }
  | { status: "not-git" }
  | { status: "error"; message: string }
  | { status: "ready"; commits: CommitInfo[] };

function fileChangeLabel(change: FileChange): string {
  if (change.status === "renamed" || change.status === "copied") {
    return `${change.from} → ${change.to}`;
  }
  return change.path;
}

function CommitRow({ commit }: { commit: CommitInfo }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="border-b border-border">
      <Button
        type="button"
        variant="ghost"
        onClick={() => setOpen((v) => !v)}
        className="h-auto w-full flex-col items-stretch gap-1 rounded-none px-3 py-2 text-left font-normal whitespace-normal hover:bg-accent"
      >
        <div className="flex w-full items-center gap-2">
          <ChevronRight
            className={cn(
              "size-3.5 shrink-0 text-muted-foreground transition-transform",
              open && "rotate-90",
            )}
          />
          <span className="shrink-0 font-mono text-xs text-muted-foreground">
            {commit.short_sha}
          </span>
          <span className="min-w-0 flex-1 truncate text-xs text-muted-foreground">
            {commit.author_name}
          </span>
        </div>
        <span className="pl-[22px] text-sm text-wrap break-words text-foreground/90">
          {commit.summary}
        </span>
      </Button>
      {open && (
        <div className="flex flex-col gap-1 border-t border-border bg-background px-3 py-2 pl-9">
          {commit.files_changed.length === 0 && (
            <span className="text-xs text-muted-foreground">No file changes recorded.</span>
          )}
          {commit.files_changed.map((change, i) => (
            <span key={i} className="font-mono text-xs break-all text-muted-foreground">
              [{change.status}] {fileChangeLabel(change)}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

export function ChangesTab({ projectPath }: ChangesTabProps) {
  const [state, setState] = useState<ChangesState>({ status: "empty" });
  const [refreshToken, setRefreshToken] = useState(0);

  useEffect(() => {
    if (!projectPath) return;
    return onProjectFilesChanged(() => setRefreshToken((t) => t + 1));
  }, [projectPath]);

  useEffect(() => {
    if (!projectPath) {
      setState({ status: "empty" });
      return;
    }

    let cancelled = false;
    setState({ status: "loading" });

    async function load() {
      try {
        const commits = await invoke<CommitInfo[]>("list_recent_commits", {
          path: projectPath,
        });
        if (!cancelled) setState({ status: "ready", commits });
      } catch (err) {
        if (cancelled) return;
        const message = err instanceof Error ? err.message : String(err);
        if (isMissingGitRepoError(message)) {
          setState({ status: "not-git" });
        } else if (isEmptyRepoError(message)) {
          // A real Git repo with zero commits genuinely has no commits —
          // render it the same way a repo that fetched zero commits would.
          setState({ status: "ready", commits: [] });
        } else {
          setState({ status: "error", message });
        }
      }
    }

    void load();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- refreshToken is an intentional manual re-fetch trigger (see the effect above), not a value read by this one.
  }, [projectPath, refreshToken]);

  return (
    <ScrollArea className="min-h-0 flex-1">
      {state.status === "empty" && (
        <p className="px-3 py-2 text-sm text-muted-foreground">No project open.</p>
      )}
      {state.status === "loading" && (
        <div className="flex flex-col gap-2 p-3">
          <Skeleton className="h-4 w-full" />
          <Skeleton className="h-4 w-4/5" />
          <Skeleton className="h-4 w-3/5" />
        </div>
      )}
      {state.status === "not-git" && (
        <p className="px-3 py-2 text-sm text-muted-foreground">
          This folder isn't a Git repository yet.
        </p>
      )}
      {state.status === "error" && (
        <Alert variant="destructive" className="m-3 w-auto">
          <AlertCircle />
          <AlertDescription>{state.message}</AlertDescription>
        </Alert>
      )}
      {state.status === "ready" && state.commits.length === 0 && (
        <p className="px-3 py-2 text-sm text-muted-foreground">No commits yet.</p>
      )}
      {state.status === "ready" &&
        state.commits.map((c) => <CommitRow key={c.sha} commit={c} />)}
    </ScrollArea>
  );
}
