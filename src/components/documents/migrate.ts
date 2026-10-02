import { invoke } from "@tauri-apps/api/core";
import { contextFolders } from "@/lib/studio-api";
import type { CardSummary } from "@/lib/studio-types";
import type { Tx } from "./store";
import { emptyBoard, makeBlock, newId, ROOT_BOARD, type Block, type Board } from "./types";

// The first time Documents opens a project, what was there before is folded
// in: every folder of notes becomes a board, every note a document on it, and
// every old graph diagram a board of notes joined by arrows. Nothing is
// deleted or rewritten; the notes stay where they are.

interface FileEntry {
  name: string;
  path: string;
  is_dir: boolean;
  children: FileEntry[] | null;
}

interface FlowNode {
  id: string;
  type?: string;
  position?: { x: number; y: number };
  data?: { label?: string };
}
interface FlowEdge {
  id?: string;
  source: string;
  target: string;
  data?: { label?: string };
}

interface LegacyGraph {
  name: string;
  nodes: FlowNode[];
  edges: FlowEdge[];
}

function flatten(entries: FileEntry[]): FileEntry[] {
  return entries.flatMap((e) => (e.is_dir ? flatten(e.children ?? []) : [e]));
}

async function legacyGraphs(projectPath: string): Promise<LegacyGraph[]> {
  let files: FileEntry[];
  try {
    files = flatten(await invoke<FileEntry[]>("list_directory", { path: `${projectPath}/.ibproject/graphs` }));
  } catch {
    return [];
  }
  const out: LegacyGraph[] = [];
  for (const f of files.filter((x) => x.name.endsWith(".graph.json"))) {
    try {
      const doc = JSON.parse(await invoke<string>("read_file", { path: f.path })) as { nodes?: FlowNode[]; edges?: FlowEdge[] };
      const nodes = (doc.nodes ?? []).filter((n) => n && typeof n.id === "string");
      if (!nodes.length) continue;
      out.push({ name: f.name.replace(/\.graph\.json$/, ""), nodes, edges: (doc.edges ?? []).filter((e) => e && e.source && e.target) });
    } catch {
      // An unreadable diagram is left where it is.
    }
  }
  return out;
}

const escape = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
const pretty = (s: string) => {
  const t = s.replace(/[-_]+/g, " ").trim();
  return t ? t[0]!.toUpperCase() + t.slice(1) : "Untitled";
};

/** Everything needed to build the first root board; apply the result with `store.seed`. */
export async function prepareRoot(projectPath: string, cards: CardSummary[]): Promise<(tx: Tx) => void> {
  const [folders, graphs] = await Promise.all([contextFolders(projectPath).catch(() => [] as string[]), legacyGraphs(projectPath)]);
  return (tx) => {
    const root: Board = { ...emptyBoard(ROOT_BOARD, "Documents", null), migrated: true };
    tx.add(root);
    const boards = new Map<string, Board>([["", root]]);
    const slots = new Map<string, number>();
    const place = (board: Board, type: "doc" | "board", extra: Record<string, unknown>) => {
      const n = slots.get(board.id) ?? 0;
      slots.set(board.id, n + 1);
      board.blocks.push(makeBlock(type, { x: 60 + (n % 4) * 280, y: 60 + Math.floor(n / 4) * 200 }, n, extra));
    };
    const folderSet = new Set(folders);
    for (const c of cards) {
      const parts = c.path.split("/");
      for (let i = 1; i < parts.length; i += 1) folderSet.add(parts.slice(0, i).join("/"));
    }
    const ordered = [...folderSet].sort((a, b) => a.split("/").length - b.split("/").length || a.localeCompare(b));
    for (const f of ordered) {
      const parentPath = f.includes("/") ? f.slice(0, f.lastIndexOf("/")) : "";
      const parent = boards.get(parentPath) ?? root;
      const b = emptyBoard(newId("bd"), pretty(f.split("/").pop()!), parent.id);
      tx.add(b);
      boards.set(f, b);
      place(parent, "board", { ref: b.id });
    }
    for (const c of cards) {
      const dir = c.path.includes("/") ? c.path.slice(0, c.path.lastIndexOf("/")) : "";
      place(boards.get(dir) ?? root, "doc", { ref: c.path });
    }
    for (const g of graphs) {
      const board = emptyBoard(newId("bd"), pretty(g.name), ROOT_BOARD);
      const ids = new Map<string, string>();
      for (const n of g.nodes) {
        const color = n.type === "diamond" ? "blue" : n.type === "circle" ? "green" : "yellow";
        const blk = makeBlock("note", { x: n.position?.x ?? 0, y: n.position?.y ?? 0 }, board.blocks.length, { html: escape(n.data?.label ?? ""), color });
        blk.w = 180;
        ids.set(n.id, blk.id);
        board.blocks.push(blk);
      }
      for (const e of g.edges) {
        const from = ids.get(e.source);
        const to = ids.get(e.target);
        if (!from || !to) continue;
        const arrow = makeBlock("arrow", { x: 0, y: 0 }, board.blocks.length, { from, to, label: e.data?.label ?? "" }) as Block;
        board.blocks.push(arrow);
      }
      tx.add(board);
      place(root, "board", { ref: board.id });
    }
  };
}
