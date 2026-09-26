import { useEffect, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { Check, CircleAlert, Sparkles } from "lucide-react";
import { Button } from "@/components/ui/button";
import { fadeRise, fadeTransition } from "@/lib/motion";
import type { GameError } from "@/lib/studio-types";
import { errorLocation, fixRequestMessage } from "./play-format";

export interface ErrorEntry {
  key: string;
  error: GameError;
  /** How many times this exact error (message, file, line) was reported. */
  count: number;
}

interface GameErrorListProps {
  entries: ErrorEntry[];
  onAskAiToFix: (message: string) => void;
}

function ErrorRow({ entry, onAskAiToFix }: { entry: ErrorEntry; onAskAiToFix: (message: string) => void }) {
  const [sent, setSent] = useState(false);
  const location = errorLocation(entry.error);

  // A brief "Sent to chat" confirmation, since the chat is a different panel
  // and the click would otherwise look like it did nothing here.
  useEffect(() => {
    if (!sent) return;
    const timer = window.setTimeout(() => setSent(false), 2000);
    return () => window.clearTimeout(timer);
  }, [sent]);

  return (
    <motion.li
      data-testid="game-error"
      layout="position"
      {...fadeRise}
      transition={fadeTransition}
      className="flex items-start gap-2 border-b border-border px-3 py-2 last:border-b-0"
    >
      <CircleAlert className="mt-0.5 size-3.5 shrink-0 text-destructive" />
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="text-sm break-words text-foreground/90">
          {entry.error.message}
          {entry.count > 1 && (
            <span className="ml-1.5 text-xs text-muted-foreground tabular-nums">
              ×{entry.count}
            </span>
          )}
        </span>
        {location && (
          <span className="font-mono text-xs break-all text-muted-foreground">{location}</span>
        )}
      </div>
      <Button
        type="button"
        size="xs"
        data-testid="ask-ai-to-fix"
        variant={sent ? "ghost" : "outline"}
        className="shrink-0"
        disabled={sent}
        onClick={() => {
          onAskAiToFix(fixRequestMessage(entry.error));
          setSent(true);
        }}
      >
        {sent ? <Check data-icon="inline-start" /> : <Sparkles data-icon="inline-start" />}
        {sent ? "Sent to chat" : "Ask AI to fix"}
      </Button>
    </motion.li>
  );
}

export function GameErrorList({ entries, onAskAiToFix }: GameErrorListProps) {
  if (entries.length === 0) return null;
  return (
    <ul className="max-h-48 shrink-0 overflow-auto border-t border-border">
      <AnimatePresence initial={false}>
        {entries.map((entry) => (
          <ErrorRow key={entry.key} entry={entry} onAskAiToFix={onAskAiToFix} />
        ))}
      </AnimatePresence>
    </ul>
  );
}
