import { useEffect, useMemo, useRef, useState } from "react";
import { FileText, Layers, Plus, Search, StickyNote, X } from "lucide-react";
import { Input } from "@/components/ui/input";
import { contextSearch, type CardHit } from "@/lib/studio-api";
import type { CardSummary } from "@/lib/studio-types";
import { NoteIcon } from "@/components/context/note-icons";
import { BLOCK_LABELS, blockText, type Boards } from "./types";

// Search across every board and every document, plus the documents that are
// not on a board yet (the AI writes some).

interface BlockHit {
  boardId: string;
  boardTitle: string;
  blockId: string;
  label: string;
  snippet: string;
}

export interface SearchPanelProps {
  projectPath: string;
  boards: Boards;
  cards: CardSummary[];
  tick: number;
  onClose: () => void;
  onPickBlock: (boardId: string, blockId: string) => void;
  onPickBoard: (boardId: string) => void;
  onPickDoc: (path: string) => void;
  onAddDoc: (path: string) => void;
}

export function SearchPanel({ projectPath, boards, cards, tick, onClose, onPickBlock, onPickBoard, onPickDoc, onAddDoc }: SearchPanelProps) {
  const [query, setQuery] = useState("");
  const [docHits, setDocHits] = useState<CardHit[]>([]);
  const input = useRef<HTMLInputElement | null>(null);
  useEffect(() => input.current?.focus(), []);

  const q = query.trim().toLowerCase();
  useEffect(() => {
    if (!q) return setDocHits([]);
    let cancelled = false;
    const t = setTimeout(() => {
      contextSearch(projectPath, q).then(
        (hits) => !cancelled && setDocHits(hits),
        () => !cancelled && setDocHits([]),
      );
    }, 180);
    return () => {
      cancelled = true;
      clearTimeout(t);
    };
  }, [q, projectPath, tick]);

  const placed = useMemo(() => {
    const s = new Set<string>();
    for (const b of Object.values(boards)) for (const x of b.blocks) if (x.type === "doc") s.add(x.ref);
    return s;
  }, [boards]);
  const unplaced = cards.filter((c) => !placed.has(c.path));

  const boardHits = useMemo(() => (q ? Object.values(boards).filter((b) => b.title.toLowerCase().includes(q)) : []), [boards, q]);
  const blockHits = useMemo(() => {
    if (!q) return [];
    const out: BlockHit[] = [];
    for (const board of Object.values(boards)) {
      for (const b of board.blocks) {
        if (b.type === "doc" || b.type === "board") continue;
        const text = blockText(b).replace(/\s+/g, " ").trim();
        if (text.toLowerCase().includes(q)) out.push({ boardId: board.id, boardTitle: board.title, blockId: b.id, label: BLOCK_LABELS[b.type], snippet: text.slice(0, 120) });
      }
    }
    return out.slice(0, 40);
  }, [boards, q]);

  const row = "flex w-full items-start gap-2 rounded-md px-2 py-1.5 text-left text-sm hover:bg-foreground/10";
  const empty = q && !docHits.length && !blockHits.length && !boardHits.length;

  return (
    <div data-testid="search-panel" className="absolute right-3 top-3 z-40 flex max-h-[80%] w-96 flex-col rounded-xl border border-border bg-card shadow-lg">
      <div className="flex items-center gap-2 border-b border-border px-3 py-2">
        <Search className="size-4 shrink-0 text-muted-foreground" />
        <Input
          ref={input}
          value={query}
          placeholder="Search boards and documents"
          data-testid="search-input"
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => e.key === "Escape" && onClose()}
          className="h-7 border-0 bg-transparent px-0 shadow-none focus-visible:ring-0"
        />
        <button type="button" aria-label="Close search" onClick={onClose} className="flex size-6 items-center justify-center rounded text-muted-foreground hover:bg-foreground/10">
          <X className="size-4" />
        </button>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto p-1.5" data-testid="search-results">
        {!q && (
          <>
            <div className="px-2 py-1 text-xs font-medium text-muted-foreground">Not on a board</div>
            {unplaced.length === 0 && <div className="px-2 py-1 text-xs text-muted-foreground">Everything is on a board.</div>}
            {unplaced.map((c) => (
              <div key={c.path} className="group flex items-center gap-1" data-testid="unplaced-doc">
                <button type="button" className={row} onClick={() => onPickDoc(c.path)}>
                  {c.icon ? <NoteIcon value={c.icon} className="mt-0.5 size-4 text-sm" /> : <FileText className="mt-0.5 size-4 shrink-0 text-muted-foreground" />}
                  <span className="min-w-0 flex-1 truncate">{c.title}</span>
                </button>
                <button type="button" title="Add to this board" aria-label="Add to this board" onClick={() => onAddDoc(c.path)} className="flex size-7 shrink-0 items-center justify-center rounded text-muted-foreground hover:bg-foreground/10 hover:text-foreground">
                  <Plus className="size-4" />
                </button>
              </div>
            ))}
          </>
        )}
        {boardHits.map((b) => (
          <button key={b.id} type="button" className={row} onClick={() => onPickBoard(b.id)} data-testid="hit-board">
            <Layers className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
            <span className="min-w-0 flex-1 truncate">{b.title}</span>
            <span className="text-xs text-muted-foreground">Board</span>
          </button>
        ))}
        {docHits.map((h) => {
          const where = Object.values(boards).find((b) => b.blocks.some((x) => x.type === "doc" && x.ref === h.path));
          return (
            <button key={h.path} type="button" className={row} onClick={() => onPickDoc(h.path)} data-testid="hit-doc">
              <FileText className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
              <span className="min-w-0 flex-1">
                <span className="block truncate">{h.title}</span>
                {h.snippet && <span className="block truncate text-xs text-muted-foreground">{h.snippet}</span>}
              </span>
              <span className="shrink-0 text-xs text-muted-foreground">{where ? where.title : "No board"}</span>
            </button>
          );
        })}
        {blockHits.map((h) => (
          <button key={h.blockId} type="button" className={row} onClick={() => onPickBlock(h.boardId, h.blockId)} data-testid="hit-block">
            <StickyNote className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
            <span className="min-w-0 flex-1">
              <span className="block truncate">{h.snippet}</span>
              <span className="block text-xs text-muted-foreground">{h.label}</span>
            </span>
            <span className="shrink-0 text-xs text-muted-foreground">{h.boardTitle}</span>
          </button>
        ))}
        {empty && <div className="px-2 py-3 text-sm text-muted-foreground">No matches.</div>}
      </div>
    </div>
  );
}
