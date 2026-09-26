import { useEffect, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { Check, CircleAlert, Sparkles } from "lucide-react";
import { Button } from "@/components/ui/button";
import type { ChatSendOutcome } from "@/components/studio/chat/ChatPanel";
import { fadeRise, fadeTransition } from "@/lib/motion";
import type { GameError } from "@/lib/studio-types";
import { errorLocation, fixRequestMessage, projectFile } from "./play-format";

export interface ErrorEntry {
  key: string;
  error: GameError;
  /** How many times this exact error (message, file, line) was reported. */
  count: number;
  /** Follow-up messages Godot printed about the same problem (e.g. "Failed
   * to load script" after a parse error), shown under it rather than as
   * rows of their own. See PlayPanel's `mergeErrors`. */
  related: GameError[];
}

interface GameErrorListProps {
  entries: ErrorEntry[];
  onAskAiToFix: (message: string) => ChatSendOutcome | null;
}

/** What the chat did with the request — "sent" to the AI, or only put in
 * the chat box because the chat couldn't send right then. */
const OUTCOME_LABEL: Record<ChatSendOutcome, string> = {
  sent: "Sent to chat",
  drafted: "Added to the chat box",
};

function ErrorRow({
  entry,
  onAskAiToFix,
}: {
  entry: ErrorEntry;
  onAskAiToFix: (message: string) => ChatSendOutcome | null;
}) {
  const [outcome, setOutcome] = useState<ChatSendOutcome | null>(null);
  const location = errorLocation(entry.error);
  // Only errors Godot tied to a file in the user's project get "Ask AI to
  // fix": for the rest (engine or driver messages) there's nothing in the
  // game for the AI to change, and offering it would suggest otherwise.
  const fixable = projectFile(entry.error) !== null;

  // A brief confirmation, since the chat is a different panel and the click
  // would otherwise look like it did nothing here.
  useEffect(() => {
    if (!outcome) return;
    const timer = window.setTimeout(() => setOutcome(null), 2000);
    return () => window.clearTimeout(timer);
  }, [outcome]);

  return (
    <motion.li
      data-testid="game-error"
      data-fixable={fixable ? "true" : "false"}
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
          <span
            data-testid="game-error-location"
            className="font-mono text-xs break-all text-muted-foreground"
          >
            {location}
          </span>
        )}
        {entry.related.map((related, i) => (
          <span
            key={i}
            data-testid="game-error-related"
            className="text-xs break-words text-muted-foreground"
          >
            {related.message}
          </span>
        ))}
        {!fixable && (
          <span data-testid="game-error-engine-note" className="text-xs text-muted-foreground">
            Reported by Godot itself, not tied to a file in your game.
          </span>
        )}
      </div>
      {fixable && (
        <Button
          type="button"
          size="xs"
          data-testid="ask-ai-to-fix"
          data-outcome={outcome ?? undefined}
          variant={outcome ? "ghost" : "outline"}
          className="shrink-0"
          disabled={outcome !== null}
          onClick={() => {
            setOutcome(onAskAiToFix(fixRequestMessage(entry.error, entry.related)));
          }}
        >
          {outcome ? <Check data-icon="inline-start" /> : <Sparkles data-icon="inline-start" />}
          {outcome ? OUTCOME_LABEL[outcome] : "Ask AI to fix"}
        </Button>
      )}
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
