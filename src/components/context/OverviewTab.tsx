import { useState } from "react";
import { Check, ClipboardCopy, Plus, Sparkles } from "lucide-react";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { contextRead } from "@/lib/studio-api";
import type { CardSummary, CardType } from "@/lib/studio-types";
import { fillGapsAsk, type AskAi } from "./aiActions";
import { errorText, typeInfo } from "./cardTypes";

// The Context studio's front page: the shelves a game's documents live on,
// how full each is (counted from the real documents), what's missing, and
// the quick ways to add, fill in or take everything out. Nothing here is
// remembered anywhere else — it's recomputed from the document list.

interface Shelf {
  type: CardType;
  title: string;
  /** Template offered for "New". */
  template: string;
  /** A game should have at least one; shown as missing otherwise. */
  core: boolean;
}

const SHELVES: Shelf[] = [
  { type: "concept", title: "The idea", template: "gdd", core: true },
  { type: "style-guide", title: "Look and sound", template: "style", core: true },
  { type: "mechanic", title: "Mechanics", template: "mechanic", core: true },
  { type: "character", title: "Characters", template: "character", core: false },
  { type: "level", title: "Levels", template: "level", core: false },
  { type: "story", title: "Story", template: "story", core: false },
  { type: "task", title: "Tasks", template: "task", core: false },
  { type: "playtest", title: "Playtests", template: "playtest", core: false },
];

interface OverviewTabProps {
  projectPath: string;
  cards: CardSummary[] | null;
  onOpen: (path: string) => void;
  onNew: (template: string) => void;
  onAskAi: AskAi;
}

export function OverviewTab({ projectPath, cards, onOpen, onNew, onAskAi }: OverviewTabProps) {
  const [copyState, setCopyState] = useState<"idle" | "working" | "done" | string>("idle");
  const list = cards ?? [];
  const missingCore = SHELVES.filter((s) => s.core && !list.some((c) => c.card_type === s.type));

  async function copyAll() {
    setCopyState("working");
    try {
      const parts: string[] = [];
      for (const c of list) {
        const card = await contextRead(projectPath, c.path);
        const body = card.body.trim();
        const hasTitle = /^#\s/.test(body);
        parts.push(hasTitle ? body : `# ${c.title}\n\n${body}`);
      }
      await navigator.clipboard.writeText(parts.join("\n\n---\n\n") + "\n");
      setCopyState("done");
      setTimeout(() => setCopyState("idle"), 2500);
    } catch (err) {
      setCopyState(errorText(err));
    }
  }

  return (
    <ScrollArea className="h-full min-h-0 flex-1 rounded-xl border border-border bg-card" data-testid="context-overview">
      <div className="mx-auto flex w-full max-w-4xl flex-col gap-5 p-5">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="flex flex-col gap-0.5">
            <h2 className="text-base font-semibold tracking-tight">Your game's documents</h2>
            <p className="text-sm text-muted-foreground">
              {cards === null ? "Reading your documents…" : `${list.length} document${list.length === 1 ? "" : "s"}`}
            </p>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <Button variant="outline" size="sm" disabled={list.length === 0 || copyState === "working"} onClick={() => void copyAll()} data-testid="copy-all">
              {copyState === "done" ? <Check /> : <ClipboardCopy />}
              {copyState === "done" ? "Copied" : "Copy all as one document"}
            </Button>
            <Button size="sm" onClick={() => onNew("gdd")} data-testid="overview-new">
              <Plus /> New document
            </Button>
          </div>
        </div>
        {copyState !== "idle" && copyState !== "working" && copyState !== "done" && (
          <p role="alert" className="rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">
            {copyState}
          </p>
        )}

        {cards !== null && missingCore.length > 0 && (
          <div className="flex flex-wrap items-center justify-between gap-3 rounded-xl border border-amber-500/40 bg-amber-500/5 p-3" data-testid="missing-docs">
            <p className="min-w-0 flex-1 text-sm">
              Still to write: <strong>{missingCore.map((s) => s.title.toLowerCase()).join(", ")}</strong>.
            </p>
            <Button size="sm" variant="outline" onClick={() => onAskAi(fillGapsAsk(missingCore.map((s) => s.title.toLowerCase())))}>
              <Sparkles /> Ask the AI to write them
            </Button>
          </div>
        )}

        <div className="grid gap-3 sm:grid-cols-2" data-testid="shelves">
          {SHELVES.map((shelf) => {
            const docs = list.filter((c) => c.card_type === shelf.type);
            const info = typeInfo(shelf.type);
            return (
              <section key={shelf.type} data-testid={`shelf-${shelf.type}`} className="flex flex-col gap-2 rounded-xl border border-border bg-background p-3">
                <div className="flex items-start gap-2">
                  <span className="mt-1.5 size-2.5 shrink-0 rounded-full" style={{ backgroundColor: info.color }} />
                  <div className="min-w-0 flex-1">
                    <h3 className="flex items-center gap-2 text-sm font-medium">
                      {shelf.title}
                      <span className="text-xs font-normal text-muted-foreground tabular-nums">{docs.length}</span>
                      {cards !== null && docs.length === 0 && shelf.core && (
                        <span className="rounded-full bg-amber-500/15 px-1.5 text-[11px] text-amber-300">missing</span>
                      )}
                    </h3>
                  </div>
                  <Button size="icon-xs" variant="ghost" aria-label={`New ${shelf.title.toLowerCase()} document`} onClick={() => onNew(shelf.template)}>
                    <Plus />
                  </Button>
                </div>
                {docs.length > 0 && (
                  <ul className="flex flex-col">
                    {docs.slice(0, 5).map((c) => (
                      <li key={c.path}>
                        <button type="button" onClick={() => onOpen(c.path)} className="w-full truncate rounded px-1.5 py-0.5 text-left text-xs text-foreground/85 hover:bg-muted/60">
                          {c.title}
                        </button>
                      </li>
                    ))}
                    {docs.length > 5 && <li className="px-1.5 text-[11px] text-muted-foreground">and {docs.length - 5} more</li>}
                  </ul>
                )}
              </section>
            );
          })}
        </div>
      </div>
    </ScrollArea>
  );
}
