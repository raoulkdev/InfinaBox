// Studio's History panel: snapshots, undo, go back.
//
// Snapshots are git commits underneath (see crates/core/src/snapshot.rs),
// but this is the novice-facing view, so it never says "git" or "commit"
// (product spec §7.5) — just what changed, when, and a safe way back. Every
// row is a real snapshot from `snapshot_list`; nothing here is invented.

import { useCallback, useEffect, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { AlertCircle, History, Loader2, Save, Sparkles, Undo2 } from "lucide-react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { fadeRise, fadeTransition } from "@/lib/motion";
import {
  onAgentTurnFinished,
  onSnapshotsChanged,
  snapshotList,
  snapshotRestore,
  snapshotUndoLast,
} from "@/lib/studio-api";
import type { Snapshot } from "@/lib/studio-types";
import { GoBackDialog } from "./GoBackDialog";
import { absoluteTime, relativeTime } from "./relative-time";

export interface HistoryPanelProps {
  projectPath: string;
}

/** How many snapshots to list. When a project has more, the panel says it's
 * showing only the latest ones rather than implying that's all of them. */
const LIST_LIMIT = 100;

type ListState =
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "ready"; snapshots: Snapshot[] };

type Notice = { tone: "info" | "error"; text: string };

function errorText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

function nowSeconds(): number {
  return Math.floor(Date.now() / 1000);
}

function filesChangedLabel(n: number): string {
  if (n === 0) return "No files changed";
  return n === 1 ? "1 file changed" : `${n} files changed`;
}

interface SnapshotRowProps {
  snapshot: Snapshot;
  latest: boolean;
  now: number;
  disabled: boolean;
  onGoBack: (snapshot: Snapshot) => void;
}

function SnapshotRow({ snapshot, latest, now, disabled, onGoBack }: SnapshotRowProps) {
  // `thread_id` is set only on snapshots an AI chat turn produced, so the
  // icon tells "the AI did this" apart from a save point made any other way.
  const byAi = snapshot.thread_id !== null;
  const Icon = byAi ? Sparkles : Save;
  return (
    <li className="group flex items-start gap-2 border-b border-border px-3 py-2 last:border-b-0 hover:bg-accent/50">
      <Tooltip>
        <TooltipTrigger asChild>
          <Icon className="mt-0.5 size-3.5 shrink-0 text-muted-foreground" />
        </TooltipTrigger>
        <TooltipContent>{byAi ? "Made by your AI" : "Save point"}</TooltipContent>
      </Tooltip>
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <div className="flex min-w-0 items-center gap-1.5">
          <span className="min-w-0 truncate text-sm text-foreground/90">{snapshot.title}</span>
          {latest && <Badge variant="outline">Latest</Badge>}
        </div>
        <span className="text-xs text-muted-foreground">
          <Tooltip>
            <TooltipTrigger asChild>
              <span>{relativeTime(snapshot.timestamp, now)}</span>
            </TooltipTrigger>
            <TooltipContent>{absoluteTime(snapshot.timestamp)}</TooltipContent>
          </Tooltip>
          {" · "}
          {filesChangedLabel(snapshot.files_changed)}
        </span>
      </div>
      <Button
        type="button"
        size="xs"
        variant="ghost"
        disabled={disabled}
        className="shrink-0 opacity-0 group-focus-within:opacity-100 group-hover:opacity-100 focus-visible:opacity-100"
        aria-label={`Go back to this point: ${snapshot.title}`}
        onClick={() => onGoBack(snapshot)}
      >
        <History data-icon="inline-start" />
        Go back
      </Button>
    </li>
  );
}

export function HistoryPanel({ projectPath }: HistoryPanelProps) {
  const [list, setList] = useState<ListState>({ status: "loading" });
  const [refreshToken, setRefreshToken] = useState(0);
  const [busy, setBusy] = useState<"undo" | "restore" | null>(null);
  const [confirming, setConfirming] = useState<Snapshot | null>(null);
  const [notice, setNotice] = useState<Notice | null>(null);
  const [now, setNow] = useState(nowSeconds);

  const refresh = useCallback(() => setRefreshToken((t) => t + 1), []);

  // Both events mean the list on disk may have changed: a restore/undo
  // (from here, the AI, or anywhere else) emits `snapshots-changed`, and an
  // AI turn's automatic snapshot lands just before `agent-turn-finished`.
  useEffect(() => {
    const offSnapshots = onSnapshotsChanged(refresh);
    const offTurn = onAgentTurnFinished(refresh);
    return () => {
      offSnapshots();
      offTurn();
    };
  }, [refresh]);

  // Keeps "2 minutes ago" honest while the panel just sits open.
  useEffect(() => {
    const timer = window.setInterval(() => setNow(nowSeconds()), 30_000);
    return () => window.clearInterval(timer);
  }, []);

  useEffect(() => {
    setList({ status: "loading" });
    setNotice(null);
  }, [projectPath]);

  useEffect(() => {
    let cancelled = false;
    snapshotList(projectPath, LIST_LIMIT)
      .then((snapshots) => {
        if (cancelled) return;
        setList({ status: "ready", snapshots });
        setNow(nowSeconds());
      })
      .catch((err) => {
        if (!cancelled) setList({ status: "error", message: errorText(err) });
      });
    return () => {
      cancelled = true;
    };
  }, [projectPath, refreshToken]);

  // Notices are confirmations, not state — they fade on their own.
  useEffect(() => {
    if (!notice || notice.tone === "error") return;
    const timer = window.setTimeout(() => setNotice(null), 5000);
    return () => window.clearTimeout(timer);
  }, [notice]);

  const snapshots = list.status === "ready" ? list.snapshots : [];

  async function undoLast() {
    const latestTitle = snapshots[0]?.title;
    setBusy("undo");
    setNotice(null);
    try {
      const result = await snapshotUndoLast(projectPath);
      setNotice(
        result === null
          ? { tone: "info", text: "There's nothing to undo yet." }
          : {
              tone: "info",
              text: latestTitle ? `Undid "${latestTitle}".` : "Undid the last change.",
            },
      );
    } catch (err) {
      setNotice({ tone: "error", text: `Couldn't undo: ${errorText(err)}` });
    } finally {
      setBusy(null);
      refresh();
    }
  }

  async function goBack(snapshot: Snapshot) {
    setConfirming(null);
    setBusy("restore");
    setNotice(null);
    try {
      await snapshotRestore(projectPath, snapshot.id);
      setNotice({ tone: "info", text: `Went back to "${snapshot.title}".` });
    } catch (err) {
      setNotice({ tone: "error", text: `Couldn't go back: ${errorText(err)}` });
    } finally {
      setBusy(null);
      refresh();
    }
  }

  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden rounded-xl border border-border bg-card">
      <div data-tauri-drag-region className="flex h-9 shrink-0 items-center gap-2 px-3">
        <span className="text-xs font-medium tracking-wide text-muted-foreground">History</span>
        <div className="flex-1" />
        <Button
          type="button"
          size="xs"
          variant="outline"
          disabled={busy !== null || snapshots.length === 0}
          onClick={() => void undoLast()}
        >
          {busy === "undo" ? (
            <Loader2 data-icon="inline-start" className="animate-spin" />
          ) : (
            <Undo2 data-icon="inline-start" />
          )}
          Undo last change
        </Button>
      </div>

      <AnimatePresence initial={false}>
        {notice && (
          <motion.div
            key={notice.text}
            {...fadeRise}
            transition={fadeTransition}
            className="shrink-0 px-3 pb-2"
          >
            {notice.tone === "error" ? (
              <Alert variant="destructive">
                <AlertCircle />
                <AlertDescription className="break-words">{notice.text}</AlertDescription>
              </Alert>
            ) : (
              <p className="text-xs text-muted-foreground">{notice.text}</p>
            )}
          </motion.div>
        )}
      </AnimatePresence>

      <div className="min-h-0 flex-1 overflow-auto border-t border-border">
        {list.status === "loading" && (
          <div className="flex flex-col gap-2 p-3">
            <Skeleton className="h-4 w-full" />
            <Skeleton className="h-4 w-4/5" />
            <Skeleton className="h-4 w-3/5" />
          </div>
        )}
        {list.status === "error" && (
          <div className="flex flex-col gap-2 p-3">
            <Alert variant="destructive">
              <AlertCircle />
              <AlertDescription className="break-words">
                Couldn't load your history: {list.message}
              </AlertDescription>
            </Alert>
            <Button type="button" size="sm" variant="outline" className="self-start" onClick={refresh}>
              Try again
            </Button>
          </div>
        )}
        {list.status === "ready" && snapshots.length === 0 && (
          <p className="px-3 py-2 text-sm text-muted-foreground">
            No history yet. Every change your AI makes will be saved here, so you can always go
            back.
          </p>
        )}
        {list.status === "ready" && snapshots.length > 0 && (
          <ul>
            {snapshots.map((snapshot, i) => (
              <SnapshotRow
                key={snapshot.id}
                snapshot={snapshot}
                latest={i === 0}
                now={now}
                disabled={busy !== null}
                onGoBack={setConfirming}
              />
            ))}
          </ul>
        )}
        {list.status === "ready" && snapshots.length >= LIST_LIMIT && (
          <p className="px-3 py-2 text-xs text-muted-foreground">
            Showing the latest {LIST_LIMIT} changes.
          </p>
        )}
      </div>

      <p className="shrink-0 border-t border-border px-3 py-1 text-[11px] text-muted-foreground">
        History is saved on this computer only.
      </p>

      <GoBackDialog
        snapshot={confirming}
        onCancel={() => setConfirming(null)}
        onConfirm={(snapshot) => void goBack(snapshot)}
      />
    </div>
  );
}
