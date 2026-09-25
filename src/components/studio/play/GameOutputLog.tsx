import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { onGameOutput, onGameState } from "@/lib/studio-api";
import type { GameOutputLine } from "@/lib/studio-types";
import { cn } from "@/lib/utils";
import { classifyOutputLine, type OutputLineKind } from "./play-format";

/** Only the newest lines are kept, so a game that prints every frame can't
 * grow memory (or the DOM) without bound. The backend keeps its own bounded
 * ring buffer for the agent; this one is just what's on screen. */
const MAX_LINES = 500;

interface LogLine extends GameOutputLine {
  id: number;
  kind: OutputLineKind;
}

const KIND_CLASS: Record<OutputLineKind, string> = {
  error: "text-destructive font-medium",
  "error-detail": "text-destructive/70",
  warning: "text-amber-600 dark:text-amber-400",
  normal: "text-foreground/80",
};

export function GameOutputLog() {
  const [lines, setLines] = useState<LogLine[]>([]);
  const nextId = useRef(0);
  // Incoming lines are buffered and flushed at most once per animation
  // frame: a burst of hundreds of `game-output` events becomes one render,
  // not hundreds.
  const pending = useRef<LogLine[]>([]);
  const frame = useRef<number | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  // Follow new output only while the user is already at the bottom — if
  // they've scrolled up to read something, don't yank them away from it.
  const stickToBottom = useRef(true);

  useEffect(() => {
    const cancelFrame = () => {
      if (frame.current !== null) cancelAnimationFrame(frame.current);
      frame.current = null;
    };
    // A new run clears the last one's output. Done here, in the same
    // subscription effect and in event order, rather than in a separate
    // effect keyed on a run counter — that would run after the re-render
    // and could wipe the new run's first lines along with the old ones.
    const offState = onGameState(({ state }) => {
      if (state !== "starting") return;
      pending.current = [];
      cancelFrame();
      setLines([]);
      stickToBottom.current = true;
    });
    const offOutput = onGameOutput((line) => {
      pending.current.push({ ...line, id: nextId.current++, kind: classifyOutputLine(line) });
      if (frame.current !== null) return;
      frame.current = requestAnimationFrame(() => {
        frame.current = null;
        const batch = pending.current;
        pending.current = [];
        setLines((prev) => {
          const merged = prev.concat(batch);
          return merged.length > MAX_LINES ? merged.slice(merged.length - MAX_LINES) : merged;
        });
      });
    });
    return () => {
      offState();
      offOutput();
      cancelFrame();
    };
  }, []);

  useLayoutEffect(() => {
    const el = scrollRef.current;
    if (el && stickToBottom.current) el.scrollTop = el.scrollHeight;
  }, [lines]);

  return (
    <div
      ref={scrollRef}
      onScroll={(e) => {
        const el = e.currentTarget;
        stickToBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
      }}
      className="min-h-0 flex-1 overflow-auto border-t border-border bg-background px-3 py-2 font-mono text-xs leading-relaxed"
    >
      {lines.length === 0 ? (
        <p className="font-sans text-muted-foreground">
          Messages from your game will show up here while it runs.
        </p>
      ) : (
        lines.map((line) => (
          <div key={line.id} className={cn("break-words whitespace-pre-wrap", KIND_CLASS[line.kind])}>
            {line.text || " "}
          </div>
        ))
      )}
    </div>
  );
}
