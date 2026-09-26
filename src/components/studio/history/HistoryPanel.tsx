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
  /** An AI turn is running in this project's chat. Undo and go back are
   * held off until it ends: the AI may still be writing files, and its own
   * snapshot of the turn lands only when the turn finishes. */
  aiWorking?: boolean;
}

/** Why undo / go back are unavailable while the AI works — said as-is. */
const AI_WORKING_REASON = "Wait for the AI to finish";

/** How many snapshots to list. One extra is requested so the panel knows
 * whether there are more, and says it's showing only the latest ones only
 * when that's actually true. */
const LIST_LIMIT = 100;

type ListState =
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "ready"; snapshots: Snapshot[]; hasMore: boolean };

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
  /** Shown as the Go back button's tooltip when set (why it's disabled). */
  disabledReason: string | null;
  onGoBack: (snapshot: Snapshot) => void;
}

function SnapshotRow({ snapshot, latest, now, disabled, disabledReason, onGoBack }: SnapshotRowProps) {
  // `thread_id` is set only on snapshots an AI chat turn produced, so this
  // tells "the AI did this" apart from a save point made any other way. Said
  // in visible text; the icon is just decoration beside it.
  const byAi = snapshot.thread_id !== null;
  const Icon = byAi ? Sparkles : Save;
  return (
    <li
      data-testid="snapshot-row"
      data-snapshot-id={snapshot.id}
      className="group flex items-start gap-2 border-b border-border px-3 py-2 last:border-b-0 hover:bg-accent/50"
    >
      <Icon aria-hidden className="mt-0.5 size-3.5 shrink-0 text-muted-foreground" />
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <div className="flex min-w-0 items-center gap-1.5">
          <span data-testid="snapshot-title" className="min-w-0 truncate text-sm text-foreground/90">
            {snapshot.title}
          </span>
          {latest && <Badge variant="outline">Latest</Badge>}
        </div>
        <span className="text-xs text-muted-foreground">
          <Tooltip>
            <TooltipTrigger asChild>
              <span tabIndex={0} className="rounded-sm outline-none focus-visible:ring-2 focus-visible:ring-ring/50">
                {relativeTime(snapshot.timestamp, now)}
              </span>
            </TooltipTrigger>
            <TooltipContent>{absoluteTime(snapshot.timestamp)}</TooltipContent>
          </Tooltip>
          {" · "}
          {filesChangedLabel(snapshot.files_changed)}
          {" · "}
          {byAi ? "by your AI" : "save point"}
        </span>
      </div>
      {/* The wrapper carries the tooltip: a disabled button gets no hover. */}
      <span className="shrink-0" title={disabledReason ?? undefined}>
        <Button
          type="button"
          size="xs"
          variant="ghost"
          data-testid="snapshot-go-back"
          disabled={disabled}
          className="opacity-60 group-focus-within:opacity-100 group-hover:opacity-100 focus-visible:opacity-100"
          aria-label={`Go back to this point: ${snapshot.title}`}
          onClick={() => onGoBack(snapshot)}
        >
          <History data-icon="inline-start" />
          Go back
        </Button>
      </span>
    </li>
  );
}

export function HistoryPanel({ projectPath, aiWorking = false }: HistoryPanelProps) {
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
    snapshotList(projectPath, LIST_LIMIT + 1)
      .then((snapshots) => {
        if (cancelled) return;
        setList({
          status: "ready",
          snapshots: snapshots.slice(0, LIST_LIMIT),
          hasMore: snapshots.length > LIST_LIMIT,
        });
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
  const blockedReason = aiWorking ? AI_WORKING_REASON : null;

  // A go-back confirmation left open when a turn starts can't be confirmed.
  useEffect(() => {
    if (aiWorking) setConfirming(null);
  }, [aiWorking]);

  async function undoLast() {
    setBusy("undo");
    setNotice(null);
    try {
      const result = await snapshotUndoLast(projectPath);
      // Deliberately generic: the list this panel last loaded may be stale
      // (or the backend may auto-save unsaved work first), so naming the
      // "latest" row here could name the wrong change. The refreshed list
      // shows exactly what happened.
      setNotice({
        tone: "info",
        text: result === null ? "There's nothing to undo yet." : "Undid the last change.",
      });
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
    <div
      data-testid="history-panel"
      data-list-status={list.status}
      className="flex h-full min-h-0 flex-col overflow-hidden rounded-xl border border-border bg-card"
    >
      <div data-tauri-drag-region className="flex h-9 shrink-0 items-center gap-2 px-3">
        <span className="text-xs font-medium tracking-wide text-muted-foreground">History</span>
        <div className="flex-1" />
        {aiWorking && (
          <span data-testid="history-ai-working" className="truncate text-[11px] text-muted-foreground">
            {AI_WORKING_REASON}
          </span>
        )}
        <span className="shrink-0" title={blockedReason ?? undefined}>
          <Button
            type="button"
            size="xs"
            variant="outline"
            data-testid="undo-last"
            disabled={busy !== null || snapshots.length === 0 || aiWorking}
            onClick={() => void undoLast()}
          >
            {busy === "undo" ? (
              <Loader2 data-icon="inline-start" className="animate-spin" />
            ) : (
              <Undo2 data-icon="inline-start" />
            )}
            Undo last change
          </Button>
        </span>
      </div>

      <AnimatePresence initial={false}>
        {notice && (
          <motion.div
            key={notice.text}
            data-testid="history-notice"
            data-tone={notice.tone}
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
                disabled={busy !== null || aiWorking}
                disabledReason={blockedReason}
                onGoBack={setConfirming}
              />
            ))}
          </ul>
        )}
        {list.status === "ready" && list.hasMore && (
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
