// Studio's Play panel: Godot install, run/stop, output, and errors.
//
// Everything shown comes from the real Godot process through
// `studio-api.ts`: whether Godot is installed (and which version), the
// game's state, every output line, and every parsed error. Failures from
// the backend are shown with their real text rather than smoothed over.

import { useCallback, useEffect, useRef, useState } from "react";
import { AlertCircle, Loader2, Play, RotateCw, Square } from "lucide-react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import {
  gameRecentErrors,
  gameRun,
  gameStatus,
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
  // Starts at "stopped" only until `gameStatus()` (seeded on mount below)
  // or the first `game-state` event says otherwise — so a panel mounted
  // while a game is already running (e.g. one the AI started through the
  // bridge) shows the truth. A run's errors from before mount come through
  // the `gameRecentErrors` seeding below.
  const [gameState, setGameState] = useState<GameState>("stopped");
  const [errors, setErrors] = useState<ErrorEntry[]>([]);
  // Set by any live `game-state`/`game-error` event; once set, the one-off
  // `gameRecentErrors` seed is stale and must not overwrite live state.
  const liveEventSeen = useRef(false);
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
    // Errors are batched once per animation frame, like GameOutputLog's
    // lines: a game erroring every frame shouldn't re-render per event.
    let pendingErrors: GameError[] = [];
    let frame: number | null = null;
    const cancelFrame = () => {
      if (frame !== null) cancelAnimationFrame(frame);
      frame = null;
    };

    const offState = onGameState(({ state }) => {
      liveEventSeen.current = true;
      setGameState(state);
      // A fresh run (including an automatic restart after an AI change)
      // starts with a clean slate: the last run's errors are about code
      // that may no longer exist. GameOutputLog clears its own output on
      // the same event.
      if (state === "starting") {
        pendingErrors = [];
        cancelFrame();
        setErrors([]);
        setActionError(null);
      }
    });
    const offError = onGameError((error) => {
      liveEventSeen.current = true;
      pendingErrors.push(error);
      if (frame !== null) return;
      frame = requestAnimationFrame(() => {
        frame = null;
        const batch = pendingErrors;
        pendingErrors = [];
        setErrors((prev) => mergeErrors(prev, batch));
      });
    });
    return () => {
      offState();
      offError();
      cancelFrame();
    };
  }, []);

  // Seed the state with the game's real current state (`game-state` only
  // reports changes). Same rule as the error seed below: a live event that
  // arrives first is newer, so the seed never overwrites it. Best-effort:
  // if the query fails, the state stays "stopped" until the next event.
  useEffect(() => {
    let cancelled = false;
    gameStatus()
      .then((state) => {
        if (!cancelled && !liveEventSeen.current) setGameState(state);
      })
      .catch((err) => {
        // Not shown in the UI (the next `game-state` event corrects it),
        // but logged so a broken `game_status` is visible in devtools.
        console.warn("PlayPanel: couldn't read the game's current state:", err);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // Seed the list with errors the backend already captured (e.g. from a run
  // that began before this panel mounted — see the game-state note above).
  // Skipped once any live event has arrived: by then the backend's buffer
  // may hold a previous run's errors, or double-count ones this panel
  // already received. Best-effort: if it fails, the list simply starts
  // empty and live `game-error` events still arrive.
  useEffect(() => {
    let cancelled = false;
    setErrors([]);
    gameRecentErrors(MAX_ERRORS)
      .then((recent) => {
        if (!cancelled && !liveEventSeen.current) setErrors(mergeErrors([], recent));
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
          <GameOutputLog />
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
