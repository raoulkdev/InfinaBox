import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AlertCircle } from "lucide-react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Skeleton } from "@/components/ui/skeleton";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { isEmptyRepoError, isMissingGitRepoError } from "@/lib/backend-errors";
import { onProjectFilesChanged } from "@/lib/fs-watch";
import type { CommitInfo } from "@/types/git";
import type { FileEntry } from "@/types/fs";

interface OverviewTabProps {
  projectPath: string | null;
}

type GitState =
  | { status: "loading" }
  | { status: "empty" }
  | { status: "not-git" }
  | { status: "empty-repo" }
  | { status: "error"; message: string }
  | { status: "ready"; branch: string; commitCount: number; lastCommit: CommitInfo | null };

type FileCountState =
  | { status: "loading" }
  | { status: "empty" }
  | { status: "error"; message: string }
  | { status: "ready"; count: number };

function countFiles(entries: FileEntry[]): number {
  let count = 0;
  for (const entry of entries) {
    count += entry.is_dir ? countFiles(entry.children ?? []) : 1;
  }
  return count;
}

function relativeTime(iso: string): string {
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return iso;
  const diffMin = Math.round((Date.now() - then) / 60000);
  if (diffMin < 1) return "just now";
  if (diffMin < 60) return `${diffMin}m ago`;
  const diffHr = Math.round(diffMin / 60);
  if (diffHr < 24) return `${diffHr}h ago`;
  return `${Math.round(diffHr / 24)}d ago`;
}

function StatRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex min-w-0 items-center justify-between border-b border-border px-3 py-2 last:border-b-0">
      <span className="shrink-0 text-sm text-muted-foreground">{label}</span>
      <Tooltip>
        <TooltipTrigger asChild>
          <span className="min-w-0 max-w-[60%] truncate text-sm text-foreground/90">{value}</span>
        </TooltipTrigger>
        <TooltipContent>{value}</TooltipContent>
      </Tooltip>
    </div>
  );
}

export function OverviewTab({ projectPath }: OverviewTabProps) {
  const [git, setGit] = useState<GitState>({ status: "loading" });
  const [files, setFiles] = useState<FileCountState>({ status: "loading" });
  const [refreshToken, setRefreshToken] = useState(0);

  // Branch/commit/file counts all come from the filesystem (git's own
  // state lives under `.git`, which a commit made in the embedded terminal
  // changes just like any other write) — a change anywhere in the project
  // re-fetches both, the same way this effect already does on mount.
  useEffect(() => {
    if (!projectPath) return;
    return onProjectFilesChanged(() => setRefreshToken((t) => t + 1));
  }, [projectPath]);

  useEffect(() => {
    if (!projectPath) {
      setGit({ status: "empty" });
      setFiles({ status: "empty" });
      return;
    }

    let cancelled = false;
    setGit({ status: "loading" });
    setFiles({ status: "loading" });

    async function loadGit() {
      try {
        const [branch, commits] = await Promise.all([
          invoke<string>("current_branch", { path: projectPath }),
          invoke<CommitInfo[]>("list_recent_commits", { path: projectPath }),
        ]);
        if (!cancelled) {
          setGit({
            status: "ready",
            branch,
            commitCount: commits.length,
            lastCommit: commits[0] ?? null,
          });
        }
      } catch (err) {
        if (!cancelled) {
          const message = err instanceof Error ? err.message : String(err);
          if (isMissingGitRepoError(message)) {
            setGit({ status: "not-git" });
          } else if (isEmptyRepoError(message)) {
            setGit({ status: "empty-repo" });
          } else {
            setGit({ status: "error", message });
          }
        }
      }
    }

    async function loadFiles() {
      try {
        const entries = await invoke<FileEntry[]>("list_directory", { path: projectPath });
        if (!cancelled) setFiles({ status: "ready", count: countFiles(entries) });
      } catch (err) {
        if (!cancelled) {
          setFiles({
            status: "error",
            message: err instanceof Error ? err.message : String(err),
          });
        }
      }
    }

    void loadGit();
    void loadFiles();

    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- refreshToken is an intentional manual re-fetch trigger (see the effect above), not a value read by this one.
  }, [projectPath, refreshToken]);

  return (
    <div className="flex min-w-0 flex-1 flex-col overflow-y-auto">
      <div className="border-b border-border">
        {git.status === "loading" && (
          <div className="flex flex-col gap-2 p-3">
            <Skeleton className="h-4 w-2/3" />
            <Skeleton className="h-4 w-1/3" />
          </div>
        )}
        {git.status === "empty" && (
          <p className="px-3 py-2 text-sm text-muted-foreground">No project open.</p>
        )}
        {git.status === "not-git" && (
          <p className="px-3 py-2 text-sm text-muted-foreground">
            This folder isn't a Git repository yet.
          </p>
        )}
        {git.status === "empty-repo" && (
          <p className="px-3 py-2 text-sm text-muted-foreground">
            This repository has no commits yet.
          </p>
        )}
        {git.status === "error" && (
          <Alert variant="destructive" className="m-3 w-auto">
            <AlertCircle />
            <AlertDescription>{git.message}</AlertDescription>
          </Alert>
        )}
        {git.status === "ready" && (
          <>
            <StatRow label="Branch" value={git.branch} />
            <StatRow label="Commits" value={String(git.commitCount)} />
            {git.lastCommit && (
              <StatRow
                label="Last commit"
                value={`${git.lastCommit.summary} — ${git.lastCommit.author_name}, ${relativeTime(git.lastCommit.timestamp)}`}
              />
            )}
          </>
        )}
      </div>
      <div>
        {files.status === "loading" && (
          <div className="p-3">
            <Skeleton className="h-4 w-1/3" />
          </div>
        )}
        {files.status === "empty" && (
          <p className="px-3 py-2 text-sm text-muted-foreground">No project open.</p>
        )}
        {files.status === "error" && (
          <Alert variant="destructive" className="m-3 w-auto">
            <AlertCircle />
            <AlertDescription>{files.message}</AlertDescription>
          </Alert>
        )}
        {files.status === "ready" && <StatRow label="Files" value={String(files.count)} />}
      </div>
    </div>
  );
}
