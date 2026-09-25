// Studio's Play panel: Godot install, run/stop, output, and errors.
//
// Everything shown comes from the real Godot process through
// `studio-api.ts`: whether Godot is installed (and which version), the
// game's state, every output line, and every parsed error. Failures from
// the backend are shown with their real text rather than smoothed over.

import { useCallback, useEffect, useState } from "react";
import { AlertCircle, Loader2, Play, RotateCw, Square } from "lucide-react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import {
  gameRecentErrors,
  gameRun,
  gameStop,
  godotStatus,
  onGameError,
  onGameState,
} from "@/lib/studio-api";
import type { GameError, GameState, GodotStatus } from "@/lib/studio-types";
import { cn } from "@/lib/utils";
import { GameErrorList, type ErrorEntry } from "./GameErrorList";
import { GameOutputLog } from "./GameOutputLog";
import { GodotInstallCard } from "./GodotInstallCard";
import { errorKey, shortGodotVersion } from "./play-format";

export interface PlayPanelProps {
  projectPath: string;
  /** Sends a message to the chat (wired to ChatPanel's registered send). */
  onAskAiToFix: (message: string) => void;
}

/** Distinct errors kept in the list; repeats of one error only bump its
 * count, so this bounds the list even for a game erroring every frame. */
const MAX_ERRORS = 50;

type GodotLoad =
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "ready"; godot: GodotStatus };

const STATE_LABEL: Record<GameState, string> = {
  stopped: "Not running",
  starting: "Starting…",
  running: "Running",
  crashed: "Stopped unexpectedly",
};

const STATE_DOT: Record<GameState, string> = {
  stopped: "bg-muted-foreground/40",
  starting: "bg-amber-500 animate-pulse",
  running: "bg-emerald-500",
  crashed: "bg-destructive",
};

function errorText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/** Folds new errors into the list: a repeat bumps its entry's count, a new
 * one goes on top, and the oldest distinct errors fall off past the cap. */
function mergeErrors(prev: ErrorEntry[], incoming: GameError[]): ErrorEntry[] {
  let next = prev;
  for (const error of incoming) {
    const key = errorKey(error);
    const existing = next.findIndex((e) => e.key === key);
    if (existing >= 0) {
      next = next.map((e, i) => (i === existing ? { ...e, count: e.count + 1 } : e));
    } else {
      next = [{ key, error, count: 1 }, ...next].slice(0, MAX_ERRORS);
    }
  }
  return next;
}

export function PlayPanel({ projectPath, onAskAiToFix }: PlayPanelProps) {
  const [godot, setGodot] = useState<GodotLoad>({ status: "loading" });
  // There's no "current game state" query in the Phase A contract, only
  // the `game-state` event — Studio mounts once per open project and stays
  // mounted, so it's listening before anything can start a game.
  const [gameState, setGameState] = useState<GameState>("stopped");
  const [runId, setRunId] = useState(0);
  const [errors, setErrors] = useState<ErrorEntry[]>([]);
  const [pending, setPending] = useState<"run" | "stop" | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  const checkGodot = useCallback(async () => {
    setGodot({ status: "loading" });
    try {
      setGodot({ status: "ready", godot: await godotStatus() });
    } catch (err) {
      setGodot({ status: "error", message: errorText(err) });
    }
  }, []);

  useEffect(() => {
    void checkGodot();
  }, [checkGodot]);

  useEffect(() => {
    const offState = onGameState(({ state }) => {
      setGameState(state);
      // A fresh run (including an automatic restart after an AI change)
      // starts with a clean slate: the last run's output and errors are
      // about code that may no longer exist.
      if (state === "starting") {
        setRunId((id) => id + 1);
        setErrors([]);
        setActionError(null);
      }
    });
    const offError = onGameError((error) => setErrors((prev) => mergeErrors(prev, [error])));
    return () => {
      offState();
      offError();
    };
  }, []);

  // Seed the list with errors the backend already captured (e.g. from a run
  // the AI started before this panel opened). Only when nothing live has
  // arrived yet — the backend's buffer would otherwise double-count errors
  // this panel already received as events. Best-effort: if it fails, the
  // list simply starts empty and live `game-error` events still arrive.
  useEffect(() => {
    let cancelled = false;
    setErrors([]);
    gameRecentErrors(MAX_ERRORS)
      .then((recent) => {
        if (!cancelled) setErrors((prev) => (prev.length === 0 ? mergeErrors([], recent) : prev));
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [projectPath]);

  async function run() {
    setPending("run");
    setActionError(null);
    try {
      await gameRun(projectPath);
    } catch (err) {
      setActionError(errorText(err));
    } finally {
      setPending(null);
    }
  }

  async function stop() {
    setPending("stop");
    setActionError(null);
    try {
      await gameStop();
    } catch (err) {
      setActionError(errorText(err));
    } finally {
      setPending(null);
    }
  }

  const live = gameState === "running" || gameState === "starting";
  const version = godot.status === "ready" ? godot.godot.version : null;

  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden rounded-xl border border-border bg-card">
      <div
        data-tauri-drag-region
        className="flex h-9 shrink-0 items-center gap-2 px-3"
      >
        <span className="text-xs font-medium tracking-wide text-muted-foreground">Play</span>
        {godot.status === "ready" && godot.godot.installed && (
          <span className="flex items-center gap-1.5 text-xs text-foreground/80">
            <span className={cn("size-1.5 rounded-full", STATE_DOT[gameState])} />
            {STATE_LABEL[gameState]}
          </span>
        )}
        <div className="flex-1" />
        {godot.status === "ready" && godot.godot.installed && (
          <div className="flex items-center gap-1">
            {live ? (
              <>
                <Button
                  type="button"
                  size="sm"
                  variant="ghost"
                  disabled={pending !== null}
                  onClick={() => void run()}
                >
                  {pending === "run" ? (
                    <Loader2 data-icon="inline-start" className="animate-spin" />
                  ) : (
                    <RotateCw data-icon="inline-start" />
                  )}
                  Restart
                </Button>
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  disabled={pending !== null}
                  onClick={() => void stop()}
                >
                  {pending === "stop" ? (
                    <Loader2 data-icon="inline-start" className="animate-spin" />
                  ) : (
                    <Square data-icon="inline-start" />
                  )}
                  Stop
                </Button>
              </>
            ) : (
              <Button
                type="button"
                size="sm"
                disabled={pending !== null}
                onClick={() => void run()}
              >
                {pending === "run" ? (
                  <Loader2 data-icon="inline-start" className="animate-spin" />
                ) : (
                  <Play data-icon="inline-start" />
                )}
                Play
              </Button>
            )}
          </div>
        )}
      </div>

      {godot.status === "loading" && (
        <div className="flex flex-col gap-2 p-3">
          <Skeleton className="h-4 w-3/5" />
          <Skeleton className="h-4 w-2/5" />
        </div>
      )}

      {godot.status === "error" && (
        <div className="flex flex-col gap-2 p-3">
          <Alert variant="destructive">
            <AlertCircle />
            <AlertDescription className="break-words">
              Couldn't check whether Godot is installed: {godot.message}
            </AlertDescription>
          </Alert>
          <Button
            type="button"
            size="sm"
            variant="outline"
            className="self-start"
            onClick={() => void checkGodot()}
          >
            Check again
          </Button>
        </div>
      )}

      {godot.status === "ready" && !godot.godot.installed && (
        <GodotInstallCard onInstalled={(status) => setGodot({ status: "ready", godot: status })} />
      )}

      {godot.status === "ready" && godot.godot.installed && (
        <>
          {actionError && (
            <Alert variant="destructive" className="m-3 w-auto shrink-0">
              <AlertCircle />
              <AlertDescription className="break-words">{actionError}</AlertDescription>
            </Alert>
          )}
          {gameState === "crashed" && (
            <p className="shrink-0 border-t border-border px-3 py-2 text-sm text-muted-foreground">
              The game stopped unexpectedly. The messages below show what it printed before it
              stopped.
            </p>
          )}
          <GameErrorList entries={errors} onAskAiToFix={onAskAiToFix} />
          <GameOutputLog runId={runId} />
          {version && (
            <div className="shrink-0 border-t border-border px-3 py-1 text-[11px] text-muted-foreground">
              <Tooltip>
                <TooltipTrigger asChild>
                  <span>
                    Godot {shortGodotVersion(version)}
                    {godot.godot.managed ? "" : " (your own install)"}
                  </span>
                </TooltipTrigger>
                <TooltipContent>
                  {version}
                  {godot.godot.path ? ` — ${godot.godot.path}` : ""}
                </TooltipContent>
              </Tooltip>
            </div>
          )}
        </>
      )}
    </div>
  );
}
