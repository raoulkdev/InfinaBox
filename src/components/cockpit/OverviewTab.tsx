import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { isEmptyRepoError, isMissingGitRepoError } from "@/lib/backend-errors";
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
    <div className="flex items-center justify-between border-b border-border px-3 py-2 last:border-b-0">
      <span className="text-sm text-muted-foreground">{label}</span>
      <span className="max-w-[60%] truncate text-sm text-foreground/90" title={value}>
        {value}
      </span>
    </div>
  );
}

export function OverviewTab({ projectPath }: OverviewTabProps) {
  const [git, setGit] = useState<GitState>({ status: "loading" });
  const [files, setFiles] = useState<FileCountState>({ status: "loading" });

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
  }, [projectPath]);

  return (
    <div className="flex flex-1 flex-col overflow-y-auto">
      <div className="border-b border-border">
        {git.status === "loading" && (
          <p className="px-3 py-2 text-sm text-muted-foreground">loading…</p>
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
          <p className="px-3 py-2 text-sm text-destructive">{git.message}</p>
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
          <p className="px-3 py-2 text-sm text-muted-foreground">loading…</p>
        )}
        {files.status === "empty" && (
          <p className="px-3 py-2 text-sm text-muted-foreground">No project open.</p>
        )}
        {files.status === "error" && (
          <p className="px-3 py-2 text-sm text-destructive">{files.message}</p>
        )}
        {files.status === "ready" && <StatRow label="Files" value={String(files.count)} />}
      </div>
    </div>
  );
}
