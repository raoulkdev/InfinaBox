import { useMemo, useRef, useState } from "react";
import { ChevronDown, ChevronRight, Plus, Search, Upload } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { CardSummary, CardType } from "@/lib/studio-types";
import { cn } from "@/lib/utils";
import { StatusPill, TypeBadge } from "./CardBadges";
import { CARD_TYPES, statusLabel, typeInfo } from "./cardTypes";

/** The order documents are grouped in, and what each group is called. */
const GROUP_ORDER: CardType[] = ["concept", "style-guide", "mechanic", "character", "level", "story", "task", "playtest", "asset", "other"];
const GROUP_LABEL: Partial<Record<CardType, string>> = {
  concept: "Idea",
  "style-guide": "Style",
  mechanic: "Mechanics",
  character: "Characters",
  level: "Levels",
  story: "Story",
  task: "Tasks",
  playtest: "Playtests",
  asset: "Assets",
  other: "Notes",
};

interface CardListProps {
  cards: CardSummary[] | null;
  loading: boolean;
  error: string | null;
  selectedPath: string | null;
  onSelect: (path: string) => void;
  onNew: () => void;
  /** Text files chosen to be brought in as documents. */
  onImport: (files: File[]) => void;
  onRetry: () => void;
}

/** The left pane of the Cards tab: search, type chips, a status filter and
 * the list of cards. Filtering is local; the list itself comes from
 * `context_list` (see ContextSection). */
export function CardList({ cards, loading, error, selectedPath, onSelect, onNew, onImport, onRetry }: CardListProps) {
  const importInput = useRef<HTMLInputElement | null>(null);
  const [collapsed, setCollapsed] = useState<Set<CardType>>(new Set());
  const [query, setQuery] = useState("");
  const [types, setTypes] = useState<Set<CardType>>(new Set());
  const [status, setStatus] = useState("");

  const counts = useMemo(() => {
    const map = new Map<CardType, number>();
    for (const c of cards ?? []) map.set(c.card_type, (map.get(c.card_type) ?? 0) + 1);
    return map;
  }, [cards]);

  const statuses = useMemo(() => {
    const set = new Set<string>();
    for (const c of cards ?? []) if (c.status) set.add(c.status);
    return [...set].sort();
  }, [cards]);

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    return (cards ?? []).filter((c) => {
      if (types.size > 0 && !types.has(c.card_type)) return false;
      if (status && c.status !== status) return false;
      if (q && !c.title.toLowerCase().includes(q) && !c.path.toLowerCase().includes(q)) return false;
      return true;
    });
  }, [cards, query, types, status]);

  const toggleType = (t: CardType) =>
    setTypes((prev) => {
      const next = new Set(prev);
      if (next.has(t)) next.delete(t);
      else next.add(t);
      return next;
    });

  const filtering = query.trim() !== "" || types.size > 0 || status !== "";

  return (
    <div className="flex h-full min-h-0 flex-col rounded-xl border border-border bg-card" data-testid="card-list">
      <div data-tauri-drag-region className="flex h-9 shrink-0 items-center justify-between gap-2 px-3">
        <span data-tauri-drag-region className="text-xs font-medium tracking-wide text-muted-foreground">
          Documents{cards ? ` · ${cards.length}` : ""}
        </span>
        <div className="flex items-center gap-1">
          <input
            ref={importInput}
            type="file"
            multiple
            hidden
            accept=".md,.markdown,.txt,text/markdown,text/plain"
            data-testid="import-input"
            onChange={(e) => {
              const files = [...(e.target.files ?? [])];
              e.target.value = "";
              if (files.length > 0) onImport(files);
            }}
          />
          <Button
            size="icon-xs"
            variant="ghost"
            title="Bring in a Markdown or text file"
            aria-label="Import a file"
            onClick={() => importInput.current?.click()}
          >
            <Upload />
          </Button>
          <Button size="xs" onClick={onNew} data-testid="new-card-button">
            <Plus /> New document
          </Button>
        </div>
      </div>

      <div className="flex shrink-0 flex-col gap-2 px-3 pb-2">
        <div className="relative">
          <Search className="pointer-events-none absolute top-1/2 left-2 size-3.5 -translate-y-1/2 text-muted-foreground" />
          <Input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search documents"
            aria-label="Search documents"
            className="h-8 pl-7"
          />
        </div>
        <div className="flex flex-wrap gap-1" role="group" aria-label="Filter by type">
          {CARD_TYPES.map((t) => {
            const count = counts.get(t.id) ?? 0;
            const on = types.has(t.id);
            return (
              <button
                key={t.id}
                type="button"
                aria-pressed={on}
                onClick={() => toggleType(t.id)}
                className={cn(
                  "inline-flex h-6 items-center gap-1 rounded-full border px-2 text-[11px] transition-colors",
                  on ? "border-ring bg-muted text-foreground" : "border-border text-muted-foreground hover:bg-muted/60",
                  count === 0 && !on && "opacity-50",
                )}
              >
                <span className="size-1.5 rounded-full" style={{ backgroundColor: t.color }} />
                {t.label}
                <span className="tabular-nums opacity-70">{count}</span>
              </button>
            );
          })}
        </div>
        <select
          value={status}
          onChange={(e) => setStatus(e.target.value)}
          aria-label="Filter by status"
          className="h-8 rounded-lg border border-input bg-transparent px-2 text-xs text-foreground outline-none focus-visible:border-ring dark:bg-input/30"
        >
          <option value="">Any status</option>
          {statuses.map((s) => (
            <option key={s} value={s}>
              {statusLabel(s)}
            </option>
          ))}
        </select>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto border-t border-border">
        {error ? (
          <div role="alert" className="m-3 rounded-lg bg-destructive/10 p-3 text-xs text-destructive">
            <p className="break-words">{error}</p>
            <Button size="xs" variant="outline" className="mt-2" onClick={onRetry}>
              Try again
            </Button>
          </div>
        ) : loading && !cards ? (
          <p className="p-4 text-center text-xs text-muted-foreground">Loading documents…</p>
        ) : cards && cards.length === 0 ? (
          <div className="flex flex-col items-center gap-2 p-6 text-center">
            <p className="text-sm font-medium">No documents yet</p>
            <p className="max-w-56 text-xs text-muted-foreground">
              Documents are notes about your game that your AI reads. Start with the big idea.
            </p>
            <Button size="sm" onClick={onNew}>
              <Plus /> New document
            </Button>
          </div>
        ) : visible.length === 0 ? (
          <p className="p-4 text-center text-xs text-muted-foreground">
            {filtering ? "No documents match these filters." : "Nothing to show."}
          </p>
        ) : (
          <div className="flex flex-col p-1.5">
            {(filtering ? [null] : GROUP_ORDER).map((group) => {
              const rows = group === null ? visible : visible.filter((c) => c.card_type === group);
              if (rows.length === 0) return null;
              const open = group === null || !collapsed.has(group);
              return (
                <section key={group ?? "all"} className="flex flex-col">
                  {group !== null && (
                    <button
                      type="button"
                      aria-expanded={open}
                      onClick={() =>
                        setCollapsed((prev) => {
                          const next = new Set(prev);
                          if (next.has(group)) next.delete(group);
                          else next.add(group);
                          return next;
                        })
                      }
                      className="flex items-center gap-1 rounded-md px-1.5 py-1 text-[11px] font-medium tracking-wide text-muted-foreground uppercase hover:text-foreground"
                    >
                      {open ? <ChevronDown className="size-3" /> : <ChevronRight className="size-3" />}
                      <span className="size-1.5 rounded-full" style={{ backgroundColor: typeInfo(group).color }} />
                      {GROUP_LABEL[group] ?? typeInfo(group).label}
                      <span className="tabular-nums opacity-70">{rows.length}</span>
                    </button>
                  )}
                  {open && (
                    <ul className="flex flex-col">
                      {rows.map((c) => (
                        <li key={c.path}>
                <button
                  type="button"
                  onClick={() => onSelect(c.path)}
                  aria-current={c.path === selectedPath}
                  data-testid="card-list-item"
                  className={cn(
                    "flex w-full flex-col gap-1 rounded-lg px-2.5 py-2 text-left transition-colors",
                    c.path === selectedPath ? "bg-muted" : "hover:bg-muted/50",
                  )}
                >
                  <span className="truncate text-sm font-medium">{c.title}</span>
                  <span className="flex flex-wrap items-center gap-1.5">
                    <TypeBadge type={c.card_type} />
                    <StatusPill status={c.status} />
                    <LinkHint card={c} />
                  </span>
                </button>
                        </li>
                      ))}
                    </ul>
                  )}
                </section>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}

function LinkHint({ card }: { card: CardSummary }) {
  const good = card.links.length;
  const broken = card.broken_links.length;
  if (good + broken === 0) return null;
  return (
    <span className="text-[11px] text-muted-foreground">
      {good + broken} link{good + broken === 1 ? "" : "s"}
      {broken > 0 && <span className="text-destructive"> · {broken} broken</span>}
    </span>
  );
}
