import { useEffect, useRef, type KeyboardEvent } from "react";
import { ArrowUp, Square } from "lucide-react";
import { Button } from "@/components/ui/button";

// Tallest the input grows (in px) before it scrolls instead — enough for a
// short pasted error message without pushing the conversation off screen.
const MAX_HEIGHT = 200;

interface ChatComposerProps {
  value: string;
  onChange: (value: string) => void;
  onSend: () => void;
  onStop: () => void;
  /** A turn is running: sending is off, and the send button becomes Stop. */
  busy: boolean;
  /** Whether Stop has already been pressed for the running turn. */
  stopping: boolean;
  /** The chat isn't ready (still loading, or failed to load) — typing is
   * still allowed so nothing is lost, but sending isn't. */
  disabled: boolean;
}

// Controlled by ChatPanel rather than owning its text, so a message injected
// while the AI is busy ("Ask AI to fix" from the Play panel) can land here
// as a draft instead of being dropped.
export function ChatComposer({ value, onChange, onSend, onStop, busy, stopping, disabled }: ChatComposerProps) {
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);
  const canSend = !busy && !disabled && value.trim().length > 0;

  // Auto-grow: reset to one row, then fit the content (capped).
  useEffect(() => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, MAX_HEIGHT)}px`;
  }, [value]);

  function handleKeyDown(e: KeyboardEvent<HTMLTextAreaElement>) {
    // `isComposing` keeps Enter working for IME users confirming a
    // character instead of sending a half-typed message. WKWebView (macOS)
    // reports the IME's confirming Enter with `isComposing` already false,
    // but still with the legacy keyCode 229, so check that too.
    const imeEnter = e.nativeEvent.isComposing || e.nativeEvent.keyCode === 229;
    if (e.key === "Enter" && !e.shiftKey && !imeEnter) {
      e.preventDefault();
      if (canSend) onSend();
    }
  }

  return (
    <div className="flex items-end gap-2 rounded-xl border border-border bg-background p-2 focus-within:border-ring">
      <textarea
        ref={textareaRef}
        rows={1}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder={busy ? "The AI is working…" : "Describe what you want to change in your game"}
        aria-label="Message the AI"
        className="max-h-[200px] min-h-8 flex-1 resize-none bg-transparent px-1 py-1.5 text-sm outline-none placeholder:text-muted-foreground"
      />
      {busy ? (
        <Button
          type="button"
          size="sm"
          variant="secondary"
          onClick={onStop}
          disabled={stopping}
        >
          <Square className="fill-current" />
          {stopping ? "Stopping…" : "Stop"}
        </Button>
      ) : (
        <Button
          type="button"
          size="icon-sm"
          onClick={onSend}
          disabled={!canSend}
          aria-label="Send"
          title="Send (Enter) · New line (Shift+Enter)"
        >
          <ArrowUp />
        </Button>
      )}
    </div>
  );
}
