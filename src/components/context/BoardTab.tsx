import { useCallback, useEffect, useRef, useState } from "react";
import { Bug, ChevronDown, Plus } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { contextBoard, contextSetStatus } from "@/lib/studio-api";
import type { BoardColumn, CardSummary, CardType } from "@/lib/studio-types";
import { cn } from "@/lib/utils";
import { TypeBadge } from "./CardBadges";
import { NewCardDialog } from "./NewCardDialog";
import { TASK_STATUSES, errorText, statusLabel } from "./cardTypes";

interface BoardTabProps {
  projectPath: string;
  refreshTick: number;
  /** Open a card in the Cards tab. */
  onOpen: (path: string) => void;
  /** A card changed; other views should refresh. */
  onChanged: () => void;
  existingPaths: string[];
}

/** Tasks (and optionally playtests) as columns by status. Cards move by
 * drag and drop or the "Move to…" menu; the move shows at once and is put
 * back with a plain message if it can't be saved. */
export function BoardTab({ projectPath, refreshTick, onOpen, onChanged, existingPaths }: BoardTabProps) {
  const [includePlaytests, setIncludePlaytests] = useState(false);
  const [columns, setColumns] = useState<BoardColumn[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [moveError, setMoveError] = useState<string | null>(null);
  const [dragOver, setDragOver] = useState<string | null>(null);
  const [creating, setCreating] = useState<"task" | "bug" | null>(null);
  const inFlight = useRef(0);
  const [reloadTick, setReloadTick] = useState(0);

  useEffect(() => {
    let cancelled = false;
    const types: CardType[] = includePlaytests ? ["task", "playtest"] : ["task"];
    contextBoard(projectPath, types).then(
      (board) => {
        // A move in progress owns the screen; the next refresh catches up.
        if (cancelled || inFlight.current > 0) return;
        setColumns(withStandardColumns(board.columns));
        setError(null);
      },
      (err) => {
        if (!cancelled) setError(errorText(err));
      },
    );
    return () => {
      cancelled = true;
    };
  }, [projectPath, includePlaytests, refreshTick, reloadTick]);

  const move = useCallback(
    async (card: CardSummary, target: string) => {
      if ((card.status ?? "") === target) return;
      const before = columns;
      if (!before) return;
      setMoveError(null);
      setColumns(
        before.map((col) => {
          const without = col.cards.filter((c) => c.path !== card.path);
          return col.status === target ? { ...col, cards: [...without, { ...card, status: target }] } : { ...col, cards: without };
        }),
      );
      inFlight.current += 1;
      try {
        await contextSetStatus(projectPath, card.path, target);
        inFlight.current -= 1;
        onChanged();
        setReloadTick((n) => n + 1);
      } catch (err) {
        inFlight.current -= 1;
        setColumns(before);
        setMoveError(`Couldn't move "${card.title}": ${errorText(err)}`);
      }
    },
    [columns, onChanged, projectPath],
  );

  const total = columns?.reduce((n, c) => n + c.cards.length, 0) ?? 0;

  return (
    <div className="flex h-full min-h-0 w-full min-w-0 flex-col rounded-xl border border-border bg-card" data-testid="board-tab">
      <div data-tauri-drag-region className="flex shrink-0 flex-wrap items-center gap-2 px-3 py-2">
        <span data-tauri-drag-region className="text-xs font-medium tracking-wide text-muted-foreground">
          Board
        </span>
        <label className="ml-2 flex cursor-pointer items-center gap-1.5 text-xs text-muted-foreground">
          <input
            type="checkbox"
            checked={includePlaytests}
            onChange={(e) => setIncludePlaytests(e.target.checked)}
            className="size-3.5 accent-primary"
          />
          Show playtest notes
        </label>
        <div className="ml-auto flex items-center gap-1.5">
          <Button size="xs" variant="outline" onClick={() => setCreating("bug")}>
            <Bug /> New bug
          </Button>
          <Button size="xs" onClick={() => setCreating("task")}>
            <Plus /> New task
          </Button>
        </div>
      </div>

      {moveError && (
        <p role="alert" className="mx-3 mb-2 rounded-lg bg-destructive/10 px-3 py-2 text-xs break-words text-destructive">
          {moveError}
        </p>
      )}

      {error ? (
        <div role="alert" className="m-3 rounded-lg bg-destructive/10 p-3 text-xs text-destructive">
          <p className="break-words">{error}</p>
          <Button size="xs" variant="outline" className="mt-2" onClick={() => setReloadTick((n) => n + 1)}>
            Try again
          </Button>
        </div>
      ) : !columns ? (
        <p className="p-6 text-center text-xs text-muted-foreground">Loading the board…</p>
      ) : (
        <div className="relative flex min-h-0 flex-1 gap-2 overflow-x-auto px-3 pb-3">
          {total === 0 && (
            <p className="pointer-events-none absolute inset-x-0 top-1/2 z-10 -translate-y-1/2 text-center text-xs text-muted-foreground">
              No tasks yet. Add one with "New task" or "New bug".
            </p>
          )}
          {columns.map((col) => (
            <section
              key={col.status || "(none)"}
              aria-label={statusLabel(col.status)}
              onDragOver={(e) => {
                e.preventDefault();
                e.dataTransfer.dropEffect = "move";
                setDragOver(col.status);
              }}
              onDragLeave={() => setDragOver((s) => (s === col.status ? null : s))}
              onDrop={(e) => {
                e.preventDefault();
                setDragOver(null);
                const path = e.dataTransfer.getData("text/plain");
                const card = columns.flatMap((c) => c.cards).find((c) => c.path === path);
                if (card && col.status) void move(card, col.status);
              }}
              className={cn(
                "flex min-h-0 w-64 min-w-56 flex-1 flex-col rounded-lg border bg-background/40 transition-colors",
                dragOver === col.status ? "border-ring bg-muted/40" : "border-border",
              )}
              data-testid={`board-column-${col.status || "none"}`}
            >
              <header className="flex shrink-0 items-center justify-between px-2.5 py-2">
                <h3 className="text-xs font-medium">{statusLabel(col.status)}</h3>
                <span className="text-[11px] text-muted-foreground tabular-nums">{col.cards.length}</span>
              </header>
              <ul className="flex min-h-0 flex-1 flex-col gap-1.5 overflow-y-auto px-1.5 pb-1.5">
                {col.cards.map((card) => (
                  <BoardCard key={card.path} card={card} column={col.status} onOpen={onOpen} onMove={move} />
                ))}
              </ul>
            </section>
          ))}
        </div>
      )}

      <NewCardDialog
        open={creating !== null}
        onOpenChange={(open) => !open && setCreating(null)}
        projectPath={projectPath}
        existingPaths={existingPaths}
        fixedType="task"
        tags={creating === "bug" ? ["bug"] : []}
        heading={creating === "bug" ? "New bug" : "New task"}
        onCreated={() => {
          onChanged();
          setReloadTick((n) => n + 1);
        }}
      />
    </div>
  );
}

/** Always show To do / Doing / Done, in that order, then anything else the
 * cards use. */
function withStandardColumns(columns: BoardColumn[]): BoardColumn[] {
  const byStatus = new Map(columns.map((c) => [c.status, c]));
  const standard = TASK_STATUSES.map((s) => byStatus.get(s) ?? { status: s, cards: [] });
  const rest = columns.filter((c) => !(TASK_STATUSES as readonly string[]).includes(c.status));
  return [...standard, ...rest];
}

function BoardCard({
  card,
  column,
  onOpen,
  onMove,
}: {
  card: CardSummary;
  column: string;
  onOpen: (path: string) => void;
  onMove: (card: CardSummary, target: string) => void;
}) {
  return (
    <li
      draggable
      onDragStart={(e) => {
        e.dataTransfer.setData("text/plain", card.path);
        e.dataTransfer.effectAllowed = "move";
      }}
      className="group cursor-grab rounded-lg border border-border bg-card p-2 active:cursor-grabbing"
      data-testid="board-card"
    >
      <button type="button" onClick={() => onOpen(card.path)} className="block w-full text-left text-sm font-medium hover:underline">
        {card.title}
      </button>
      <div className="mt-1.5 flex items-center gap-1.5">
        {card.card_type !== "task" && <TypeBadge type={card.card_type} />}
        {card.broken_links.length > 0 && (
          <span className="text-[11px] text-destructive">{card.broken_links.length} broken</span>
        )}
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button size="xs" variant="ghost" className="ml-auto" aria-label={`Move ${card.title} to…`}>
              Move to… <ChevronDown />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuLabel>Move to</DropdownMenuLabel>
            {TASK_STATUSES.map((s) => (
              <DropdownMenuItem key={s} disabled={s === column} onSelect={() => onMove(card, s)}>
                {statusLabel(s)}
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </li>
  );
}
