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
import { errorText, typeInfo } from "./cardTypes";
import { DOC_TEMPLATES, templateById, templatedCard } from "./templates";

export interface NewCardDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  projectPath: string;
  /** Paths of the cards that exist now, so a new file never collides. */
  existingPaths: string[];
  /** Pre-selected template (see `templates.ts`). */
  initialTemplate?: string;
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
  initialTemplate = "note",
  fixedType,
  tags = [],
  heading = "New document",
  onCreated,
}: NewCardDialogProps) {
  const [templateId, setTemplateId] = useState<string>(initialTemplate);
  const [title, setTitle] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Start fresh every time the dialog opens.
  useEffect(() => {
    if (open) {
      setTemplateId(initialTemplate);
      setTitle("");
      setError(null);
      setBusy(false);
    }
  }, [open, fixedType, initialTemplate]);

  const trimmed = title.trim();

  const create = async () => {
    if (!trimmed || busy) return;
    setBusy(true);
    setError(null);
    const template = fixedType
      ? (DOC_TEMPLATES.find((t) => t.type === fixedType && (tags.length === 0) === !t.tags?.length) ?? templateById("task"))
      : templateById(templateId);
    const { path, meta, body } = templatedCard(template, trimmed, existingPaths, tags);
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
              : "Start from a template, then give it a name. Your AI reads these documents to understand your game."}
          </DialogDescription>
        </DialogHeader>

        {!fixedType && (
          <div className="grid max-h-72 grid-cols-1 gap-1.5 overflow-y-auto pr-1 sm:grid-cols-2" role="radiogroup" aria-label="Template">
            {DOC_TEMPLATES.map((t) => (
              <button
                key={t.id}
                type="button"
                role="radio"
                aria-checked={templateId === t.id}
                data-testid={`template-${t.id}`}
                onClick={() => setTemplateId(t.id)}
                className={cn(
                  "flex items-start gap-2 rounded-lg border px-2.5 py-1.5 text-left transition-colors",
                  templateId === t.id ? "border-ring bg-muted" : "border-border hover:bg-muted/60",
                )}
              >
                <span className="mt-1 size-2.5 shrink-0 rounded-full" style={{ backgroundColor: typeInfo(t.type).color }} />
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
            placeholder={`Name this ${(fixedType ? typeInfo(fixedType).label : templateById(templateId).label).toLowerCase()}`}
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
