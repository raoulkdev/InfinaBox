import { useCallback, useEffect, useRef, useState } from "react";
import { boardDelete, boardWrite, boardsReadAll } from "@/lib/studio-api";
import { onProjectFilesChanged } from "@/lib/fs-watch";
import { parseBoard, serializeBoard, type Board, type Boards } from "./types";

// All of a project's boards, in memory, with undo/redo and saving. Every edit
// goes through `commit`, which records what the touched boards looked like
// before and after, so one undo can reverse an edit that spans two boards
// (a block moved from one to another).

const SAVE_DELAY_MS = 400;
const COALESCE_MS = 1500;
const MAX_HISTORY = 200;

/** The boards an edit touches. Mutate only what `board()` hands back. */
export class Tx {
  readonly touched = new Map<string, Board>();
  readonly removed = new Set<string>();
  constructor(private readonly base: Boards) {}

  has(id: string): boolean {
    return this.touched.has(id) || (!this.removed.has(id) && !!this.base[id]);
  }

  board(id: string): Board {
    const t = this.touched.get(id);
    if (t) return t;
    const b = this.base[id];
    if (!b || this.removed.has(id)) throw new Error(`No board ${id}`);
    const copy = structuredClone(b);
    this.touched.set(id, copy);
    return copy;
  }

  add(board: Board): void {
    this.removed.delete(board.id);
    this.touched.set(board.id, board);
  }

  remove(id: string): void {
    this.touched.delete(id);
    this.removed.add(id);
  }
}

interface Entry {
  before: Record<string, Board | null>;
  after: Record<string, Board | null>;
  key?: string;
  at: number;
}

export interface BoardStore {
  boards: Boards;
  loaded: boolean;
  saveError: string | null;
  canUndo: boolean;
  canRedo: boolean;
  /** An undoable edit. Edits with the same `key` in quick succession merge into one step. */
  commit: (mutate: (tx: Tx) => void, key?: string) => void;
  /** An edit that is not an undo step (first-run setup). */
  seed: (mutate: (tx: Tx) => void) => void;
  undo: () => void;
  redo: () => void;
}

export function useBoardStore(projectPath: string): BoardStore {
  const [boards, setBoards] = useState<Boards>({});
  const [loaded, setLoaded] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [, setVersion] = useState(0);
  const ref = useRef<Boards>({});
  const undoStack = useRef<Entry[]>([]);
  const redoStack = useRef<Entry[]>([]);
  const lastWritten = useRef(new Map<string, string>());
  const dirty = useRef(new Set<string>());
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const project = useRef(projectPath);
  project.current = projectPath;

  const bumpHistory = useCallback(() => setVersion((n) => n + 1), []);

  const flush = useCallback(async () => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
    const ids = [...dirty.current];
    dirty.current.clear();
    const path = project.current;
    for (const id of ids) {
      const board = ref.current[id];
      try {
        if (board) {
          const json = serializeBoard(board);
          lastWritten.current.set(id, json);
          await boardWrite(path, id, json);
        } else {
          lastWritten.current.delete(id);
          await boardDelete(path, id);
        }
        setSaveError(null);
      } catch (err) {
        dirty.current.add(id);
        setSaveError(typeof err === "string" ? err : err instanceof Error ? err.message : "Couldn't save the boards.");
      }
    }
  }, []);

  const schedule = useCallback(() => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => void flush(), SAVE_DELAY_MS);
  }, [flush]);

  const applyChanges = useCallback(
    (changes: Record<string, Board | null>) => {
      const next = { ...ref.current };
      for (const [id, board] of Object.entries(changes)) {
        if (board) next[id] = board;
        else delete next[id];
        dirty.current.add(id);
      }
      ref.current = next;
      setBoards(next);
      schedule();
    },
    [schedule],
  );

  const run = useCallback((mutate: (tx: Tx) => void): Entry | null => {
    const base = ref.current;
    const tx = new Tx(base);
    mutate(tx);
    const before: Record<string, Board | null> = {};
    const after: Record<string, Board | null> = {};
    for (const [id, b] of tx.touched) {
      const old = base[id] ?? null;
      if (old && JSON.stringify(old) === JSON.stringify(b)) continue;
      before[id] = old;
      after[id] = b;
    }
    for (const id of tx.removed) {
      if (!base[id]) continue;
      before[id] = base[id];
      after[id] = null;
    }
    if (Object.keys(after).length === 0) return null;
    return { before, after, at: Date.now() };
  }, []);

  const commit = useCallback(
    (mutate: (tx: Tx) => void, key?: string) => {
      const entry = run(mutate);
      if (!entry) return;
      entry.key = key;
      const last = undoStack.current[undoStack.current.length - 1];
      const sameBoards = last && Object.keys(last.after).sort().join() === Object.keys(entry.after).sort().join();
      if (key && last && last.key === key && sameBoards && entry.at - last.at < COALESCE_MS) {
        last.after = entry.after;
        last.at = entry.at;
      } else {
        undoStack.current.push(entry);
        if (undoStack.current.length > MAX_HISTORY) undoStack.current.shift();
      }
      redoStack.current = [];
      applyChanges(entry.after);
      bumpHistory();
    },
    [run, applyChanges, bumpHistory],
  );

  const seed = useCallback(
    (mutate: (tx: Tx) => void) => {
      const entry = run(mutate);
      if (entry) applyChanges(entry.after);
    },
    [run, applyChanges],
  );

  const undo = useCallback(() => {
    const entry = undoStack.current.pop();
    if (!entry) return;
    redoStack.current.push(entry);
    applyChanges(entry.before);
    bumpHistory();
  }, [applyChanges, bumpHistory]);

  const redo = useCallback(() => {
    const entry = redoStack.current.pop();
    if (!entry) return;
    undoStack.current.push(entry);
    applyChanges(entry.after);
    bumpHistory();
  }, [applyChanges, bumpHistory]);

  // Read what's on disk: everything at first, then whatever changed behind
  // our back (a version restored, a project pulled). Boards with edits not yet
  // saved are left alone.
  const readDisk = useCallback(async (first: boolean) => {
    const path = project.current;
    let stored;
    try {
      stored = await boardsReadAll(path);
    } catch (err) {
      if (first) setSaveError(typeof err === "string" ? err : "Couldn't read the boards.");
      return;
    }
    if (path !== project.current) return;
    const next = { ...ref.current };
    let changed = false;
    const seen = new Set<string>();
    for (const s of stored) {
      seen.add(s.id);
      if (dirty.current.has(s.id) || lastWritten.current.get(s.id) === s.json) continue;
      const board = parseBoard(s.id, s.json);
      lastWritten.current.set(s.id, s.json);
      if (!board) continue;
      next[s.id] = board;
      changed = true;
    }
    for (const id of Object.keys(next)) {
      if (!seen.has(id) && lastWritten.current.has(id) && !dirty.current.has(id)) {
        delete next[id];
        lastWritten.current.delete(id);
        changed = true;
      }
    }
    if (changed) {
      ref.current = next;
      setBoards(next);
      if (!first) {
        undoStack.current = [];
        redoStack.current = [];
        bumpHistory();
      }
    }
    if (first) setLoaded(true);
  }, [bumpHistory]);

  useEffect(() => {
    ref.current = {};
    setBoards({});
    setLoaded(false);
    undoStack.current = [];
    redoStack.current = [];
    lastWritten.current.clear();
    dirty.current.clear();
    void readDisk(true);
    let t: ReturnType<typeof setTimeout> | null = null;
    const off = onProjectFilesChanged(() => {
      if (t) clearTimeout(t);
      t = setTimeout(() => void readDisk(false), 250);
    });
    const onHide = () => void flush();
    window.addEventListener("beforeunload", onHide);
    return () => {
      off();
      if (t) clearTimeout(t);
      window.removeEventListener("beforeunload", onHide);
      void flush();
    };
  }, [projectPath, readDisk, flush]);

  return {
    boards,
    loaded,
    saveError,
    canUndo: undoStack.current.length > 0,
    canRedo: redoStack.current.length > 0,
    commit,
    seed,
    undo,
    redo,
  };
}
