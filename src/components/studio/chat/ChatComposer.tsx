import { useEffect, useRef, type KeyboardEvent, type RefObject } from "react";
import { ArrowUp, Square } from "lucide-react";
import { Button } from "@/components/ui/button";
import type { Role } from "@/lib/studio-types";
import { RolePicker } from "./RolePicker";

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
  /** Replaces the usual placeholder (e.g. "What should be different?" after
   * "Change something" on a plan). */
  hint?: string | null;
  /** Lets the chat focus the input (e.g. from a plan's "Change something"). */
  inputRef?: RefObject<HTMLTextAreaElement | null>;
  /** Who the next message is sent to (default: the Director). */
  role: Role;
  onRoleChange: (role: Role) => void;
}

// Controlled by ChatPanel rather than owning its text, so a message injected
// while the AI is busy ("Ask AI to fix" from the Play panel) can land here
// as a draft instead of being dropped.
export function ChatComposer({
  value,
  onChange,
  onSend,
  onStop,
  busy,
  stopping,
  disabled,
  hint,
  inputRef,
  role,
  onRoleChange,
}: ChatComposerProps) {
  const ownRef = useRef<HTMLTextAreaElement | null>(null);
  const textareaRef = inputRef ?? ownRef;
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
    <div className="flex flex-col gap-1.5 rounded-xl border border-border bg-background p-2 focus-within:border-ring">
      <div className="flex items-end gap-2">
      <textarea
        ref={textareaRef}
        rows={1}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder={busy ? "The AI is working…" : (hint ?? "Describe what you want to change in your game")}
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
      <div className="flex items-center gap-2 px-1">
        <span className="text-xs text-muted-foreground">Ask as</span>
        <RolePicker value={role} onChange={onRoleChange} disabled={busy} />
      </div>
    </div>
  );
}
