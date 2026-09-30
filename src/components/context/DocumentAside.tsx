import { useEffect, useMemo, useState } from "react";
import { Loader2, Sparkles } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { contextRead } from "@/lib/studio-api";
import type { Card, CardSummary } from "@/lib/studio-types";
import { DOC_ACTIONS, questionAbout, type AskAi } from "./aiActions";

// The right-hand panel of the Documents tab: what's in the open document
// (outline, length, links) and the ways to ask the AI to work on it. It
// reads the saved copy, so it catches up whenever the document is saved.

interface DocumentAsideProps {
  projectPath: string;
  path: string;
  cards: CardSummary[] | null;
  refreshTick: number;
  onAsk: AskAi;
  onOpen: (path: string) => void;
}

interface Heading {
  level: number;
  text: string;
}

function outlineOf(body: string): Heading[] {
  const out: Heading[] = [];
  let fenced = false;
  for (const line of body.split("\n")) {
    if (line.trim().startsWith("```")) fenced = !fenced;
    if (fenced) continue;
    const m = /^(#{1,4})\s+(.+?)\s*#*\s*$/.exec(line);
    if (m) out.push({ level: m[1]!.length, text: m[2]! });
  }
  return out;
}

function wordCount(body: string): number {
  return body.split(/\s+/).filter((w) => /[\p{L}\p{N}]/u.test(w)).length;
}

export function DocumentAside({ projectPath, path, cards, refreshTick, onAsk, onOpen }: DocumentAsideProps) {
  const [card, setCard] = useState<Card | null>(null);
  const [question, setQuestion] = useState("");

  useEffect(() => {
    let cancelled = false;
    contextRead(projectPath, path).then(
      (c) => !cancelled && setCard(c),
      () => !cancelled && setCard(null),
    );
    return () => {
      cancelled = true;
    };
  }, [projectPath, path, refreshTick]);

  const summary = cards?.find((c) => c.path === path) ?? null;
  const outline = useMemo(() => (card ? outlineOf(card.body) : []), [card]);
  const words = card ? wordCount(card.body) : 0;
  const titleOf = (p: string) => cards?.find((c) => c.path === p)?.title ?? p;

  return (
    <aside
      data-testid="document-aside"
      className="flex h-full min-h-0 flex-col gap-4 overflow-y-auto rounded-xl border border-border bg-card p-3"
    >
      <section className="flex flex-col gap-1.5">
        <h3 className="flex items-center gap-1.5 text-xs font-medium tracking-wide text-muted-foreground">
          <Sparkles className="size-3.5" />
          Ask the AI
        </h3>
        <div className="flex flex-col gap-1">
          {DOC_ACTIONS.map((a) => (
            <Button
              key={a.id}
              variant="outline"
              size="sm"
              title={a.hint}
              data-testid={`doc-action-${a.id}`}
              className="h-auto justify-start py-1.5 text-left whitespace-normal"
              onClick={() => onAsk(a.ask(path))}
            >
              {a.label}
            </Button>
          ))}
        </div>
        <form
          className="flex gap-1.5"
          onSubmit={(e) => {
            e.preventDefault();
            if (!question.trim()) return;
            onAsk(questionAbout(path, question));
            setQuestion("");
          }}
        >
          <Input
            value={question}
            onChange={(e) => setQuestion(e.target.value)}
            placeholder="Ask about this document…"
            aria-label="Ask about this document"
            data-testid="doc-question"
            className="h-8 text-xs"
          />
          <Button type="submit" size="sm" disabled={!question.trim()}>
            Ask
          </Button>
        </form>
      </section>

      <section className="flex flex-col gap-1">
        <h3 className="text-xs font-medium tracking-wide text-muted-foreground">At a glance</h3>
        {!card ? (
          <p className="flex items-center gap-1.5 text-xs text-muted-foreground">
            <Loader2 className="size-3 animate-spin" /> Reading…
          </p>
        ) : (
          <dl className="grid grid-cols-2 gap-x-3 gap-y-1 text-xs" data-testid="doc-glance">
            <dt className="text-muted-foreground">Words</dt>
            <dd className="tabular-nums">{words}</dd>
            <dt className="text-muted-foreground">Reading time</dt>
            <dd>{words === 0 ? "—" : `${Math.max(1, Math.round(words / 200))} min`}</dd>
            <dt className="text-muted-foreground">Sections</dt>
            <dd className="tabular-nums">{outline.length}</dd>
            <dt className="text-muted-foreground">Links out</dt>
            <dd className="tabular-nums">{summary?.links.length ?? 0}</dd>
            <dt className="text-muted-foreground">Linked from</dt>
            <dd className="tabular-nums">{summary?.backlinks.length ?? 0}</dd>
          </dl>
        )}
      </section>

      <section className="flex flex-col gap-1">
        <h3 className="text-xs font-medium tracking-wide text-muted-foreground">Outline</h3>
        {outline.length === 0 ? (
          <p className="text-xs text-muted-foreground">No headings yet.</p>
        ) : (
          <ul className="flex flex-col gap-0.5" data-testid="doc-outline">
            {outline.map((h, i) => (
              <li
                key={`${i}-${h.text}`}
                style={{ paddingLeft: `${(h.level - 1) * 10}px` }}
                className="truncate text-xs text-foreground/85"
              >
                {h.text}
              </li>
            ))}
          </ul>
        )}
      </section>

      {summary && (summary.links.length > 0 || summary.backlinks.length > 0 || summary.broken_links.length > 0) && (
        <section className="flex flex-col gap-1">
          <h3 className="text-xs font-medium tracking-wide text-muted-foreground">Connected</h3>
          <ul className="flex flex-col gap-0.5">
            {[...summary.links, ...summary.backlinks.filter((b) => !summary.links.includes(b))].map((p) => (
              <li key={p}>
                <button type="button" onClick={() => onOpen(p)} className="truncate text-left text-xs text-foreground/85 underline-offset-2 hover:underline">
                  {titleOf(p)}
                </button>
              </li>
            ))}
            {summary.broken_links.map((p) => (
              <li key={p} className="truncate text-xs text-destructive">
                {p} (missing)
              </li>
            ))}
          </ul>
        </section>
      )}
    </aside>
  );
}
