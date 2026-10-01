import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { AlignCenterHorizontal, AlignCenterVertical, AlignEndHorizontal, AlignEndVertical, AlignStartHorizontal, AlignStartVertical, ArrowDownToLine, ArrowUpToLine, Copy, Maximize, Minus, Plus, Trash2 } from "lucide-react";
import { boardSaveFile } from "@/lib/studio-api";
import { getLayout, setLayout } from "@/lib/layout-store";
import type { CardSummary } from "@/lib/studio-types";
import { cn } from "@/lib/utils";
import { BlockContent, NOTE_BG, type BlockCtx } from "./blocks";
import { collectDocRefs, copyDocs, createDocument, importMarkdown } from "./docs";
import { imageSize } from "./media";
import {
  addBoardBlock, cloneBlocks, deleteBlocks, freeBlock, moveToBoard, putInColumn,
} from "./ops";
import { fileToBase64, IMAGE_EXT, isWebUrl } from "./paths";
import type { BoardStore, Tx } from "./store";
import { Toolbar, type Tool } from "./Toolbar";
import {
  NOTE_COLORS, bottomZ, edgePoint, intersects, makeBlock, topZ, unionRect, withDescendants,
  type Block, type BlockType, type Board, type NoteColor, type Rect, type View,
} from "./types";

// The infinite board: blocks at free positions you pan and zoom around.
// Everything here is positions and pointers; what blocks look like is in
// blocks.tsx, how edits are kept (undo, saving) is in store.ts and ops.ts.

const MIN_ZOOM = 0.1;
const MAX_ZOOM = 3;
const SNAP_PX = 6;
const FIXED_HEIGHT: ReadonlySet<BlockType> = new Set(["doc", "board", "image", "sketch", "swatch"]);
const EDITS_IN_PLACE: ReadonlySet<BlockType> = new Set(["note", "text", "comment", "link"]);
const MARKER = "infinabox-blocks:";

interface Clip {
  id: string;
  blocks: Block[];
  boards: BoardStore["boards"];
  from: string;
  cut: boolean;
}
let clipboard: Clip | null = null;

type Drag =
  | { kind: "pan"; cx: number; cy: number; view: View }
  | { kind: "box"; x0: number; y0: number; base: Set<string> }
  | { kind: "move"; ids: string[]; cx: number; cy: number; rects: Record<string, Rect>; moved: boolean; clicked: string }
  | { kind: "resize"; id: string; handle: "se" | "e"; cx: number; cy: number; rect: Rect; ratio: number }
  | { kind: "connect"; from: string; moved: boolean };

type DropTarget = { kind: "board"; boardId: string; blockId: string } | { kind: "crumb"; boardId: string } | { kind: "column"; id: string; index: number };
interface Guide {
  axis: "x" | "y";
  pos: number;
  from: number;
  to: number;
}

export interface FocusRequest {
  blockId: string;
  nonce: number;
}

export interface CanvasViewProps {
  projectPath: string;
  boardId: string;
  store: BoardStore;
  cards: CardSummary[];
  tick: number;
  /** False while a document is open over the board: shortcuts are off. */
  active: boolean;
  focus: FocusRequest | null;
  onOpenDoc: (path: string) => void;
  onOpenBoard: (id: string) => void;
  /** Cards were made or copied: refresh the list. */
  onCardsChanged: () => void;
  onSearch: () => void;
  onError: (message: string) => void;
}

const sameRect = (a: Rect | undefined, b: Rect) => !!a && Math.abs(a.x - b.x) < 0.25 && Math.abs(a.y - b.y) < 0.25 && Math.abs(a.w - b.w) < 0.25 && Math.abs(a.h - b.h) < 0.25;

const INTERACTIVE = 'input,textarea,select,button,[contenteditable="true"],[contenteditable="plaintext-only"],[data-no-drag]';

function errorMessage(err: unknown): string {
  return typeof err === "string" ? err : err instanceof Error ? err.message : "Something went wrong.";
}

export function CanvasView(props: CanvasViewProps) {
  const { projectPath, boardId, store, cards, tick, active, focus, onOpenDoc, onOpenBoard, onCardsChanged, onSearch, onError } = props;
  const board = store.boards[boardId]!;
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const fileInput = useRef<HTMLInputElement | null>(null);
  const fileKind = useRef<"image" | "file">("file");

  const saved = useMemo(() => getLayout<View>(`documents.view.${boardId}`), [boardId]);
  const [view, setView] = useState<View>(saved ?? { x: 120, y: 60, zoom: 1 });
  const [rects, setRects] = useState<Record<string, Rect>>({});
  const [sel, setSel] = useState<Set<string>>(new Set());
  const [tool, setTool] = useState<Tool>("select");
  const [editingId, setEditingId] = useState<string | null>(null);
  const [space, setSpace] = useState(false);
  const [delta, setDelta] = useState<{ dx: number; dy: number; ids: Set<string> } | null>(null);
  const [guides, setGuides] = useState<Guide[]>([]);
  const [marquee, setMarquee] = useState<Rect | null>(null);
  const [dropTarget, setDropTarget] = useState<DropTarget | null>(null);
  const [resizing, setResizing] = useState<{ id: string; w: number; h: number } | null>(null);
  const [connect, setConnect] = useState<{ from: string; x: number; y: number } | null>(null);
  const [pendingFrom, setPendingFrom] = useState<string | null>(null);
  const [quickAdd, setQuickAdd] = useState<{ sx: number; sy: number; x: number; y: number } | null>(null);
  const [, setMeasureTick] = useState(0);
  const [fileDrag, setFileDrag] = useState(false);

  const drag = useRef<Drag | null>(null);
  const needsFit = useRef(!saved);
  const cascade = useRef(0);

  const byId = useMemo(() => new Map(board.blocks.map((b) => [b.id, b])), [board.blocks]);
  const cardMap = useMemo(() => new Map(cards.map((c) => [c.path, c])), [cards]);
  const taken = useMemo(() => new Set(cards.map((c) => c.path)), [cards]);

  // Latest values for the window-level pointer and key handlers.
  const L = useRef({ board, view, rects, sel, tool, space, active, store, byId, taken, cardMap });
  L.current = { board, view, rects, sel, tool, space, active, store, byId, taken, cardMap };

  // ---- measuring ----
  // Block sizes follow their content, so positions for arrows, snapping and
  // selection come from the screen, in board coordinates.
  useLayoutEffect(() => {
    const wrap = wrapRef.current;
    if (!wrap) return;
    const wr = wrap.getBoundingClientRect();
    const next: Record<string, Rect> = {};
    wrap.querySelectorAll<HTMLElement>("[data-block-id]").forEach((el) => {
      const r = el.getBoundingClientRect();
      next[el.dataset.blockId!] = { x: (r.left - wr.left - view.x) / view.zoom, y: (r.top - wr.top - view.y) / view.zoom, w: r.width / view.zoom, h: r.height / view.zoom };
    });
    const prev = L.current.rects;
    const ids = Object.keys(next);
    if (ids.length !== Object.keys(prev).length || ids.some((id) => !sameRect(prev[id], next[id]!))) setRects(next);
  });
  useEffect(() => {
    const wrap = wrapRef.current;
    if (!wrap || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(() => setMeasureTick((n) => n + 1));
    wrap.querySelectorAll("[data-block-id]").forEach((el) => ro.observe(el));
    return () => ro.disconnect();
  }, [board.blocks]);

  // ---- view ----
  useEffect(() => {
    setLayout(`documents.view.${boardId}`, view);
  }, [boardId, view]);

  const toWorld = useCallback((cx: number, cy: number) => {
    const r = wrapRef.current!.getBoundingClientRect();
    const v = L.current.view;
    return { x: (cx - r.left - v.x) / v.zoom, y: (cy - r.top - v.y) / v.zoom };
  }, []);

  const zoomAt = useCallback((factor: number, sx: number, sy: number) => {
    setView((v) => {
      const zoom = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, v.zoom * factor));
      const k = zoom / v.zoom;
      return { zoom, x: sx - (sx - v.x) * k, y: sy - (sy - v.y) * k };
    });
  }, []);

  const fit = useCallback(() => {
    const wrap = wrapRef.current;
    const all = Object.entries(L.current.rects).filter(([id]) => L.current.byId.get(id)?.type !== "arrow").map(([, r]) => r);
    const u = unionRect(all);
    if (!wrap || !u) return setView({ x: 120, y: 60, zoom: 1 });
    const { width, height } = wrap.getBoundingClientRect();
    const zoom = Math.min(1, Math.max(MIN_ZOOM, Math.min((width - 200) / u.w, (height - 120) / u.h)));
    setView({ zoom, x: (width - u.w * zoom) / 2 - u.x * zoom + 30, y: (height - u.h * zoom) / 2 - u.y * zoom });
  }, []);

  useEffect(() => {
    if (needsFit.current && Object.keys(rects).length > 0) {
      needsFit.current = false;
      fit();
    }
  }, [rects, fit]);

  useEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      if ((e.target as HTMLElement).closest("[data-canvas-ui]")) return;
      e.preventDefault();
      const r = el.getBoundingClientRect();
      if (e.ctrlKey || e.metaKey) {
        const d = Math.max(-60, Math.min(60, e.deltaY));
        zoomAt(Math.exp(-d * 0.01), e.clientX - r.left, e.clientY - r.top);
      } else {
        setView((v) => ({ ...v, x: v.x - (e.shiftKey && !e.deltaX ? e.deltaY : e.deltaX), y: v.y - (e.shiftKey && !e.deltaX ? 0 : e.deltaY) }));
      }
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [zoomAt]);

  // Jump to a block (search, the graph).
  useEffect(() => {
    if (!focus) return;
    const b = L.current.byId.get(focus.blockId);
    if (!b) return;
    needsFit.current = false;
    setSel(new Set([b.id]));
    const wrap = wrapRef.current;
    if (!wrap) return;
    const { width, height } = wrap.getBoundingClientRect();
    const r = L.current.rects[b.id] ?? { x: b.x, y: b.y, w: b.w, h: b.h };
    const zoom = Math.min(1, Math.max(0.5, L.current.view.zoom));
    setView({ zoom, x: width / 2 - (r.x + r.w / 2) * zoom, y: height / 2 - (r.y + r.h / 2) * zoom });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [focus?.nonce]);

  // ---- edits ----
  const commit = store.commit;
  const update = useCallback(
    (id: string, patch: Record<string, unknown>) => {
      commit((tx) => {
        const b = tx.board(boardId).blocks.find((x) => x.id === id);
        if (b) Object.assign(b, patch);
      }, `edit:${id}`);
    },
    [commit, boardId],
  );

  const select = useCallback((ids: Iterable<string>) => setSel(new Set(ids)), []);

  const placeAt = useCallback(
    (at?: { x: number; y: number }) => {
      if (at) return at;
      const wrap = wrapRef.current!.getBoundingClientRect();
      const v = L.current.view;
      cascade.current = (cascade.current + 1) % 8;
      return { x: (wrap.width / 2 - v.x) / v.zoom + cascade.current * 24 - 100, y: (wrap.height / 2 - v.y) / v.zoom + cascade.current * 24 - 60 };
    },
    [],
  );

  const addBlocks = useCallback(
    (blocks: Block[], extra?: (tx: Tx) => void) => {
      if (!blocks.length && !extra) return;
      commit((tx) => {
        const b = tx.board(boardId);
        let z = topZ(b);
        for (const blk of blocks) {
          blk.z = z++;
          b.blocks.push(blk);
        }
        extra?.(tx);
      });
      select(blocks.map((b) => b.id));
    },
    [commit, boardId, select],
  );

  const addDocument = useCallback(
    async (template: string | null, at?: { x: number; y: number }) => {
      try {
        const path = await createDocument(projectPath, L.current.taken, template);
        const p = placeAt(at);
        addBlocks([makeBlock("doc", p, 0, { ref: path })]);
        onCardsChanged();
        onOpenDoc(path);
      } catch (err) {
        onError(errorMessage(err));
      }
    },
    [projectPath, placeAt, addBlocks, onCardsChanged, onOpenDoc, onError],
  );

  const addFiles = useCallback(
    async (files: File[], at?: { x: number; y: number }) => {
      const start = placeAt(at);
      const blocks: Block[] = [];
      const taken = new Set(L.current.taken);
      let n = 0;
      let madeCards = false;
      for (const file of files) {
        const pos = { x: start.x + n * 24, y: start.y + n * 24 };
        n += 1;
        try {
          if (/\.(md|markdown)$/i.test(file.name) && file.size < 2_000_000) {
            const path = await importMarkdown(projectPath, file.name, await file.text(), taken);
            taken.add(path);
            madeCards = true;
            blocks.push(makeBlock("doc", pos, 0, { ref: path }));
            continue;
          }
          const src = await boardSaveFile(projectPath, file.name, await fileToBase64(file));
          if (file.type.startsWith("image/") || IMAGE_EXT.test(file.name)) {
            const url = URL.createObjectURL(file);
            const size = await imageSize(url);
            URL.revokeObjectURL(url);
            const w = Math.min(360, Math.max(120, size.w));
            blocks.push(makeBlock("image", pos, 0, { src, ratio: size.w / size.h, w, h: Math.round(w / (size.w / size.h)) }));
          } else {
            blocks.push(makeBlock("file", pos, 0, { src, name: file.name, size: file.size }));
          }
        } catch (err) {
          onError(errorMessage(err));
        }
      }
      if (madeCards) onCardsChanged();
      addBlocks(blocks);
    },
    [projectPath, placeAt, addBlocks, onCardsChanged, onError],
  );

  const add = useCallback(
    (type: BlockType, at?: { x: number; y: number }) => {
      if (type === "image" || type === "file") {
        fileKind.current = type;
        if (fileInput.current) {
          fileInput.current.accept = type === "image" ? "image/*" : "";
          fileInput.current.click();
        }
        return;
      }
      if (type === "doc") return void addDocument(null, at);
      const p = placeAt(at);
      if (type === "board") {
        let created: Block | null = null;
        commit((tx) => {
          created = addBoardBlock(tx, boardId, p);
        });
        if (created) select([(created as Block).id]);
        return;
      }
      const blk = makeBlock(type, p, 0);
      addBlocks([blk]);
      if (EDITS_IN_PLACE.has(type)) setEditingId(blk.id);
      if (type === "sketch") setEditingId(blk.id);
    },
    [addDocument, placeAt, commit, boardId, select, addBlocks],
  );

  const selectedBlocks = useCallback((): Block[] => {
    const { board: bd, sel: s } = L.current;
    const all = withDescendants(bd, s);
    return bd.blocks.filter((b) => all.has(b.id) || (b.type === "arrow" && all.has(b.from) && all.has(b.to)));
  }, []);

  const remove = useCallback(() => {
    const ids = [...L.current.sel];
    if (!ids.length) return;
    commit((tx) => deleteBlocks(tx, boardId, ids));
    setSel(new Set());
    setEditingId(null);
  }, [commit, boardId]);

  const duplicate = useCallback(
    async (blocks: Block[], from: Board, offset: { dx: number; dy: number }, into = boardId) => {
      if (!blocks.length) return;
      try {
        const refs = collectDocRefs(blocks, L.current.store.boards);
        const map = refs.length ? await copyDocs(projectPath, refs, new Set(L.current.taken)) : new Map<string, string>();
        if (map.size) onCardsChanged();
        let made: Block[] = [];
        commit((tx) => {
          made = cloneBlocks(tx, L.current.store.boards, blocks, into, (r) => map.get(r) ?? r, offset);
        });
        void from;
        if (into === boardId) select(made.filter((b) => b.type !== "arrow" && !b.col).map((b) => b.id));
      } catch (err) {
        onError(errorMessage(err));
      }
    },
    [boardId, projectPath, commit, onCardsChanged, select, onError],
  );

  const copy = useCallback(
    (cut: boolean) => {
      const blocks = selectedBlocks();
      if (!blocks.length) return null;
      clipboard = { id: Math.random().toString(36).slice(2), blocks: structuredClone(blocks), boards: structuredClone(L.current.store.boards), from: boardId, cut };
      return clipboard.id;
    },
    [selectedBlocks, boardId],
  );

  const paste = useCallback(() => {
    const c = clipboard;
    if (!c) return;
    if (c.cut) {
      const ids = c.blocks.filter((b) => !b.col && b.type !== "arrow").map((b) => b.id);
      if (c.from !== boardId && L.current.store.boards[c.from]) {
        commit((tx) => moveToBoard(tx, c.from, boardId, ids, { x: 60, y: 60 }));
        select(ids);
      }
      clipboard = null;
      return;
    }
    void duplicate(c.blocks, L.current.board, c.from === boardId ? { dx: 28, dy: 28 } : { dx: 0, dy: 0 });
  }, [boardId, commit, select, duplicate]);

  const order = useCallback(
    (front: boolean) => {
      const ids = new Set(L.current.sel);
      commit((tx) => {
        const b = tx.board(boardId);
        let z = front ? topZ(b) : bottomZ(b) - ids.size;
        for (const blk of b.blocks.filter((x) => ids.has(x.id) && x.type !== "arrow").sort((p, q) => p.z - q.z)) blk.z = z++;
      });
    },
    [commit, boardId],
  );

  const align = useCallback(
    (mode: "left" | "hcenter" | "right" | "top" | "vcenter" | "bottom" | "hdist" | "vdist") => {
      const { board: bd, sel: s, rects: rs } = L.current;
      const items = bd.blocks.filter((b) => s.has(b.id) && !b.col && b.type !== "arrow" && rs[b.id]);
      const u = unionRect(items.map((b) => rs[b.id]!));
      if (items.length < 2 || !u) return;
      commit((tx) => {
        const target = tx.board(boardId);
        const get = (id: string) => target.blocks.find((x) => x.id === id)!;
        if (mode === "hdist" || mode === "vdist") {
          const h = mode === "hdist";
          const sorted = [...items].sort((a, b) => (h ? rs[a.id]!.x - rs[b.id]!.x : rs[a.id]!.y - rs[b.id]!.y));
          const total = sorted.reduce((n, b) => n + (h ? rs[b.id]!.w : rs[b.id]!.h), 0);
          const gap = ((h ? u.w : u.h) - total) / (sorted.length - 1);
          let cur = h ? u.x : u.y;
          for (const b of sorted) {
            const r = rs[b.id]!;
            const t = get(b.id);
            if (h) t.x = Math.round(t.x + cur - r.x);
            else t.y = Math.round(t.y + cur - r.y);
            cur += (h ? r.w : r.h) + gap;
          }
          return;
        }
        for (const b of items) {
          const r = rs[b.id]!;
          const t = get(b.id);
          if (mode === "left") t.x = Math.round(t.x + u.x - r.x);
          if (mode === "right") t.x = Math.round(t.x + u.x + u.w - (r.x + r.w));
          if (mode === "hcenter") t.x = Math.round(t.x + u.x + u.w / 2 - (r.x + r.w / 2));
          if (mode === "top") t.y = Math.round(t.y + u.y - r.y);
          if (mode === "bottom") t.y = Math.round(t.y + u.y + u.h - (r.y + r.h));
          if (mode === "vcenter") t.y = Math.round(t.y + u.y + u.h / 2 - (r.y + r.h / 2));
        }
      });
    },
    [commit, boardId],
  );

  const connectBlocks = useCallback(
    (from: string, to: string) => {
      const { byId: ids } = L.current;
      const a = ids.get(from);
      const b = ids.get(to);
      if (!a || !b || a.type === "arrow" || b.type === "arrow" || from === to) return;
      const blk = makeBlock("arrow", { x: 0, y: 0 }, 0, { from, to });
      blk.w = 0;
      blk.h = 0;
      addBlocks([blk]);
    },
    [addBlocks],
  );

  // ---- pointer handling ----
  const endDrag = useRef<(() => void) | null>(null);
  const stopDrag = () => {
    endDrag.current?.();
    endDrag.current = null;
  };
  useEffect(() => stopDrag, []);

  const dropAt = (cx: number, cy: number, moving: string[]): DropTarget | null => {
    const el = document.elementFromPoint(cx, cy) as HTMLElement | null;
    if (!el) return null;
    const crumb = el.closest<HTMLElement>("[data-drop-board]");
    if (crumb?.dataset.dropBoard && crumb.dataset.dropBoard !== boardId) return { kind: "crumb", boardId: crumb.dataset.dropBoard };
    let cur = el.closest<HTMLElement>("[data-block-id]");
    while (cur) {
      const id = cur.dataset.blockId!;
      const blk = L.current.byId.get(id);
      if (blk && !moving.includes(id)) {
        if (blk.type === "board" && blk.ref) return { kind: "board", boardId: blk.ref, blockId: id };
        if (blk.type === "column") {
          const dragged = moving.map((m) => L.current.byId.get(m)).filter((b): b is Block => !!b && b.type !== "column" && b.type !== "arrow");
          if (!dragged.length) return null;
          const w = toWorld(cx, cy);
          const kids = blk.children.filter((c) => !moving.includes(c));
          const index = kids.filter((c) => {
            const r = L.current.rects[c];
            return r && r.y + r.h / 2 < w.y;
          }).length;
          return { kind: "column", id, index };
        }
      }
      cur = cur.parentElement?.closest<HTMLElement>("[data-block-id]") ?? null;
    }
    return null;
  };

  const startMove = (e: React.PointerEvent, id: string) => {
    const { sel: s, byId: ids, rects: rs } = L.current;
    const group = s.has(id) ? s : new Set([id]);
    const top = [...group].filter((i) => {
      const b = ids.get(i);
      return !!b && b.type !== "arrow" && !(b.col && group.has(b.col)) && rs[i];
    });
    const startRects: Record<string, Rect> = {};
    for (const i of top) startRects[i] = rs[i]!;
    drag.current = { kind: "move", ids: top, cx: e.clientX, cy: e.clientY, rects: startRects, moved: false, clicked: id };
    attach();
  };

  const attach = () => {
    const move = (ev: PointerEvent) => onMove(ev);
    const up = (ev: PointerEvent) => {
      stopDrag();
      onUp(ev);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", up);
    endDrag.current = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", up);
    };
  };

  const onMove = (e: PointerEvent) => {
    const d = drag.current;
    if (!d) return;
    const { view: v } = L.current;
    if (d.kind === "pan") {
      setView({ ...d.view, x: d.view.x + e.clientX - d.cx, y: d.view.y + e.clientY - d.cy });
    } else if (d.kind === "box") {
      const w = toWorld(e.clientX, e.clientY);
      const r: Rect = { x: Math.min(d.x0, w.x), y: Math.min(d.y0, w.y), w: Math.abs(w.x - d.x0), h: Math.abs(w.y - d.y0) };
      setMarquee(r);
      const hit = new Set(d.base);
      for (const b of L.current.board.blocks) {
        const rr = L.current.rects[b.id];
        if (b.type !== "arrow" && !b.col && rr && intersects(r, rr)) hit.add(b.id);
      }
      setSel(hit);
    } else if (d.kind === "move") {
      if (!d.moved && Math.hypot(e.clientX - d.cx, e.clientY - d.cy) < 4) return;
      d.moved = true;
      setEditingId(null);
      let dx = (e.clientX - d.cx) / v.zoom;
      let dy = (e.clientY - d.cy) / v.zoom;
      const found: Guide[] = [];
      const u = unionRect(d.ids.map((i) => d.rects[i]!));
      const target = dropAt(e.clientX, e.clientY, d.ids);
      if (u && !e.altKey && !target) {
        const thr = SNAP_PX / v.zoom;
        const others = L.current.board.blocks.filter((b) => b.type !== "arrow" && !b.col && !d.ids.includes(b.id) && L.current.rects[b.id]).map((b) => L.current.rects[b.id]!);
        const best = (mine: number[], theirs: number[][]) => {
          let pick: { adj: number; at: number; i: number } | null = null;
          for (const m of mine)
            for (let i = 0; i < theirs.length; i += 1)
              for (const t of theirs[i]!) {
                const diff = t - m;
                if (Math.abs(diff) <= thr && (!pick || Math.abs(diff) < Math.abs(pick.adj))) pick = { adj: diff, at: t, i };
              }
          return pick;
        };
        const mx = [u.x + dx, u.x + dx + u.w / 2, u.x + dx + u.w];
        const my = [u.y + dy, u.y + dy + u.h / 2, u.y + dy + u.h];
        const px = best(mx, others.map((o) => [o.x, o.x + o.w / 2, o.x + o.w]));
        const py = best(my, others.map((o) => [o.y, o.y + o.h / 2, o.y + o.h]));
        if (px) dx += px.adj;
        if (py) dy += py.adj;
        const moved: Rect = { x: u.x + dx, y: u.y + dy, w: u.w, h: u.h };
        if (px) {
          const o = others[px.i]!;
          found.push({ axis: "x", pos: px.at, from: Math.min(o.y, moved.y), to: Math.max(o.y + o.h, moved.y + moved.h) });
        }
        if (py) {
          const o = others[py.i]!;
          found.push({ axis: "y", pos: py.at, from: Math.min(o.x, moved.x), to: Math.max(o.x + o.w, moved.x + moved.w) });
        }
      }
      setGuides(found);
      setDelta({ dx, dy, ids: new Set(d.ids) });
      setDropTarget(target);
    } else if (d.kind === "resize") {
      const w0 = d.rect.w;
      const dxw = (e.clientX - d.cx) / v.zoom;
      const dyw = (e.clientY - d.cy) / v.zoom;
      const nw = Math.max(80, Math.round(w0 + dxw));
      const blk = L.current.byId.get(d.id);
      let nh = d.rect.h;
      if (blk && FIXED_HEIGHT.has(blk.type)) {
        nh = blk.type === "image" ? Math.round(nw / d.ratio) : d.handle === "se" ? Math.max(60, Math.round(d.rect.h + dyw)) : d.rect.h;
      }
      setResizing({ id: d.id, w: nw, h: nh });
    } else if (d.kind === "connect") {
      const w = toWorld(e.clientX, e.clientY);
      d.moved = true;
      setConnect({ from: d.from, x: w.x, y: w.y });
    }
  };

  const onUp = (e: PointerEvent) => {
    const d = drag.current;
    drag.current = null;
    setGuides([]);
    setMarquee(null);
    if (!d) return;
    if (d.kind === "move") {
      const dl = delta;
      const target = dropTarget;
      setDelta(null);
      setDropTarget(null);
      if (!d.moved) {
        if (!e.shiftKey && L.current.sel.size > 1 && L.current.sel.has(d.clicked)) setSel(new Set([d.clicked]));
        return;
      }
      if (!dl) return;
      const { dx, dy } = dl;
      if (target?.kind === "board" || target?.kind === "crumb") {
        const to = target.boardId;
        commit((tx) => void moveToBoard(tx, boardId, to, d.ids, { x: 40, y: 40 }));
        setSel(new Set());
        return;
      }
      commit((tx) => {
        const bd = tx.board(boardId);
        if (target?.kind === "column") {
          const movable = d.ids
            .map((i) => bd.blocks.find((b) => b.id === i))
            .filter((b): b is Block => !!b && b.type !== "column")
            .sort((a, b) => d.rects[a.id]!.y - d.rects[b.id]!.y);
          movable.forEach((b, k) => putInColumn(bd, b.id, target.id, target.index + k));
          for (const i of d.ids) {
            const b = bd.blocks.find((x) => x.id === i);
            if (b?.type === "column") {
              b.x = Math.round(d.rects[i]!.x + dx);
              b.y = Math.round(d.rects[i]!.y + dy);
            }
          }
          return;
        }
        for (const i of d.ids) {
          const b = bd.blocks.find((x) => x.id === i);
          if (!b) continue;
          if (b.col) freeBlock(bd, i);
          b.x = Math.round(d.rects[i]!.x + dx);
          b.y = Math.round(d.rects[i]!.y + dy);
        }
      });
    } else if (d.kind === "resize") {
      const r = resizingRef.current;
      setResizing(null);
      if (r) commit((tx) => {
        const b = tx.board(boardId).blocks.find((x) => x.id === r.id);
        if (!b) return;
        b.w = r.w;
        if (FIXED_HEIGHT.has(b.type)) b.h = r.h;
      });
    } else if (d.kind === "connect") {
      setConnect(null);
      if (d.moved) {
        const el = document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLElement>("[data-block-id]");
        if (el?.dataset.blockId) connectBlocks(d.from, el.dataset.blockId);
        setTool("select");
      } else if (L.current.tool === "arrow") {
        const from = pendingRef.current;
        if (from && from !== d.from) {
          connectBlocks(from, d.from);
          setPendingFrom(null);
          setTool("select");
        } else setPendingFrom(d.from);
      }
    }
  };

  const resizingRef = useRef(resizing);
  resizingRef.current = resizing;
  const pendingRef = useRef(pendingFrom);
  pendingRef.current = pendingFrom;

  const onWrapDown = (e: React.PointerEvent) => {
    if ((e.target as HTMLElement).closest("[data-canvas-ui]")) return;
    (document.activeElement as HTMLElement | null)?.blur?.();
    setEditingId(null);
    setQuickAdd(null);
    const { tool: t, space: sp, view: v } = L.current;
    if (e.button === 1 || t === "hand" || sp) {
      e.preventDefault();
      drag.current = { kind: "pan", cx: e.clientX, cy: e.clientY, view: v };
      attach();
      return;
    }
    if (e.button !== 0) return;
    setPendingFrom(null);
    const w = toWorld(e.clientX, e.clientY);
    const base = e.shiftKey ? new Set(L.current.sel) : new Set<string>();
    if (!e.shiftKey) setSel(new Set());
    drag.current = { kind: "box", x0: w.x, y0: w.y, base };
    attach();
  };

  const onBlockDown = (e: React.PointerEvent, b: Block) => {
    if (e.button === 1 || L.current.tool === "hand" || L.current.space) return;
    if (e.button !== 0) return;
    e.stopPropagation();
    setQuickAdd(null);
    const t = e.target as HTMLElement;
    if (editingId && editingId !== b.id) {
      (document.activeElement as HTMLElement | null)?.blur?.();
      setEditingId(null);
    }
    if (L.current.tool === "arrow") {
      if (b.type === "arrow") return;
      drag.current = { kind: "connect", from: b.id, moved: false };
      attach();
      return;
    }
    if (e.shiftKey) {
      setSel((s) => {
        const n = new Set(s);
        if (n.has(b.id)) n.delete(b.id);
        else n.add(b.id);
        return n;
      });
      return;
    }
    if (!L.current.sel.has(b.id)) setSel(new Set([b.id]));
    if (t.closest(INTERACTIVE)) return;
    startMove(e, b.id);
  };

  const onBlockDouble = (b: Block) => {
    if (b.type === "doc") onOpenDoc(b.ref);
    else if (b.type === "board" && b.ref) onOpenBoard(b.ref);
    else if (EDITS_IN_PLACE.has(b.type) || b.type === "sketch") setEditingId(b.id);
  };

  const startResize = (e: React.PointerEvent, b: Block, handle: "se" | "e") => {
    e.stopPropagation();
    const r = L.current.rects[b.id];
    if (!r) return;
    drag.current = { kind: "resize", id: b.id, handle, cx: e.clientX, cy: e.clientY, rect: r, ratio: b.type === "image" ? b.ratio || r.w / r.h : r.w / r.h };
    attach();
  };

  const startConnect = (e: React.PointerEvent, b: Block) => {
    e.stopPropagation();
    drag.current = { kind: "connect", from: b.id, moved: false };
    attach();
  };

  // ---- keyboard, clipboard, drops ----
  useEffect(() => {
    const visible = () => !!wrapRef.current && L.current.active && !wrapRef.current.closest("[inert]");
    const editable = (t: EventTarget | null) => !!(t as HTMLElement | null)?.closest?.('input,textarea,select,[contenteditable="true"],[contenteditable="plaintext-only"],.cm-editor,.xterm');
    const onKey = (e: KeyboardEvent) => {
      if (!visible()) return;
      const mod = e.metaKey || e.ctrlKey;
      if (e.key === " " && !editable(e.target)) {
        e.preventDefault();
        setSpace(true);
        return;
      }
      if (editable(e.target)) return;
      const k = e.key.toLowerCase();
      if (mod && k === "z") {
        e.preventDefault();
        if (e.shiftKey) L.current.store.redo();
        else L.current.store.undo();
      } else if (mod && k === "y") {
        e.preventDefault();
        L.current.store.redo();
      } else if (mod && k === "a") {
        e.preventDefault();
        select(L.current.board.blocks.filter((b) => b.type !== "arrow" && !b.col).map((b) => b.id));
      } else if (mod && k === "d") {
        e.preventDefault();
        void duplicate(selectedBlocks(), L.current.board, { dx: 28, dy: 28 });
      } else if (mod && k === "f") {
        e.preventDefault();
        onSearch();
      } else if (mod && (k === "c" || k === "x")) {
        // handled by the copy/cut events below
      } else if (mod) {
        return;
      } else if (e.key === "Delete" || e.key === "Backspace") {
        e.preventDefault();
        remove();
      } else if (e.key === "Escape") {
        setSel(new Set());
        setTool("select");
        setPendingFrom(null);
        setQuickAdd(null);
      } else if (k === "]") order(true);
      else if (k === "[") order(false);
      else if (k === "+" || k === "=") {
        const r = wrapRef.current!.getBoundingClientRect();
        zoomAt(1.2, r.width / 2, r.height / 2);
      } else if (k === "-") {
        const r = wrapRef.current!.getBoundingClientRect();
        zoomAt(1 / 1.2, r.width / 2, r.height / 2);
      } else if (k === "0") setView((v) => ({ ...v, zoom: 1 }));
      else if (k === "1") fit();
      else if (k === "v") setTool("select");
      else if (k === "h") setTool("hand");
      else if (k === "a") setTool("arrow");
      else if (k === "n") add("note");
      else if (k === "t") add("text");
      else if (e.key.startsWith("Arrow") && L.current.sel.size) {
        e.preventDefault();
        const step = e.shiftKey ? 10 : 1;
        const dx = e.key === "ArrowLeft" ? -step : e.key === "ArrowRight" ? step : 0;
        const dy = e.key === "ArrowUp" ? -step : e.key === "ArrowDown" ? step : 0;
        commit((tx) => {
          const bd = tx.board(boardId);
          for (const b of bd.blocks) if (L.current.sel.has(b.id) && !b.col && b.type !== "arrow") {
            b.x += dx;
            b.y += dy;
          }
        }, "nudge");
      }
    };
    const onKeyUp = (e: KeyboardEvent) => {
      if (e.key === " ") setSpace(false);
    };
    const onCopy = (e: ClipboardEvent) => {
      if (!visible() || editable(e.target) || !L.current.sel.size) return;
      const id = copy(false);
      if (id) {
        e.preventDefault();
        e.clipboardData?.setData("text/plain", MARKER + id);
      }
    };
    const onCut = (e: ClipboardEvent) => {
      if (!visible() || editable(e.target) || !L.current.sel.size) return;
      const id = copy(true);
      if (id) {
        e.preventDefault();
        e.clipboardData?.setData("text/plain", MARKER + id);
      }
    };
    const onPaste = (e: ClipboardEvent) => {
      if (!visible() || editable(e.target)) return;
      const files = [...(e.clipboardData?.files ?? [])];
      const text = e.clipboardData?.getData("text/plain") ?? "";
      e.preventDefault();
      if (files.length) void addFiles(files);
      else if (text.startsWith(MARKER) && clipboard && text === MARKER + clipboard.id) paste();
      else if (isWebUrl(text.trim()) && !text.trim().includes("\n")) {
        const blk = makeBlock("link", placeAt(), 0, { url: text.trim() });
        addBlocks([blk]);
      } else if (text.trim()) {
        const blk = makeBlock("note", placeAt(), 0, { html: text.trim().replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/\n/g, "<br>") });
        addBlocks([blk]);
      }
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("keyup", onKeyUp);
    window.addEventListener("copy", onCopy);
    window.addEventListener("cut", onCut);
    window.addEventListener("paste", onPaste);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("keyup", onKeyUp);
      window.removeEventListener("copy", onCopy);
      window.removeEventListener("cut", onCut);
      window.removeEventListener("paste", onPaste);
    };
  }, [select, duplicate, selectedBlocks, onSearch, remove, order, zoomAt, fit, add, commit, boardId, copy, paste, addFiles, placeAt, addBlocks]);

  // ---- render ----
  const ctx: BlockCtx = useMemo(
    () => ({
      projectPath,
      tick,
      boards: store.boards,
      cards: cardMap,
      editingId,
      update,
      startEditing: setEditingId,
      stopEditing: () => setEditingId(null),
      openDoc: onOpenDoc,
      openBoard: onOpenBoard,
      renameBoard: (id, title) =>
        commit((tx) => {
          if (tx.has(id)) tx.board(id).title = title;
        }, `rename:${id}`),
      renderChild: (id) => {
        const b = L.current.byId.get(id);
        return b ? renderShell(b, true) : null;
      },
    }),
    // renderShell reads through refs and state it closes over each render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [projectPath, tick, store.boards, cardMap, editingId, update, onOpenDoc, onOpenBoard, commit, sel, delta, resizing, dropTarget, tool, pendingFrom, view.zoom],
  );

  const renderShell = (b: Block, inColumn: boolean) => {
    const selected = sel.has(b.id);
    const moving = delta?.ids.has(b.id) ?? false;
    const live = resizing?.id === b.id ? resizing : null;
    const w = live?.w ?? b.w;
    const h = live?.h ?? b.h;
    const solo = selected && sel.size === 1 && !inColumn && tool === "select";
    const isTarget = (dropTarget?.kind === "board" && dropTarget.blockId === b.id) || (dropTarget?.kind === "column" && dropTarget.id === b.id);
    const style: React.CSSProperties = inColumn
      ? { position: "relative", width: "100%", zIndex: moving ? 50 : undefined }
      : { position: "absolute", left: b.x, top: b.y, width: w, zIndex: moving ? 50 : b.z };
    if (FIXED_HEIGHT.has(b.type)) style.height = h;
    if (moving && delta) style.transform = `translate(${delta.dx}px, ${delta.dy}px)`;
    const connectable = tool === "arrow";
    return (
      <div
        key={b.id}
        data-block-id={b.id}
        data-block-type={b.type}
        data-testid={`block-${b.type}`}
        className={cn(
          "group/block touch-none",
          moving && "pointer-events-none opacity-90",
          connectable && "cursor-alias",
          pendingFrom === b.id && "ring-2 ring-primary",
        )}
        style={{
          ...style,
          boxShadow: selected ? "0 0 0 calc(2px * var(--inv)) var(--primary)" : isTarget ? "0 0 0 calc(3px * var(--inv)) var(--ring)" : undefined,
          borderRadius: 8,
        }}
        onPointerDown={(e) => onBlockDown(e, b)}
        onDoubleClick={(e) => {
          if ((e.target as HTMLElement).closest("input,textarea,button")) return;
          e.stopPropagation();
          onBlockDouble(b);
        }}
      >
        <BlockContent block={b} ctx={ctx} />
        {solo && (
          <>
            {!inColumn && b.type !== "column" && (
              <div
                data-canvas-ui
                className="absolute -right-1.5 top-1/2 z-10 size-3 -translate-y-1/2 translate-x-full cursor-alias rounded-full border border-primary bg-background hover:bg-primary"
                style={{ transform: "translate(100%, -50%) scale(var(--inv))", transformOrigin: "left center" }}
                title="Drag to another block to connect"
                onPointerDown={(e) => startConnect(e, b)}
              />
            )}
            <div
              data-canvas-ui
              className="absolute -right-1 top-1/2 z-10 h-6 w-2 -translate-y-1/2 cursor-ew-resize rounded-full border border-primary bg-background"
              style={{ transform: "translateY(-50%) scale(var(--inv))" }}
              onPointerDown={(e) => startResize(e, b, "e")}
            />
            <div
              data-canvas-ui
              className="absolute -bottom-1 -right-1 z-10 size-3 cursor-nwse-resize rounded-sm border border-primary bg-background"
              style={{ transform: "scale(var(--inv))", transformOrigin: "bottom right" }}
              onPointerDown={(e) => startResize(e, b, "se")}
            />
          </>
        )}
      </div>
    );
  };

  const free = board.blocks.filter((b) => b.type !== "arrow" && !b.col).sort((a, b) => a.z - b.z);
  const arrows = board.blocks.filter((b): b is Extract<Block, { type: "arrow" }> => b.type === "arrow");

  const selRect = useMemo(() => {
    const rs = [...sel].map((id) => rects[id]).filter((r): r is Rect => !!r);
    const u = unionRect(rs);
    if (!u || delta || resizing) return null;
    return u;
  }, [sel, rects, delta, resizing]);
  const selBlocks = [...sel].map((id) => byId.get(id)).filter((b): b is Block => !!b);
  const allNotes = selBlocks.length > 0 && selBlocks.every((b) => b.type === "note");
  const freeSel = selBlocks.filter((b) => !b.col && b.type !== "arrow");
  const arrowSel = selBlocks.length === 1 && selBlocks[0]!.type === "arrow";

  const barPos = selRect
    ? (() => {
        const sx = selRect.x * view.zoom + view.x;
        const sy = selRect.y * view.zoom + view.y;
        const wrapW = wrapRef.current?.clientWidth ?? 800;
        return { left: Math.min(Math.max(sx + (selRect.w * view.zoom) / 2, 160), Math.max(160, wrapW - 160)), top: sy > 56 ? sy - 46 : sy + selRect.h * view.zoom + 8 };
      })()
    : null;

  const dots = Math.max(8, 24 * view.zoom);
  const hand = tool === "hand" || space;

  return (
    <div
      ref={wrapRef}
      data-testid="canvas"
      data-zoom={view.zoom.toFixed(3)}
      className={cn("relative size-full touch-none overflow-hidden rounded-xl border border-border bg-background", hand ? "cursor-grab" : tool === "arrow" ? "cursor-crosshair" : "")}
      style={{
        backgroundImage: "radial-gradient(circle, color-mix(in oklab, var(--foreground) 14%, transparent) 1px, transparent 1px)",
        backgroundSize: `${dots}px ${dots}px`,
        backgroundPosition: `${view.x}px ${view.y}px`,
      }}
      onPointerDown={onWrapDown}
      onDoubleClick={(e) => {
        if ((e.target as HTMLElement).closest("[data-block-id],[data-canvas-ui]")) return;
        const r = wrapRef.current!.getBoundingClientRect();
        setQuickAdd({ sx: e.clientX - r.left, sy: e.clientY - r.top, ...toWorld(e.clientX, e.clientY) });
      }}
      onDragOver={(e) => {
        if (e.dataTransfer.types.includes("Files")) {
          e.preventDefault();
          setFileDrag(true);
        }
      }}
      onDragLeave={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setFileDrag(false);
      }}
      onDrop={(e) => {
        setFileDrag(false);
        const files = [...e.dataTransfer.files];
        if (!files.length) return;
        e.preventDefault();
        void addFiles(files, toWorld(e.clientX, e.clientY));
      }}
    >
      <div
        className="absolute left-0 top-0 origin-top-left"
        style={{ transform: `translate(${view.x}px, ${view.y}px) scale(${view.zoom})`, ["--inv" as string]: 1 / view.zoom }}
      >
        {free.map((b) => renderShell(b, false))}

        <svg width={1} height={1} style={{ position: "absolute", left: 0, top: 0, overflow: "visible", pointerEvents: "none", zIndex: 9000 }}>
          {arrows.map((a) => {
            const rf = rects[a.from];
            const rt = rects[a.to];
            if (!rf || !rt) return null;
            const p1 = edgePoint(rf, { x: rt.x + rt.w / 2, y: rt.y + rt.h / 2 });
            const p2 = edgePoint(rt, { x: rf.x + rf.w / 2, y: rf.y + rf.h / 2 });
            const ang = Math.atan2(p2.y - p1.y, p2.x - p1.x);
            const head = 11;
            const hx = (k: number) => p2.x - head * Math.cos(ang + k);
            const hy = (k: number) => p2.y - head * Math.sin(ang + k);
            const on = sel.has(a.id);
            return (
              <g key={a.id} className={on ? "text-primary" : "text-foreground/70"} data-arrow-id={a.id} data-testid="block-arrow">
                <line
                  x1={p1.x} y1={p1.y} x2={p2.x} y2={p2.y}
                  stroke="transparent" strokeWidth={14} style={{ pointerEvents: "stroke", cursor: "pointer" }}
                  onPointerDown={(e) => {
                    e.stopPropagation();
                    if (e.shiftKey) setSel((s) => new Set(s.has(a.id) ? [...s].filter((i) => i !== a.id) : [...s, a.id]));
                    else setSel(new Set([a.id]));
                  }}
                  onDoubleClick={() => setEditingId(a.id)}
                />
                <line x1={p1.x} y1={p1.y} x2={p2.x - 8 * Math.cos(ang)} y2={p2.y - 8 * Math.sin(ang)} stroke="currentColor" strokeWidth={on ? 2.5 : 2} />
                <polygon points={`${p2.x},${p2.y} ${hx(0.4)},${hy(0.4)} ${hx(-0.4)},${hy(-0.4)}`} fill="currentColor" />
              </g>
            );
          })}
          {connect && rects[connect.from] && (
            <line
              x1={rects[connect.from]!.x + rects[connect.from]!.w / 2} y1={rects[connect.from]!.y + rects[connect.from]!.h / 2}
              x2={connect.x} y2={connect.y} stroke="var(--primary)" strokeWidth={2} strokeDasharray="6 4"
            />
          )}
          {guides.map((g, i) =>
            g.axis === "x" ? (
              <line key={i} x1={g.pos} x2={g.pos} y1={g.from - 20} y2={g.to + 20} stroke="#f43f5e" strokeWidth={1 / view.zoom} />
            ) : (
              <line key={i} y1={g.pos} y2={g.pos} x1={g.from - 20} x2={g.to + 20} stroke="#f43f5e" strokeWidth={1 / view.zoom} />
            ),
          )}
        </svg>

        {arrows.map((a) => {
          const rf = rects[a.from];
          const rt = rects[a.to];
          if (!rf || !rt) return null;
          const p1 = edgePoint(rf, { x: rt.x + rt.w / 2, y: rt.y + rt.h / 2 });
          const p2 = edgePoint(rt, { x: rf.x + rf.w / 2, y: rf.y + rf.h / 2 });
          const editing = editingId === a.id;
          if (!a.label && !editing) return null;
          return (
            <div
              key={a.id}
              data-canvas-ui
              className="absolute z-[9001] -translate-x-1/2 -translate-y-1/2 rounded-full border border-border bg-card px-2 py-0.5 text-xs shadow-sm"
              style={{ left: (p1.x + p2.x) / 2, top: (p1.y + p2.y) / 2 }}
              onPointerDown={(e) => {
                e.stopPropagation();
                setSel(new Set([a.id]));
              }}
              onDoubleClick={() => setEditingId(a.id)}
            >
              {editing ? (
                <input
                  autoFocus
                  value={a.label}
                  size={Math.max(6, a.label.length + 1)}
                  onChange={(e) => update(a.id, { label: e.target.value })}
                  onBlur={() => setEditingId(null)}
                  onKeyDown={(e) => (e.key === "Enter" || e.key === "Escape") && (e.target as HTMLElement).blur()}
                  className="bg-transparent text-center outline-none"
                />
              ) : (
                a.label
              )}
            </div>
          );
        })}

        {marquee && (
          <div
            className="pointer-events-none absolute border border-primary bg-primary/10"
            style={{ left: marquee.x, top: marquee.y, width: marquee.w, height: marquee.h }}
          />
        )}
      </div>

      {board.blocks.length === 0 && (
        <div className="pointer-events-none absolute inset-0 flex items-center justify-center text-sm text-muted-foreground">Double-click to add</div>
      )}

      <Toolbar
        tool={tool}
        setTool={(t) => {
          setTool(t);
          setPendingFrom(null);
        }}
        onAdd={(t) => add(t)}
        onNewDoc={(t) => void addDocument(t)}
        onUndo={store.undo}
        onRedo={store.redo}
        canUndo={store.canUndo}
        canRedo={store.canRedo}
      />

      {barPos && selBlocks.length > 0 && !editingId && (
        <div
          data-canvas-ui
          data-testid="selection-bar"
          className="absolute z-40 flex -translate-x-1/2 items-center gap-0.5 rounded-lg border border-border bg-card p-1 shadow-md"
          style={barPos}
        >
          {allNotes &&
            NOTE_COLORS.map((c: NoteColor) => (
              <button
                key={c}
                type="button"
                aria-label={`Color ${c}`}
                data-testid={`note-color-${c}`}
                onClick={() => commit((tx) => tx.board(boardId).blocks.forEach((b) => sel.has(b.id) && b.type === "note" && (b.color = c)))}
                className="size-5 rounded-full border border-black/10"
                style={{ background: NOTE_BG[c] }}
              />
            ))}
          {allNotes && <div className="mx-0.5 h-5 w-px bg-border" />}
          {freeSel.length > 1 && (
            <>
              {(
                [
                  ["left", AlignStartVertical, "Align left"],
                  ["hcenter", AlignCenterVertical, "Align center"],
                  ["right", AlignEndVertical, "Align right"],
                  ["top", AlignStartHorizontal, "Align top"],
                  ["vcenter", AlignCenterHorizontal, "Align middle"],
                  ["bottom", AlignEndHorizontal, "Align bottom"],
                ] as const
              ).map(([m, Icon, label]) => (
                <BarBtn key={m} label={label} testId={`align-${m}`} onClick={() => align(m)}>
                  <Icon className="size-4" />
                </BarBtn>
              ))}
              {freeSel.length > 2 && (
                <>
                  <BarBtn label="Space evenly across" testId="align-hdist" onClick={() => align("hdist")}>
                    <span className="text-[11px] font-medium">↔</span>
                  </BarBtn>
                  <BarBtn label="Space evenly down" testId="align-vdist" onClick={() => align("vdist")}>
                    <span className="text-[11px] font-medium">↕</span>
                  </BarBtn>
                </>
              )}
              <div className="mx-0.5 h-5 w-px bg-border" />
            </>
          )}
          {!arrowSel && (
            <>
              <BarBtn label="Duplicate" testId="bar-duplicate" onClick={() => void duplicate(selectedBlocks(), board, { dx: 28, dy: 28 })}>
                <Copy className="size-4" />
              </BarBtn>
              <BarBtn label="Bring to front" onClick={() => order(true)}>
                <ArrowUpToLine className="size-4" />
              </BarBtn>
              <BarBtn label="Send to back" onClick={() => order(false)}>
                <ArrowDownToLine className="size-4" />
              </BarBtn>
            </>
          )}
          <BarBtn label="Delete" testId="bar-delete" onClick={remove}>
            <Trash2 className="size-4" />
          </BarBtn>
        </div>
      )}

      {quickAdd && (
        <div
          data-canvas-ui
          data-testid="quick-add"
          className="absolute z-40 grid w-44 grid-cols-1 rounded-lg border border-border bg-card p-1 shadow-md"
          style={{ left: Math.min(quickAdd.sx, (wrapRef.current?.clientWidth ?? 600) - 190), top: Math.min(quickAdd.sy, (wrapRef.current?.clientHeight ?? 400) - 330) }}
        >
          {(["note", "doc", "todo", "column", "board", "image", "file", "link", "sketch", "swatch", "table", "text", "comment"] as BlockType[]).map((t) => (
            <button
              key={t}
              type="button"
              data-testid={`quick-${t}`}
              className="rounded px-2 py-1 text-left text-sm hover:bg-foreground/10"
              onClick={() => {
                setQuickAdd(null);
                add(t, { x: quickAdd.x, y: quickAdd.y });
              }}
            >
              {QUICK_LABEL[t]}
            </button>
          ))}
        </div>
      )}

      <div data-canvas-ui className="absolute bottom-3 right-3 z-30 flex items-center gap-0.5 rounded-lg border border-border bg-card/95 p-0.5 shadow-md">
        <BarBtn label="Zoom out" onClick={() => zoomAt(1 / 1.25, (wrapRef.current?.clientWidth ?? 0) / 2, (wrapRef.current?.clientHeight ?? 0) / 2)}>
          <Minus className="size-4" />
        </BarBtn>
        <button
          type="button"
          title="Reset zoom"
          data-testid="zoom-level"
          onClick={() => setView((v) => ({ ...v, zoom: 1 }))}
          className="min-w-12 rounded px-1 text-center text-xs tabular-nums text-muted-foreground hover:bg-foreground/10"
        >
          {Math.round(view.zoom * 100)}%
        </button>
        <BarBtn label="Zoom in" onClick={() => zoomAt(1.25, (wrapRef.current?.clientWidth ?? 0) / 2, (wrapRef.current?.clientHeight ?? 0) / 2)}>
          <Plus className="size-4" />
        </BarBtn>
        <BarBtn label="Fit everything" testId="zoom-fit" onClick={fit}>
          <Maximize className="size-4" />
        </BarBtn>
      </div>

      {fileDrag && (
        <div className="pointer-events-none absolute inset-2 z-50 flex items-center justify-center rounded-xl border-2 border-dashed border-primary bg-primary/5 text-sm font-medium">
          Drop to add
        </div>
      )}

      <input
        ref={fileInput}
        type="file"
        multiple
        hidden
        data-testid="canvas-file-input"
        onChange={(e) => {
          const files = [...(e.target.files ?? [])];
          e.target.value = "";
          if (files.length) void addFiles(files);
        }}
      />
    </div>
  );
}

const QUICK_LABEL: Record<BlockType, string> = {
  doc: "Document",
  note: "Note",
  todo: "To-do list",
  column: "Column",
  board: "Board",
  image: "Image",
  file: "File",
  link: "Link",
  arrow: "Arrow",
  sketch: "Sketch",
  swatch: "Color",
  table: "Table",
  text: "Text",
  comment: "Comment",
};

function BarBtn({ label, onClick, children, testId }: { label: string; onClick: () => void; children: React.ReactNode; testId?: string }) {
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      data-testid={testId}
      onClick={onClick}
      className="flex size-7 items-center justify-center rounded-md text-muted-foreground hover:bg-foreground/10 hover:text-foreground"
    >
      {children}
    </button>
  );
}
