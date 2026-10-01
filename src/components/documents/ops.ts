import type { Tx } from "./store";
import {
  emptyBoard,
  newId,
  topZ,
  withDescendants,
  type Block,
  type Board,
  type Boards,
} from "./types";

// Edits to boards, as functions of a transaction. Each is one undo step.

/** Removes blocks (and what depends on them: a column's contents, arrows that
 * touch them, a board block's whole nested board). Documents stay on disk. */
export function deleteBlocks(tx: Tx, boardId: string, ids: Iterable<string>): void {
  const board = tx.board(boardId);
  const gone = withDescendants(board, ids);
  for (const b of board.blocks) {
    if (b.type === "arrow" && (gone.has(b.from) || gone.has(b.to))) gone.add(b.id);
  }
  for (const b of board.blocks) {
    if (gone.has(b.id) && b.type === "board" && b.ref) removeBoardTree(tx, b.ref);
  }
  board.blocks = board.blocks.filter((b) => !gone.has(b.id));
  for (const b of board.blocks) {
    if (b.type === "column") b.children = b.children.filter((c) => !gone.has(c));
  }
}

/** Drops a board and every board nested inside it. */
export function removeBoardTree(tx: Tx, id: string, seen = new Set<string>()): void {
  if (seen.has(id) || !tx.has(id)) return;
  seen.add(id);
  for (const b of tx.board(id).blocks) {
    if (b.type === "board" && b.ref) removeBoardTree(tx, b.ref, seen);
  }
  tx.remove(id);
}

/** Takes a block out of its column (if any), leaving it free at (x, y). */
export function freeBlock(board: Board, id: string, at?: { x: number; y: number }): void {
  const b = board.blocks.find((x) => x.id === id);
  if (!b || !b.col) return;
  const col = board.blocks.find((x) => x.id === b.col);
  if (col && col.type === "column") col.children = col.children.filter((c) => c !== id);
  delete b.col;
  if (at) {
    b.x = Math.round(at.x);
    b.y = Math.round(at.y);
  }
  b.z = topZ(board);
}

/** Puts a block into a column at `index` (end if omitted). */
export function putInColumn(board: Board, id: string, columnId: string, index?: number): void {
  const b = board.blocks.find((x) => x.id === id);
  const col = board.blocks.find((x) => x.id === columnId);
  if (!b || !col || col.type !== "column" || b.type === "arrow" || b.type === "column" || b.id === col.id) return;
  if (b.col) {
    const old = board.blocks.find((x) => x.id === b.col);
    if (old && old.type === "column") old.children = old.children.filter((c) => c !== id);
  }
  const children = col.children.filter((c) => c !== id);
  children.splice(index ?? children.length, 0, id);
  col.children = children;
  b.col = columnId;
}

/** Moves blocks (with what rides along) to another board, laid out from (x, y).
 * Arrows between moved blocks go too; arrows to blocks left behind are dropped. */
export function moveToBoard(tx: Tx, from: string, to: string, ids: Iterable<string>, origin?: { x: number; y: number }): string[] {
  if (from === to) return [];
  const src = tx.board(from);
  const dst = tx.board(to);
  const moving = withDescendants(src, ids);
  // A board can't be moved into itself or something inside it.
  for (const b of src.blocks) {
    if (moving.has(b.id) && b.type === "board" && (b.ref === to || isInside(tx, to, b.ref))) moving.delete(b.id);
  }
  for (const b of src.blocks) {
    if (b.type === "arrow" && !(moving.has(b.from) && moving.has(b.to)) && (moving.has(b.from) || moving.has(b.to))) {
      moving.delete(b.id);
    }
  }
  const moved = src.blocks.filter((b) => moving.has(b.id));
  if (!moved.length) return [];
  // Arrows left behind whose end moved away are removed.
  const movedIds = new Set(moved.map((b) => b.id));
  src.blocks = src.blocks.filter((b) => !movedIds.has(b.id) && !(b.type === "arrow" && (movedIds.has(b.from) || movedIds.has(b.to))));
  for (const b of src.blocks) if (b.type === "column") b.children = b.children.filter((c) => !movedIds.has(c));
  const free = moved.filter((b) => !b.col || !movedIds.has(b.col));
  const minX = Math.min(...free.filter((b) => b.type !== "arrow").map((b) => b.x), Infinity);
  const minY = Math.min(...free.filter((b) => b.type !== "arrow").map((b) => b.y), Infinity);
  let z = topZ(dst);
  for (const b of moved) {
    if (b.type !== "arrow" && !b.col) {
      if (origin && Number.isFinite(minX)) {
        b.x = Math.round(b.x - minX + origin.x);
        b.y = Math.round(b.y - minY + origin.y);
      }
      b.z = z++;
    }
    if (b.col && !movedIds.has(b.col)) delete b.col;
    if (b.type === "board" && b.ref && tx.has(b.ref)) tx.board(b.ref).parent = to;
  }
  dst.blocks.push(...moved);
  return moved.map((b) => b.id);
}

/** Whether `candidate` is `ancestor` or sits somewhere inside it. */
export function isInside(tx: Tx, candidate: string, ancestor: string): boolean {
  let cur: string | null = candidate;
  const seen = new Set<string>();
  while (cur && !seen.has(cur)) {
    if (cur === ancestor) return true;
    seen.add(cur);
    cur = tx.has(cur) ? tx.board(cur).parent : null;
  }
  return false;
}

/** The boards above `id`, root first, ending with `id` itself. */
export function pathTo(boards: Boards, id: string): Board[] {
  const out: Board[] = [];
  const seen = new Set<string>();
  let cur: Board | undefined = boards[id];
  while (cur && !seen.has(cur.id)) {
    out.unshift(cur);
    seen.add(cur.id);
    cur = cur.parent ? boards[cur.parent] : undefined;
  }
  return out;
}

/** A new nested board and its block on `parent`. Returns the block. */
export function addBoardBlock(tx: Tx, parent: string, at: { x: number; y: number }, title = "Untitled board"): Block {
  const id = newId("bd");
  tx.add(emptyBoard(id, title, parent));
  const pb = tx.board(parent);
  const block = {
    id: newId(),
    type: "board",
    ref: id,
    x: Math.round(at.x),
    y: Math.round(at.y),
    w: 220,
    h: 150,
    z: topZ(pb),
  } as Block;
  pb.blocks.push(block);
  return block;
}

/** Copies blocks (for duplicate and paste). Ids are fresh; arrows keep their
 * ends only when both ends are copied; board blocks get a deep copy of their
 * board. `docCopy` maps a document path to its copy's path. */
export function cloneBlocks(
  tx: Tx,
  source: Boards,
  blocks: Block[],
  into: string,
  docCopy: (ref: string) => string,
  offset: { dx: number; dy: number },
): Block[] {
  const idMap = new Map<string, string>();
  for (const b of blocks) idMap.set(b.id, newId());
  const dst = tx.board(into);
  let z = topZ(dst);
  const out: Block[] = [];
  for (const orig of blocks) {
    if (orig.type === "arrow" && !(idMap.has(orig.from) && idMap.has(orig.to))) continue;
    const b = structuredClone(orig) as Block;
    b.id = idMap.get(orig.id)!;
    if (b.col) {
      if (idMap.has(b.col)) b.col = idMap.get(b.col)!;
      else {
        delete b.col;
        b.x = orig.x + offset.dx;
        b.y = orig.y + offset.dy;
      }
    } else {
      b.x = orig.x + offset.dx;
      b.y = orig.y + offset.dy;
      b.z = z++;
    }
    if (b.type === "arrow") {
      b.from = idMap.get(b.from)!;
      b.to = idMap.get(b.to)!;
    } else if (b.type === "column") {
      b.children = b.children.filter((c) => idMap.has(c)).map((c) => idMap.get(c)!);
    } else if (b.type === "doc") {
      b.ref = docCopy(b.ref);
    } else if (b.type === "board") {
      b.ref = cloneBoardTree(tx, source, b.ref, into, docCopy);
    }
    out.push(b);
  }
  dst.blocks.push(...out);
  return out;
}

function cloneBoardTree(tx: Tx, source: Boards, id: string, parent: string, docCopy: (ref: string) => string): string {
  const orig = source[id];
  const copyId = newId("bd");
  if (!orig) {
    tx.add(emptyBoard(copyId, "Untitled board", parent));
    return copyId;
  }
  tx.add({ ...emptyBoard(copyId, orig.title, parent), icon: orig.icon });
  const inner = cloneBlocks(tx, source, orig.blocks, copyId, docCopy, { dx: 0, dy: 0 });
  void inner;
  return copyId;
}
