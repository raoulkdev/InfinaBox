import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ChevronRight, Home, Network, PanelsTopLeft, Search, X } from "lucide-react";
import {
  AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { PageView } from "@/components/context/PageView";
import type { AskAi } from "@/components/context/aiActions";
import { errorText } from "@/components/context/cardTypes";
import { contextDelete } from "@/lib/studio-api";
import { getLayout, setLayout } from "@/lib/layout-store";
import type { CardSummary } from "@/lib/studio-types";
import { cn } from "@/lib/utils";
import { CanvasView, type FocusRequest } from "./CanvasView";
import { copyDocs } from "./docs";
import { GraphView, type GraphTarget } from "./GraphView";
import { prepareRoot } from "./migrate";
import { pathTo } from "./ops";
import { SearchPanel } from "./SearchPanel";
import { useBoardStore } from "./store";
import { makeBlock, ROOT_BOARD, topZ, unionRect, type Block } from "./types";

// Documents: boards you arrange freely (canvas), the same boards drawn as a
// graph, one search over all of it, and a focused editor for a document.

export interface OpenRequest {
  path: string;
  nonce: number;
}

export interface DocumentsViewProps {
  projectPath: string;
  cards: CardSummary[] | null;
  cardsError: string | null;
  refreshTick: number;
  openRequest: OpenRequest | null;
  onChanged: () => void;
  onAskAi: AskAi;
  /** The Documents / Tasks switch, shown first in the bar. */
  leading?: React.ReactNode;
}

type Mode = "canvas" | "graph";

export function DocumentsView({ projectPath, cards, cardsError, refreshTick, openRequest, onChanged, onAskAi, leading }: DocumentsViewProps) {
  const store = useBoardStore(projectPath);
  const { boards, loaded, commit, seed } = store;
  const [boardId, setBoardId] = useState<string>(() => getLayout<string>("documents.board") ?? ROOT_BOARD);
  const [mode, setMode] = useState<Mode>("canvas");
  const [editorPath, setEditorPath] = useState<string | null>(null);
  const [focusTitle, setFocusTitle] = useState(0);
  const [focus, setFocus] = useState<FocusRequest | null>(null);
  const [searching, setSearching] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<string | null>(null);
  const seeding = useRef(false);
  const list = cards ?? [];

  // The first time, fold in what the project had before.
  useEffect(() => {
    if (!loaded || cards === null || boards[ROOT_BOARD] || seeding.current) return;
    seeding.current = true;
    prepareRoot(projectPath, cards).then(
      (build) => seed(build),
      (err) => setProblem(errorText(err)),
    );
  }, [loaded, cards, boards, projectPath, seed]);
  useEffect(() => {
    seeding.current = false;
  }, [projectPath]);

  const current = boards[boardId] ? boardId : ROOT_BOARD;
  useEffect(() => setLayout("documents.board", current), [current]);

  const crumbs = useMemo(() => pathTo(boards, current), [boards, current]);
  const board = boards[current];

  const navigate = useCallback((id: string, blockId?: string) => {
    setBoardId(id);
    setMode("canvas");
    setEditorPath(null);
    setSearching(false);
    if (blockId) setFocus((f) => ({ blockId, nonce: (f?.nonce ?? 0) + 1 }));
  }, []);

  const openDoc = useCallback((path: string) => {
    setEditorPath(path);
    setFocusTitle((n) => n + 1);
  }, []);

  const locateDoc = useCallback(
    (path: string, openEditor = true) => {
      let found = false;
      for (const b of Object.values(boards)) {
        const blk = b.blocks.find((x) => x.type === "doc" && x.ref === path);
        if (blk) {
          navigate(b.id, blk.id);
          found = true;
          break;
        }
      }
      // A document on no board can only be opened.
      if (openEditor || !found) {
        setMode("canvas");
        openDoc(path);
      }
    },
    [boards, navigate, openDoc],
  );

  // A document opened from elsewhere (the task board).
  const lastNonce = useRef(0);
  useEffect(() => {
    if (!openRequest || openRequest.nonce === lastNonce.current || !loaded) return;
    lastNonce.current = openRequest.nonce;
    locateDoc(openRequest.path);
  }, [openRequest, loaded, locateDoc]);

  const addDocBlock = useCallback(
    (path: string) => {
      if (!board) return;
      const rects = board.blocks.filter((b) => b.type !== "arrow" && !b.col).map((b) => ({ x: b.x, y: b.y, w: b.w, h: b.h }));
      const u = unionRect(rects);
      const blk: Block = makeBlock("doc", { x: u ? u.x : 60, y: u ? u.y + u.h + 40 : 60 }, topZ(board), { ref: path });
      commit((tx) => void tx.board(current).blocks.push(blk));
      setSearching(false);
      setFocus((f) => ({ blockId: blk.id, nonce: (f?.nonce ?? 0) + 1 }));
    },
    [board, commit, current],
  );

  const confirmDelete = async () => {
    const path = deleting;
    setDeleting(null);
    if (!path) return;
    try {
      await contextDelete(projectPath, path);
      commit((tx) => {
        for (const b of Object.values(boards)) {
          if (!b.blocks.some((x) => x.type === "doc" && x.ref === path)) continue;
          const target = tx.board(b.id);
          const gone = new Set(target.blocks.filter((x) => x.type === "doc" && x.ref === path).map((x) => x.id));
          target.blocks = target.blocks.filter((x) => !gone.has(x.id) && !(x.type === "arrow" && (gone.has(x.from) || gone.has(x.to))));
          for (const x of target.blocks) if (x.type === "column") x.children = x.children.filter((c) => !gone.has(c));
        }
      });
      setEditorPath(null);
      onChanged();
    } catch (err) {
      setProblem(errorText(err));
    }
  };

  const duplicateDoc = async (path: string) => {
    try {
      const map = await copyDocs(projectPath, [path], new Set(list.map((c) => c.path)));
      const copy = map.get(path);
      if (!copy) return;
      onChanged();
      addDocBlock(copy);
      setEditorPath(copy);
    } catch (err) {
      setProblem(errorText(err));
    }
  };

  const pickGraph = (t: GraphTarget) => {
    if (t.kind === "board") navigate(t.boardId);
    else if (t.kind === "block") navigate(t.boardId, t.blockId);
    else locateDoc(t.path, false);
  };

  useEffect(() => {
    if (!editorPath) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !document.querySelector('[role="dialog"],[role="menu"]')) setEditorPath(null);
    };
    // Capture phase: a menu or dialog still open at this keypress owns the Escape.
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [editorPath]);

  const placed = useMemo(() => {
    const s = new Set<string>();
    for (const b of Object.values(boards)) for (const x of b.blocks) if (x.type === "doc") s.add(x.ref);
    return s;
  }, [boards]);
  const unplacedCount = list.filter((c) => !placed.has(c.path)).length;

  const segment = (m: Mode, label: string, icon: React.ReactNode) => (
    <button
      type="button"
      data-testid={`mode-${m}`}
      aria-pressed={mode === m}
      onClick={() => setMode(m)}
      className={cn("flex items-center gap-1.5 rounded-md px-2.5 py-1 text-xs", mode === m ? "bg-foreground/10 text-foreground" : "text-muted-foreground hover:text-foreground")}
    >
      {icon}
      {label}
    </button>
  );

  if (cardsError) {
    return (
      <div role="alert" className="m-2 flex-1 rounded-lg bg-destructive/10 p-3 text-sm text-destructive">
        {cardsError}
      </div>
    );
  }

  return (
    <div className="flex h-full min-h-0 w-full min-w-0 flex-col gap-2" data-testid="documents-view">
      <div data-tauri-drag-region className="flex h-11 shrink-0 items-center gap-2 rounded-xl border border-border bg-card px-3">
        {leading}
        <nav aria-label="Boards" data-testid="breadcrumbs" className="flex min-w-0 flex-1 items-center gap-0.5 text-sm">
          {crumbs.map((b, i) => {
            const last = i === crumbs.length - 1;
            return (
              <div key={b.id} className="flex min-w-0 items-center gap-0.5">
                {i > 0 && <ChevronRight className="size-3.5 shrink-0 text-muted-foreground" />}
                {last ? (
                  <input
                    value={b.title}
                    data-drop-board={b.id}
                    data-testid="board-name"
                    aria-label="Board name"
                    placeholder="Untitled board"
                    onChange={(e) => commit((tx) => void (tx.board(b.id).title = e.target.value), `rename:${b.id}`)}
                    style={{ width: `${Math.min(32, Math.max(8, b.title.length + 2))}ch` }}
                    className="min-w-0 rounded bg-transparent px-1.5 py-0.5 font-medium outline-none focus:bg-foreground/5"
                  />
                ) : (
                  <button
                    type="button"
                    data-drop-board={b.id}
                    data-testid="crumb"
                    onClick={() => navigate(b.id)}
                    className="flex min-w-0 items-center gap-1 truncate rounded px-1.5 py-0.5 text-muted-foreground hover:bg-foreground/10 hover:text-foreground"
                  >
                    {i === 0 && <Home className="size-3.5 shrink-0" />}
                    <span className="truncate">{b.title || "Untitled board"}</span>
                  </button>
                )}
              </div>
            );
          })}
        </nav>
        {store.saveError && (
          <span role="alert" className="max-w-60 truncate text-xs text-destructive" title={store.saveError}>
            Couldn't save
          </span>
        )}
        <div className="flex items-center rounded-lg border border-border p-0.5">
          {segment("canvas", "Canvas", <PanelsTopLeft className="size-3.5" />)}
          {segment("graph", "Graph", <Network className="size-3.5" />)}
        </div>
        <button
          type="button"
          data-testid="search-open"
          aria-label="Search"
          title="Search (⌘F)"
          onClick={() => setSearching((s) => !s)}
          className="relative flex size-8 items-center justify-center rounded-md text-muted-foreground hover:bg-foreground/10 hover:text-foreground"
        >
          <Search className="size-4" />
          {unplacedCount > 0 && (
            <span data-testid="unplaced-count" className="absolute -right-0.5 -top-0.5 flex min-w-4 items-center justify-center rounded-full bg-primary px-1 text-[10px] leading-4 text-primary-foreground">
              {unplacedCount}
            </span>
          )}
        </button>
      </div>

      {problem && (
        <div role="alert" className="flex items-start gap-2 rounded-lg bg-destructive/10 p-2 text-sm text-destructive">
          <span className="min-w-0 flex-1 break-words">{problem}</span>
          <button type="button" aria-label="Dismiss" onClick={() => setProblem(null)}>
            <X className="size-4" />
          </button>
        </div>
      )}

      <div className="relative min-h-0 flex-1">
        {!loaded || !board || cards === null ? (
          <div className="size-full rounded-xl border border-border bg-card" />
        ) : mode === "canvas" ? (
          <CanvasView
            key={current}
            projectPath={projectPath}
            boardId={current}
            store={store}
            cards={list}
            tick={refreshTick}
            active={editorPath === null}
            focus={focus}
            onOpenDoc={openDoc}
            onOpenBoard={navigate}
            onCardsChanged={onChanged}
            onSearch={() => setSearching(true)}
            onError={setProblem}
          />
        ) : (
          <GraphView projectPath={projectPath} boards={boards} cards={list} tick={refreshTick} onPick={pickGraph} />
        )}

        {searching && (
          <SearchPanel
            projectPath={projectPath}
            boards={boards}
            cards={list}
            tick={refreshTick}
            onClose={() => setSearching(false)}
            onPickBlock={(b, blk) => navigate(b, blk)}
            onPickBoard={(b) => navigate(b)}
            onPickDoc={(p) => locateDoc(p)}
            onAddDoc={addDocBlock}
          />
        )}

        {editorPath && (
          <div className="absolute inset-0 z-50 flex items-stretch justify-center rounded-xl bg-background/80 p-3 backdrop-blur-sm" data-testid="doc-editor">
            <div className="flex min-h-0 w-full max-w-4xl flex-col gap-2">
              <button
                type="button"
                data-testid="doc-close"
                onClick={() => setEditorPath(null)}
                className="flex w-fit items-center gap-1.5 rounded-md px-2 py-1 text-sm text-muted-foreground hover:bg-foreground/10 hover:text-foreground"
              >
                <ChevronRight className="size-4 rotate-180" /> Back to board
              </button>
              <div className="min-h-0 flex-1">
                <PageView
                  key={editorPath}
                  projectPath={projectPath}
                  path={editorPath}
                  cards={list}
                  refreshTick={refreshTick}
                  onAsk={onAskAi}
                  onTitleCommitted={async () => {}}
                  onDelete={() => setDeleting(editorPath)}
                  onDuplicate={() => void duplicateDoc(editorPath)}
                  onSaved={onChanged}
                  onOpen={(p) => {
                    setEditorPath(p);
                    setFocusTitle((n) => n + 1);
                  }}
                  focusTitle={focusTitle}
                />
              </div>
            </div>
          </div>
        )}
      </div>

      <AlertDialog open={deleting !== null} onOpenChange={(open) => !open && setDeleting(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete this document?</AlertDialogTitle>
            <AlertDialogDescription>It leaves every board it is on. Saved versions of the game still have it.</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction variant="destructive" data-testid="confirm-delete" onClick={() => void confirmDelete()}>
              Delete
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}
