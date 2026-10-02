import { useEffect, useRef, useState, type ClipboardEvent, type DragEvent, type KeyboardEvent, type RefObject } from "react";
import { ArrowUp, FileText, Loader2, Paperclip, Square, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import type { ProviderId, Role, SkillInfo, TurnMode } from "@/lib/studio-types";
import { skillCreate, skillsList } from "@/lib/studio-api";
import { ModelPicker, type ModelChoice } from "./ModelPicker";
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
  /** Ask only reads and answers; Build may change the game. */
  mode: TurnMode;
  onModeChange: (mode: TurnMode) => void;
  /** The open game, for its saved skills (`/name`). */
  projectPath?: string | null;
  /** The AI in use, and the model/effort chosen for the next message. */
  provider: ProviderId | null;
  modelChoice: ModelChoice;
  onModelChange: (choice: ModelChoice) => void;
  /** Files (pictures, documents) attached to the next message. */
  attachments: AttachedFile[];
  /** Files still being saved. */
  uploading: boolean;
  onAttach: (files: File[]) => void;
  onRemoveAttachment: (path: string) => void;
  /** Why attaching failed, if it did. */
  attachError: string | null;
}

export interface AttachedFile {
  path: string;
  label: string;
  /** A preview for pictures (a data URL), when there is one. */
  preview: string | null;
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
  mode,
  onModeChange,
  projectPath,
  provider,
  modelChoice,
  onModelChange,
  attachments,
  uploading,
  onAttach,
  onRemoveAttachment,
  attachError,
}: ChatComposerProps) {
  const fileInput = useRef<HTMLInputElement | null>(null);
  const [dragging, setDragging] = useState(false);
  const ownRef = useRef<HTMLTextAreaElement | null>(null);
  const textareaRef = inputRef ?? ownRef;
  const [skills, setSkills] = useState<SkillInfo[]>([]);
  const [naming, setNaming] = useState<string | null>(null);
  const [skillError, setSkillError] = useState<string | null>(null);
  const typingCommand = /^\/[a-z0-9-]*$/.test(value);
  useEffect(() => {
    if (!typingCommand || !projectPath) return;
    let cancelled = false;
    skillsList(projectPath).then(
      (list) => !cancelled && setSkills(list),
      () => !cancelled && setSkills([]),
    );
    return () => {
      cancelled = true;
    };
  }, [typingCommand, projectPath]);
  const matches = typingCommand ? skills.filter((k) => `/${k.slug}`.startsWith(value)) : [];
  const canSend = !busy && !disabled && !uploading && (value.trim().length > 0 || attachments.length > 0);

  // Auto-grow: one row when empty, else fit the content (capped). Measured
  // again whenever the box changes width (it starts hidden or narrow in
  // Studio's stacked layout, which made the empty placeholder count as many
  // wrapped lines).
  useEffect(() => {
    const el = textareaRef.current;
    if (!el) return;
    const fit = () => {
      el.style.height = "auto";
      const wanted = el.value === "" ? 0 : el.scrollHeight;
      el.style.height = wanted === 0 ? "" : `${Math.min(wanted, MAX_HEIGHT)}px`;
    };
    fit();
    const observer = new ResizeObserver(fit);
    observer.observe(el);
    return () => observer.disconnect();
  }, [value, textareaRef]);

  function handleDrop(e: DragEvent) {
    e.preventDefault();
    setDragging(false);
    const files = [...e.dataTransfer.files];
    if (files.length > 0) onAttach(files);
  }

  function handlePaste(e: ClipboardEvent) {
    const files = [...e.clipboardData.files];
    if (files.length > 0) {
      e.preventDefault();
      onAttach(files);
    }
  }

  function handleKeyDown(e: KeyboardEvent<HTMLTextAreaElement>) {
    // `isComposing` keeps Enter working for IME users confirming a
    // character instead of sending a half-typed message. WKWebView (macOS)
    // reports the IME's confirming Enter with `isComposing` already false,
    // but still with the legacy keyCode 229, so check that too.
    const imeEnter = e.nativeEvent.isComposing || e.nativeEvent.keyCode === 229;
    if (e.key === "Tab" && matches.length > 0 && !e.shiftKey) {
      e.preventDefault();
      onChange(`/${matches[0]!.slug} `);
      return;
    }
    if (e.key === "Enter" && !e.shiftKey && !imeEnter) {
      e.preventDefault();
      if (canSend) onSend();
    }
  }

  return (
    <div
      data-testid="chat-composer"
      onDragOver={(e) => {
        if (e.dataTransfer.types.includes("Files")) {
          e.preventDefault();
          setDragging(true);
        }
      }}
      onDragLeave={() => setDragging(false)}
      onDrop={handleDrop}
      className={`flex flex-col gap-1.5 rounded-xl border bg-background p-2 focus-within:border-ring ${
        dragging ? "border-ring bg-accent/40" : "border-border"
      }`}
    >
      {typingCommand && !busy && (
        <div data-testid="skill-menu" className="flex flex-col gap-0.5 rounded-lg border border-border bg-muted/40 p-1 text-sm">
          {matches.map((k) => (
            <button
              key={k.slug}
              type="button"
              data-testid="skill-item"
              onClick={() => {
                onChange(`/${k.slug} `);
                textareaRef.current?.focus();
              }}
              className="flex items-baseline gap-2 rounded px-2 py-1 text-left hover:bg-accent"
            >
              <span className="font-medium">/{k.slug}</span>
              <span className="truncate text-xs text-muted-foreground">{k.description || k.title}</span>
            </button>
          ))}
          {matches.length === 0 && skills.length === 0 && (
            <span className="px-2 py-1 text-xs text-muted-foreground">No saved skills yet.</span>
          )}
          {naming === null ? (
            <button
              type="button"
              data-testid="skill-new"
              disabled={!projectPath}
              onClick={() => {
                setNaming("");
                setSkillError(null);
              }}
              className="rounded px-2 py-1 text-left text-xs text-muted-foreground hover:bg-accent"
            >
              New skill…
            </button>
          ) : (
            <form
              className="flex items-center gap-2 px-1"
              onSubmit={(e) => {
                e.preventDefault();
                if (!projectPath) return;
                skillCreate(projectPath, naming).then(
                  (made) => {
                    setNaming(null);
                    onChange(`/${made.slug} `);
                  },
                  (err) => setSkillError(String(err)),
                );
              }}
            >
              <input
                autoFocus
                data-testid="skill-name"
                value={naming}
                onChange={(e) => setNaming(e.target.value)}
                placeholder="Name, e.g. Add an enemy"
                className="min-w-0 flex-1 bg-transparent px-1 py-1 text-sm outline-none"
              />
              <button type="submit" className="text-xs text-muted-foreground hover:text-foreground">
                Create
              </button>
              <button type="button" onClick={() => setNaming(null)} className="text-xs text-muted-foreground hover:text-foreground">
                Cancel
              </button>
            </form>
          )}
          {skillError && <span className="px-2 text-xs text-destructive">{skillError}</span>}
          {naming !== null && !skillError && (
            <span className="px-2 pb-1 text-xs text-muted-foreground">Edit it in Documents, under skills.</span>
          )}
        </div>
      )}
      {(attachments.length > 0 || uploading || attachError) && (
        <div className="flex flex-wrap items-center gap-1.5 px-1" data-testid="attachments">
          {attachments.map((a) => (
            <span
              key={a.path}
              data-testid="attachment-chip"
              className="inline-flex max-w-56 items-center gap-1.5 rounded-lg border border-border bg-muted/50 py-0.5 pr-1 pl-1 text-xs"
            >
              {a.preview ? (
                <img src={a.preview} alt="" className="size-6 rounded object-cover" />
              ) : (
                <FileText className="size-4 text-muted-foreground" />
              )}
              <span className="truncate">{a.label}</span>
              <button
                type="button"
                aria-label={`Remove ${a.label}`}
                onClick={() => onRemoveAttachment(a.path)}
                className="rounded p-0.5 text-muted-foreground hover:text-foreground"
              >
                <X className="size-3" />
              </button>
            </span>
          ))}
          {uploading && (
            <span className="inline-flex items-center gap-1 text-xs text-muted-foreground">
              <Loader2 className="size-3 animate-spin" />
              Adding…
            </span>
          )}
          {attachError && <span className="text-xs text-destructive">{attachError}</span>}
        </div>
      )}
      <div className="flex items-end gap-2">
        <input
          ref={fileInput}
          type="file"
          multiple
          hidden
          data-testid="attach-input"
          onChange={(e) => {
            const files = [...(e.target.files ?? [])];
            e.target.value = "";
            if (files.length > 0) onAttach(files);
          }}
        />
        <Button
          type="button"
          size="icon-sm"
          variant="ghost"
          aria-label="Attach a picture or file"
          title="Attach a picture or file (or drop / paste one here)"
          data-testid="attach-button"
          disabled={busy || disabled}
          onClick={() => fileInput.current?.click()}
        >
          <Paperclip />
        </Button>
      <textarea
        ref={textareaRef}
        rows={1}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={handleKeyDown}
        onPaste={handlePaste}
        placeholder={busy ? "The AI is working…" : (hint ?? "Talk about your game, or ask for a change")}
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
        <div role="group" aria-label="Mode" className="flex rounded-md border border-border p-0.5 text-xs">
          {(["build", "trial", "ask"] as const).map((m) => (
            <button
              key={m}
              type="button"
              data-testid={`mode-${m}`}
              aria-pressed={mode === m}
              disabled={busy}
              onClick={() => onModeChange(m)}
              title={
                m === "ask"
                  ? "Ask: the AI answers and changes nothing"
                  : m === "trial"
                    ? "In a copy: the AI tries the change in a separate copy; you review it before it touches your game"
                    : "Build: the AI can change the game"
              }
              className={`rounded px-2 py-0.5 ${mode === m ? "bg-accent text-accent-foreground" : "text-muted-foreground"}`}
            >
              {m === "ask" ? "Ask" : m === "trial" ? "In a copy" : "Build"}
            </button>
          ))}
        </div>
        <span className="text-xs text-muted-foreground">Ask as</span>
        <RolePicker value={role} onChange={onRoleChange} disabled={busy} />
        <ModelPicker provider={provider} value={modelChoice} onChange={onModelChange} disabled={busy} />
      </div>
    </div>
  );
}
