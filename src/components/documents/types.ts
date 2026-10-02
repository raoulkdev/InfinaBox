// The Documents canvas: boards hold blocks placed at free positions. A board
// is saved as one JSON file (`.ibproject/boards/<id>.json`); a document block
// only points at a Markdown card, which stays an ordinary card the AI can read.

export const ROOT_BOARD = "root";

export const NOTE_COLORS = ["yellow", "pink", "blue", "green", "purple", "gray"] as const;
export type NoteColor = (typeof NOTE_COLORS)[number];

interface Base {
  id: string;
  x: number;
  y: number;
  w: number;
  h: number;
  /** Layer: higher is in front. */
  z: number;
  /** The column this block sits inside, if any (its x/y are then unused). */
  col?: string;
}

export interface TodoItem {
  id: string;
  text: string;
  done: boolean;
}

export type Block = Base &
  (
    | { type: "doc"; ref: string }
    | { type: "note"; html: string; color: NoteColor }
    | { type: "todo"; title: string; items: TodoItem[] }
    | { type: "column"; title: string; children: string[] }
    | { type: "board"; ref: string }
    | { type: "image"; src: string; caption: string; ratio: number }
    | { type: "file"; src: string; name: string; size: number }
    | { type: "link"; url: string; title: string; description: string }
    | { type: "arrow"; from: string; to: string; label: string }
    | { type: "sketch"; strokes: number[][]; vw: number; vh: number }
    | { type: "swatch"; hex: string; label: string }
    | { type: "table"; rows: string[][] }
    | { type: "text"; text: string; size: "s" | "m" | "l" }
    | { type: "comment"; text: string }
  );

export type BlockType = Block["type"];

export interface Board {
  v: 1;
  id: string;
  title: string;
  /** The board this one sits on, or null for the root. */
  parent: string | null;
  icon?: string | null;
  blocks: Block[];
  /** Set on the root once cards and graph files from before boards were folded in. */
  migrated?: boolean;
}

export type Boards = Record<string, Board>;

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface View {
  x: number;
  y: number;
  zoom: number;
}

let counter = 0;
export function newId(prefix = "b"): string {
  counter += 1;
  return `${prefix}-${Date.now().toString(36)}${counter.toString(36)}${Math.random().toString(36).slice(2, 6)}`;
}

/** Default sizes by type; types that grow with their content only use `w`. */
export const DEFAULT_SIZE: Record<BlockType, { w: number; h: number }> = {
  doc: { w: 240, h: 150 },
  note: { w: 220, h: 120 },
  todo: { w: 240, h: 120 },
  column: { w: 270, h: 120 },
  board: { w: 220, h: 150 },
  image: { w: 280, h: 200 },
  file: { w: 240, h: 64 },
  link: { w: 260, h: 90 },
  arrow: { w: 0, h: 0 },
  sketch: { w: 280, h: 200 },
  swatch: { w: 120, h: 120 },
  table: { w: 320, h: 120 },
  text: { w: 260, h: 40 },
  comment: { w: 200, h: 60 },
};

/** Types whose height follows their content (measured from the screen). */
export const AUTO_HEIGHT: ReadonlySet<BlockType> = new Set(["note", "todo", "column", "link", "table", "text", "comment", "file"]);

export const BLOCK_LABELS: Record<BlockType, string> = {
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

export function makeBlock(type: BlockType, at: { x: number; y: number }, z: number, extra: Record<string, unknown> = {}): Block {
  const size = DEFAULT_SIZE[type];
  const base = { id: newId(), x: Math.round(at.x), y: Math.round(at.y), w: size.w, h: size.h, z };
  const data: Record<string, unknown> = (() => {
    switch (type) {
      case "doc":
        return { ref: "" };
      case "note":
        return { html: "", color: "yellow" };
      case "todo":
        return { title: "To-do", items: [{ id: newId("i"), text: "", done: false }] };
      case "column":
        return { title: "Column", children: [] };
      case "board":
        return { ref: "" };
      case "image":
        return { src: "", caption: "", ratio: 1.4 };
      case "file":
        return { src: "", name: "", size: 0 };
      case "link":
        return { url: "", title: "", description: "" };
      case "arrow":
        return { from: "", to: "", label: "" };
      case "sketch":
        return { strokes: [], vw: size.w, vh: size.h };
      case "swatch":
        return { hex: "#e8a33d", label: "" };
      case "table":
        return {
          rows: [
            ["", "", ""],
            ["", "", ""],
          ],
        };
      case "text":
        return { text: "", size: "m" };
      case "comment":
        return { text: "" };
    }
  })();
  return { ...base, type, ...data, ...extra } as Block;
}

export function emptyBoard(id: string, title: string, parent: string | null): Board {
  return { v: 1, id, title, parent, blocks: [] };
}

/** Parses a stored board; anything unusable comes back null. */
export function parseBoard(id: string, json: string): Board | null {
  try {
    const raw = JSON.parse(json) as Partial<Board>;
    if (!raw || typeof raw !== "object" || !Array.isArray(raw.blocks)) return null;
    const blocks = raw.blocks.filter(
      (b): b is Block => !!b && typeof b === "object" && typeof (b as Block).id === "string" && typeof (b as Block).type === "string" && (b as Block).type in DEFAULT_SIZE,
    );
    return {
      v: 1,
      id,
      title: typeof raw.title === "string" ? raw.title : "Untitled",
      parent: typeof raw.parent === "string" ? raw.parent : null,
      icon: typeof raw.icon === "string" ? raw.icon : null,
      blocks: blocks.map((b) => ({ ...b, x: num(b.x), y: num(b.y), w: num(b.w, 200), h: num(b.h, 100), z: num(b.z) })),
      migrated: raw.migrated === true ? true : undefined,
    };
  } catch {
    return null;
  }
}

function num(v: unknown, fallback = 0): number {
  return typeof v === "number" && Number.isFinite(v) ? v : fallback;
}

export function serializeBoard(b: Board): string {
  return JSON.stringify(b, null, 1);
}

export function topZ(board: Board): number {
  return board.blocks.reduce((m, b) => Math.max(m, b.z), 0) + 1;
}

export function bottomZ(board: Board): number {
  return board.blocks.reduce((m, b) => Math.min(m, b.z), 0) - 1;
}

export function unionRect(rects: Rect[]): Rect | null {
  if (!rects.length) return null;
  const x1 = Math.min(...rects.map((r) => r.x));
  const y1 = Math.min(...rects.map((r) => r.y));
  const x2 = Math.max(...rects.map((r) => r.x + r.w));
  const y2 = Math.max(...rects.map((r) => r.y + r.h));
  return { x: x1, y: y1, w: x2 - x1, h: y2 - y1 };
}

export function intersects(a: Rect, b: Rect): boolean {
  return a.x < b.x + b.w && a.x + a.w > b.x && a.y < b.y + b.h && a.y + a.h > b.y;
}

/** Where a line from `r`'s centre toward `to` leaves `r`. */
export function edgePoint(r: Rect, to: { x: number; y: number }): { x: number; y: number } {
  const cx = r.x + r.w / 2;
  const cy = r.y + r.h / 2;
  const dx = to.x - cx;
  const dy = to.y - cy;
  if (dx === 0 && dy === 0) return { x: cx, y: cy };
  const sx = dx === 0 ? Infinity : r.w / 2 / Math.abs(dx);
  const sy = dy === 0 ? Infinity : r.h / 2 / Math.abs(dy);
  const s = Math.min(sx, sy);
  return { x: cx + dx * s, y: cy + dy * s };
}

/** Every block id a block drags along: itself, and a column's children. */
export function withDescendants(board: Board, ids: Iterable<string>): Set<string> {
  const out = new Set<string>(ids);
  for (const b of board.blocks) {
    if (b.type === "column" && out.has(b.id)) for (const c of b.children) out.add(c);
  }
  return out;
}

const PLAIN_TAGS = new Set(["B", "STRONG", "I", "EM", "U", "S", "STRIKE", "BR", "DIV", "P", "UL", "OL", "LI", "SPAN"]);

/** Note text is stored as HTML, so anything read from a project file is
 * reduced to a few harmless formatting tags before it is shown. */
export function sanitizeHtml(html: string): string {
  const doc = new DOMParser().parseFromString(`<body>${html}</body>`, "text/html");
  const walk = (node: Node): string => {
    let out = "";
    node.childNodes.forEach((child) => {
      if (child.nodeType === Node.TEXT_NODE) {
        out += (child.textContent ?? "").replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
      } else if (child.nodeType === Node.ELEMENT_NODE) {
        const el = child as Element;
        const tag = el.tagName;
        if (!PLAIN_TAGS.has(tag)) out += walk(el);
        else if (tag === "BR") out += "<br>";
        else {
          const t = tag.toLowerCase();
          out += `<${t}>${walk(el)}</${t}>`;
        }
      }
    });
    return out;
  };
  return walk(doc.body);
}

export function htmlToText(html: string): string {
  const doc = new DOMParser().parseFromString(`<body>${html}</body>`, "text/html");
  return (doc.body.innerText || doc.body.textContent || "").trim();
}

/** The searchable text of a block. */
export function blockText(b: Block, docTitle?: (ref: string) => string): string {
  switch (b.type) {
    case "doc":
      return docTitle?.(b.ref) ?? b.ref;
    case "note":
      return htmlToText(b.html);
    case "todo":
      return [b.title, ...b.items.map((i) => i.text)].join(" ");
    case "column":
      return b.title;
    case "image":
      return b.caption;
    case "file":
      return b.name;
    case "link":
      return [b.title, b.url, b.description].join(" ");
    case "arrow":
      return b.label;
    case "swatch":
      return `${b.label} ${b.hex}`;
    case "table":
      return b.rows.flat().join(" ");
    case "text":
      return b.text;
    case "comment":
      return b.text;
    default:
      return "";
  }
}
