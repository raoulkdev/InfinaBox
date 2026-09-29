import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { contextWrite } from "@/lib/studio-api";
import type { CardType } from "@/lib/studio-types";
import { cn } from "@/lib/utils";
import { CARD_TYPES, errorText, newCardContent, newCardPath, typeInfo } from "./cardTypes";

export interface NewCardDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  projectPath: string;
  /** Paths of the cards that exist now, so a new file never collides. */
  existingPaths: string[];
  /** Pre-selected type. */
  initialType?: CardType;
  /** When set the type can't be changed (New task, New bug). */
  fixedType?: CardType;
  /** Tags the new card starts with (a bug is a task tagged `bug`). */
  tags?: string[];
  heading?: string;
  /** Called with the new card's path once it's saved. */
  onCreated: (path: string) => void;
}

/** "New card": pick a type, give it a title, and the card is written with
 * sensible starting front-matter and a short prompt in the body. */
export function NewCardDialog({
  open,
  onOpenChange,
  projectPath,
  existingPaths,
  initialType = "mechanic",
  fixedType,
  tags = [],
  heading = "New card",
  onCreated,
}: NewCardDialogProps) {
  const [type, setType] = useState<CardType>(fixedType ?? initialType);
  const [title, setTitle] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Start fresh every time the dialog opens.
  useEffect(() => {
    if (open) {
      setType(fixedType ?? initialType);
      setTitle("");
      setError(null);
      setBusy(false);
    }
  }, [open, fixedType, initialType]);

  const trimmed = title.trim();

  const create = async () => {
    if (!trimmed || busy) return;
    setBusy(true);
    setError(null);
    const path = newCardPath(type, trimmed, existingPaths);
    const { meta, body } = newCardContent(type, trimmed, tags);
    try {
      await contextWrite(projectPath, path, meta, body);
      onOpenChange(false);
      onCreated(path);
    } catch (err) {
      setError(errorText(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md" data-testid="new-card-dialog">
        <DialogHeader>
          <DialogTitle>{heading}</DialogTitle>
          <DialogDescription>
            {fixedType
              ? "Give it a short, clear name."
              : "Pick what kind of card this is, then give it a name. Your AI reads these cards to understand your game."}
          </DialogDescription>
        </DialogHeader>

        {!fixedType && (
          <div className="grid max-h-64 gap-1 overflow-y-auto pr-1" role="radiogroup" aria-label="Card type">
            {CARD_TYPES.map((t) => (
              <button
                key={t.id}
                type="button"
                role="radio"
                aria-checked={type === t.id}
                onClick={() => setType(t.id)}
                className={cn(
                  "flex items-start gap-2 rounded-lg border px-2.5 py-1.5 text-left transition-colors",
                  type === t.id ? "border-ring bg-muted" : "border-transparent hover:bg-muted/60",
                )}
              >
                <span className="mt-1 size-2.5 shrink-0 rounded-full" style={{ backgroundColor: t.color }} />
                <span className="min-w-0">
                  <span className="block text-sm font-medium">{t.label}</span>
                  <span className="block text-xs text-muted-foreground">{t.blurb}</span>
                </span>
              </button>
            ))}
          </div>
        )}

        <div className="grid gap-1.5">
          <label htmlFor="new-card-title" className="text-xs font-medium text-muted-foreground">
            Title
          </label>
          <Input
            id="new-card-title"
            autoFocus
            value={title}
            placeholder={`Name this ${typeInfo(type).label.toLowerCase()}`}
            onChange={(e) => setTitle(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void create();
            }}
          />
        </div>

        {error && (
          <p role="alert" className="rounded-lg bg-destructive/10 px-2.5 py-1.5 text-xs text-destructive">
            {error}
          </p>
        )}

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button disabled={!trimmed || busy} onClick={() => void create()}>
            {busy ? "Creating…" : "Create"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
