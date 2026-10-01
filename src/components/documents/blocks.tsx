import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import {
  Archive, ArrowUpRight, Check, FileAudio, FileCode, FileImage, FileText, FileVideo, File as FileIcon, GripVertical, Layers, MessageCircle, Plus, X,
} from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { cn } from "@/lib/utils";
import { NoteIcon } from "@/components/context/note-icons";
import type { CardSummary } from "@/lib/studio-types";
import { useDocPreview, useImageSrc } from "./media";
import { formatSize, hostOf, isWebUrl, normalizeUrl } from "./paths";
import { newId, sanitizeHtml, type Block, type Boards, type NoteColor } from "./types";

// What each kind of block looks like. The canvas wraps these in a positioned
// shell that handles selecting, dragging and resizing; here is only content.

export interface BlockCtx {
  projectPath: string;
  tick: number;
  boards: Boards;
  cards: Map<string, CardSummary>;
  editingId: string | null;
  update: (id: string, patch: Record<string, unknown>) => void;
  startEditing: (id: string) => void;
  stopEditing: () => void;
  openDoc: (path: string) => void;
  openBoard: (id: string) => void;
  renameBoard: (id: string, title: string) => void;
  renderChild: (id: string) => ReactNode;
}

export const NOTE_BG: Record<NoteColor, string> = {
  yellow: "#fde68a",
  pink: "#fbcfe8",
  blue: "#bfdbfe",
  green: "#bbf7d0",
  purple: "#ddd6fe",
  gray: "#e5e7eb",
};

const NOTE_INK = "#1f2937";

const field =
  "min-w-0 rounded bg-transparent px-1 py-0.5 outline-none placeholder:text-muted-foreground/60 focus:bg-foreground/5 focus:ring-1 focus:ring-ring/50";

function put<T extends Block["type"]>(ctx: BlockCtx, b: Extract<Block, { type: T }>, patch: Partial<Extract<Block, { type: T }>>) {
  ctx.update(b.id, patch as Record<string, unknown>);
}

export function BlockContent({ block, ctx }: { block: Block; ctx: BlockCtx }) {
  switch (block.type) {
    case "doc":
      return <DocContent b={block} ctx={ctx} />;
    case "note":
      return <NoteContent b={block} ctx={ctx} />;
    case "todo":
      return <TodoContent b={block} ctx={ctx} />;
    case "column":
      return <ColumnContent b={block} ctx={ctx} />;
    case "board":
      return <BoardContent b={block} ctx={ctx} />;
    case "image":
      return <ImageContent b={block} ctx={ctx} />;
    case "file":
      return <FileContent b={block} ctx={ctx} />;
    case "link":
      return <LinkContent b={block} ctx={ctx} />;
    case "sketch":
      return <SketchContent b={block} ctx={ctx} />;
    case "swatch":
      return <SwatchContent b={block} ctx={ctx} />;
    case "table":
      return <TableContent b={block} ctx={ctx} />;
    case "text":
      return <TextContent b={block} ctx={ctx} />;
    case "comment":
      return <CommentContent b={block} ctx={ctx} />;
    default:
      return null;
  }
}

// ---- editable text ----

/** Text that is typed in place. `html` keeps bold/italic; otherwise plain. */
function Editable({
  value,
  html,
  editing,
  placeholder,
  className,
  style,
  onChange,
  onDone,
}: {
  value: string;
  html?: boolean;
  editing: boolean;
  placeholder: string;
  className?: string;
  style?: React.CSSProperties;
  onChange: (value: string) => void;
  onDone: () => void;
}) {
  const ref = useRef<HTMLDivElement | null>(null);
  const read = (el: HTMLElement) => (html ? sanitizeHtml(el.innerHTML) : el.innerText.replace(/\n$/, ""));
  // Set the content when it changes from outside, never while it is being typed.
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el || document.activeElement === el) return;
    if (html) {
      const clean = sanitizeHtml(value);
      if (el.innerHTML !== clean) el.innerHTML = clean;
    } else if (el.innerText !== value) el.innerText = value;
  }, [value, html, editing]);
  useEffect(() => {
    const el = ref.current;
    if (!editing || !el) return;
    el.focus();
    const range = document.createRange();
    range.selectNodeContents(el);
    range.collapse(false);
    const sel = window.getSelection();
    sel?.removeAllRanges();
    sel?.addRange(range);
  }, [editing]);
  const empty = html ? !value.replace(/<[^>]*>/g, "").trim() : !value;
  return (
    <div className="relative">
      {empty && !editing && <span className="pointer-events-none absolute inset-0 opacity-50">{placeholder}</span>}
      <div
        ref={ref}
        contentEditable={editing ? (html ? true : ("plaintext-only" as unknown as boolean)) : false}
        suppressContentEditableWarning
        spellCheck={false}
        data-editable={editing ? "" : undefined}
        className={cn("min-h-[1.25em] whitespace-pre-wrap break-words outline-none", editing && "cursor-text select-text", className)}
        style={style}
        onInput={(e) => onChange(read(e.currentTarget))}
        onBlur={(e) => {
          onChange(read(e.currentTarget));
          onDone();
        }}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.preventDefault();
            e.currentTarget.blur();
          }
        }}
      />
    </div>
  );
}

// ---- document ----

function DocContent({ b, ctx }: { b: Extract<Block, { type: "doc" }>; ctx: BlockCtx }) {
  const card = ctx.cards.get(b.ref);
  const preview = useDocPreview(ctx.projectPath, b.ref, ctx.tick);
  if (!card) {
    return (
      <div className="flex h-full flex-col justify-center gap-1 rounded-lg border border-dashed border-border bg-card p-3 text-xs text-muted-foreground">
        <span className="font-medium text-foreground/80">Document not found</span>
        <span className="truncate">{b.ref}</span>
      </div>
    );
  }
  const icon = card.icon;
  return (
    <div className="flex h-full flex-col overflow-hidden rounded-lg border border-border bg-card shadow-sm">
      <div className="flex items-center gap-2 border-b border-border/60 px-3 py-2">
        {icon ? <NoteIcon value={icon} className="size-4 text-sm" /> : <FileText className="size-4 shrink-0 text-muted-foreground" />}
        <span className="min-w-0 flex-1 truncate text-sm font-medium">{card.title}</span>
        <button
          type="button"
          data-no-drag
          aria-label="Open document"
          title="Open"
          data-testid="doc-open"
          onClick={() => ctx.openDoc(b.ref)}
          className="flex size-5 shrink-0 items-center justify-center rounded text-muted-foreground hover:bg-foreground/10 hover:text-foreground"
        >
          <ArrowUpRight className="size-3.5" />
        </button>
      </div>
      <p className="min-h-0 flex-1 overflow-hidden whitespace-pre-line px-3 py-2 text-xs leading-relaxed text-muted-foreground">
        {preview ?? ""}
      </p>
    </div>
  );
}

// ---- note ----

function NoteContent({ b, ctx }: { b: Extract<Block, { type: "note" }>; ctx: BlockCtx }) {
  const editing = ctx.editingId === b.id;
  return (
    <div
      className="min-h-[72px] rounded-md p-3 text-sm leading-snug shadow-sm"
      style={{ background: NOTE_BG[b.color] ?? NOTE_BG.yellow, color: NOTE_INK }}
    >
      <Editable
        html
        value={b.html}
        editing={editing}
        placeholder="Note"
        onChange={(html) => put(ctx, b, { html })}
        onDone={ctx.stopEditing}
      />
    </div>
  );
}

// ---- to-do list ----

function TodoContent({ b, ctx }: { b: Extract<Block, { type: "todo" }>; ctx: BlockCtx }) {
  const [dragging, setDragging] = useState<string | null>(null);
  const [over, setOver] = useState<string | null>(null);
  const done = b.items.filter((i) => i.done).length;
  const setItems = (items: typeof b.items) => put(ctx, b, { items });
  const move = (from: string, to: string) => {
    if (from === to) return;
    const items = [...b.items];
    const a = items.findIndex((i) => i.id === from);
    const z = items.findIndex((i) => i.id === to);
    if (a < 0 || z < 0) return;
    const [item] = items.splice(a, 1);
    items.splice(z, 0, item!);
    setItems(items);
  };
  return (
    <div className="rounded-lg border border-border bg-card p-2 shadow-sm">
      <div className="flex items-center gap-2 px-1 pb-1">
        <input
          value={b.title}
          placeholder="To-do"
          onChange={(e) => put(ctx, b, { title: e.target.value })}
          className={cn(field, "flex-1 text-sm font-medium")}
        />
        {b.items.length > 0 && (
          <span className="text-[11px] tabular-nums text-muted-foreground">
            {done}/{b.items.length}
          </span>
        )}
      </div>
      <ul className="flex flex-col">
        {b.items.map((item, i) => (
          <li
            key={item.id}
            draggable
            onDragStart={(e) => {
              if (!(e.target as HTMLElement).closest("[data-handle]")) return e.preventDefault();
              e.dataTransfer.effectAllowed = "move";
              setDragging(item.id);
            }}
            onDragOver={(e) => {
              if (!dragging) return;
              e.preventDefault();
              e.stopPropagation();
              setOver(item.id);
            }}
            onDrop={(e) => {
              if (!dragging) return;
              e.preventDefault();
              e.stopPropagation();
              move(dragging, item.id);
              setDragging(null);
              setOver(null);
            }}
            onDragEnd={() => {
              setDragging(null);
              setOver(null);
            }}
            className={cn("group flex items-center gap-1 rounded px-0.5", over === item.id && dragging !== item.id && "bg-foreground/10")}
          >
            <span data-handle data-no-drag className="flex size-4 shrink-0 cursor-grab items-center justify-center text-muted-foreground/0 group-hover:text-muted-foreground">
              <GripVertical className="size-3" />
            </span>
            <button
              type="button"
              data-no-drag
              role="checkbox"
              aria-checked={item.done}
              aria-label="Done"
              onClick={() => setItems(b.items.map((x) => (x.id === item.id ? { ...x, done: !x.done } : x)))}
              className={cn(
                "flex size-4 shrink-0 items-center justify-center rounded border",
                item.done ? "border-foreground bg-foreground text-background" : "border-muted-foreground/60",
              )}
            >
              {item.done && <Check className="size-3" />}
            </button>
            <input
              value={item.text}
              placeholder="Item"
              data-todo-item={item.id}
              onChange={(e) => setItems(b.items.map((x) => (x.id === item.id ? { ...x, text: e.target.value } : x)))}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  const id = newId("i");
                  const items = [...b.items];
                  items.splice(i + 1, 0, { id, text: "", done: false });
                  setItems(items);
                  requestAnimationFrame(() => document.querySelector<HTMLInputElement>(`[data-todo-item="${id}"]`)?.focus());
                } else if (e.key === "Backspace" && item.text === "" && b.items.length > 1) {
                  e.preventDefault();
                  setItems(b.items.filter((x) => x.id !== item.id));
                  const prev = b.items[i - 1] ?? b.items[i + 1];
                  if (prev) requestAnimationFrame(() => document.querySelector<HTMLInputElement>(`[data-todo-item="${prev.id}"]`)?.focus());
                }
              }}
              className={cn(field, "flex-1 text-sm", item.done && "text-muted-foreground line-through")}
            />
            <button
              type="button"
              data-no-drag
              aria-label="Remove item"
              onClick={() => setItems(b.items.filter((x) => x.id !== item.id))}
              className="flex size-4 shrink-0 items-center justify-center rounded text-muted-foreground/0 hover:bg-foreground/10 group-hover:text-muted-foreground"
            >
              <X className="size-3" />
            </button>
          </li>
        ))}
      </ul>
      <button
        type="button"
        data-no-drag
        onClick={() => {
          const id = newId("i");
          setItems([...b.items, { id, text: "", done: false }]);
          requestAnimationFrame(() => document.querySelector<HTMLInputElement>(`[data-todo-item="${id}"]`)?.focus());
        }}
        className="mt-1 flex items-center gap-1 rounded px-1.5 py-0.5 text-xs text-muted-foreground hover:bg-foreground/10 hover:text-foreground"
      >
        <Plus className="size-3" /> Item
      </button>
    </div>
  );
}

// ---- column ----

function ColumnContent({ b, ctx }: { b: Extract<Block, { type: "column" }>; ctx: BlockCtx }) {
  return (
    <div className="rounded-lg border border-border bg-muted/40 shadow-sm" data-column-id={b.id}>
      <div className="flex items-center gap-1.5 px-2 pt-2">
        <Layers className="size-3.5 shrink-0 text-muted-foreground" />
        <input
          value={b.title}
          placeholder="Column"
          onChange={(e) => put(ctx, b, { title: e.target.value })}
          className={cn(field, "flex-1 text-sm font-medium")}
        />
      </div>
      <div data-column-body={b.id} className="flex min-h-[56px] flex-col gap-2 p-2">
        {b.children.map((id) => ctx.renderChild(id))}
        {b.children.length === 0 && (
          <div className="flex min-h-[40px] flex-1 items-center justify-center rounded-md border border-dashed border-border text-xs text-muted-foreground">
            Drop here
          </div>
        )}
      </div>
    </div>
  );
}

// ---- board ----

const THUMB_FILL: Record<string, string> = {
  doc: "#94a3b8",
  note: "#fbbf24",
  todo: "#34d399",
  column: "#a1a1aa",
  board: "#60a5fa",
  image: "#f472b6",
  file: "#a78bfa",
  link: "#38bdf8",
  sketch: "#fb923c",
  swatch: "#f87171",
  table: "#2dd4bf",
  text: "#d4d4d8",
  comment: "#fde047",
};

function BoardContent({ b, ctx }: { b: Extract<Block, { type: "board" }>; ctx: BlockCtx }) {
  const child = ctx.boards[b.ref];
  const items = (child?.blocks ?? []).filter((x) => x.type !== "arrow" && !x.col);
  const xs = items.map((i) => i.x);
  const ys = items.map((i) => i.y);
  const x1 = Math.min(...xs, 0);
  const y1 = Math.min(...ys, 0);
  const x2 = Math.max(...items.map((i) => i.x + i.w), x1 + 1);
  const y2 = Math.max(...items.map((i) => i.y + i.h), y1 + 1);
  const pad = Math.max(x2 - x1, y2 - y1) * 0.05;
  return (
    <div className="flex h-full flex-col overflow-hidden rounded-lg border border-border bg-card shadow-sm">
      <div className="relative min-h-0 flex-1 bg-muted/50">
        {items.length > 0 && (
          <svg
            viewBox={`${x1 - pad} ${y1 - pad} ${x2 - x1 + pad * 2} ${y2 - y1 + pad * 2}`}
            preserveAspectRatio="xMidYMid meet"
            className="absolute inset-0 size-full"
            aria-hidden
          >
            {items.map((i) => (
              <rect key={i.id} x={i.x} y={i.y} width={i.w} height={Math.max(i.h, 16)} rx={6} fill={THUMB_FILL[i.type] ?? "#a1a1aa"} opacity={0.85} />
            ))}
          </svg>
        )}
      </div>
      <div className="flex items-center gap-1.5 border-t border-border/60 px-2.5 py-1.5">
        {child?.icon ? <NoteIcon value={child.icon} className="size-3.5 text-xs" /> : <Layers className="size-3.5 shrink-0 text-muted-foreground" />}
        <input
          value={child?.title ?? ""}
          placeholder="Board"
          data-testid="board-title"
          onChange={(e) => ctx.renameBoard(b.ref, e.target.value)}
          className={cn(field, "flex-1 text-sm font-medium")}
        />
        <button
          type="button"
          data-no-drag
          aria-label="Open board"
          title="Open"
          onClick={() => ctx.openBoard(b.ref)}
          className="flex size-5 shrink-0 items-center justify-center rounded text-muted-foreground hover:bg-foreground/10 hover:text-foreground"
        >
          <ArrowUpRight className="size-3.5" />
        </button>
      </div>
    </div>
  );
}

// ---- image ----

function ImageContent({ b, ctx }: { b: Extract<Block, { type: "image" }>; ctx: BlockCtx }) {
  const url = useImageSrc(ctx.projectPath, b.src);
  return (
    <div>
      <div className="overflow-hidden rounded-md bg-muted/50 shadow-sm" style={{ height: b.h }}>
        {url ? (
          <img src={url} alt={b.caption} draggable={false} className="size-full object-cover" />
        ) : (
          <div className="flex size-full items-center justify-center text-xs text-muted-foreground">{b.src ? "Loading…" : "No picture"}</div>
        )}
      </div>
      <input
        value={b.caption}
        placeholder="Caption"
        onChange={(e) => put(ctx, b, { caption: e.target.value })}
        className={cn(field, "mt-1 w-full text-center text-xs text-muted-foreground", !b.caption && "opacity-0 focus:opacity-100 group-hover/block:opacity-100")}
      />
    </div>
  );
}

// ---- file ----

function FileContent({ b, ctx }: { b: Extract<Block, { type: "file" }>; ctx: BlockCtx }) {
  const ext = b.name.split(".").pop()?.toLowerCase() ?? "";
  const Icon = /^(png|jpe?g|gif|webp|svg|bmp)$/.test(ext)
    ? FileImage
    : /^(mp3|wav|ogg|flac|m4a)$/.test(ext)
      ? FileAudio
      : /^(mp4|mov|webm|mkv)$/.test(ext)
        ? FileVideo
        : /^(zip|tar|gz|7z|rar)$/.test(ext)
          ? Archive
          : /^(gd|rs|ts|tsx|js|json|py|cs|tscn|tres|gdshader)$/.test(ext)
            ? FileCode
            : /^(md|txt|pdf|doc|docx)$/.test(ext)
              ? FileText
              : FileIcon;
  return (
    <div className="flex items-center gap-2.5 rounded-lg border border-border bg-card p-2.5 shadow-sm">
      <div className="flex size-9 shrink-0 items-center justify-center rounded-md bg-muted">
        <Icon className="size-5 text-muted-foreground" />
      </div>
      <div className="min-w-0 flex-1">
        <div className="truncate text-sm font-medium">{b.name}</div>
        <div className="text-[11px] text-muted-foreground">
          {ext ? ext.toUpperCase() : "File"} · {formatSize(b.size)}
        </div>
      </div>
      <button
        type="button"
        data-no-drag
        title="Show in folder"
        aria-label="Show in folder"
        onClick={() => void revealItemInDir(`${ctx.projectPath}/${b.src}`).catch(() => {})}
        className="flex size-6 shrink-0 items-center justify-center rounded text-muted-foreground hover:bg-foreground/10 hover:text-foreground"
      >
        <ArrowUpRight className="size-3.5" />
      </button>
    </div>
  );
}

// ---- link ----

function LinkContent({ b, ctx }: { b: Extract<Block, { type: "link" }>; ctx: BlockCtx }) {
  const editing = ctx.editingId === b.id || !b.url;
  const web = isWebUrl(b.url);
  return (
    <div className="rounded-lg border border-border bg-card p-3 shadow-sm">
      {editing ? (
        <div className="flex flex-col gap-1" onBlur={(e) => { if (!e.currentTarget.contains(e.relatedTarget as Node | null)) ctx.stopEditing(); }}>
          <input
            autoFocus
            value={b.url}
            placeholder="https://"
            data-testid="link-url"
            onChange={(e) => put(ctx, b, { url: e.target.value })}
            onBlur={(e) => {
              const fixed = normalizeUrl(e.target.value);
              if (fixed !== b.url) put(ctx, b, { url: fixed });
            }}
            onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLElement).blur()}
            className={cn(field, "text-sm")}
          />
          <input value={b.title} placeholder="Title" onChange={(e) => put(ctx, b, { title: e.target.value })} className={cn(field, "text-sm font-medium")} />
          <input value={b.description} placeholder="Description" onChange={(e) => put(ctx, b, { description: e.target.value })} className={cn(field, "text-xs")} />
        </div>
      ) : (
        <>
          <div className="line-clamp-2 text-sm font-medium">{b.title || hostOf(b.url)}</div>
          {b.description && <div className="mt-0.5 line-clamp-3 text-xs text-muted-foreground">{b.description}</div>}
          <button
            type="button"
            data-no-drag
            disabled={!web}
            onClick={() => web && void openUrl(b.url).catch(() => {})}
            className="mt-1.5 flex max-w-full items-center gap-1 truncate text-[11px] text-muted-foreground hover:text-foreground disabled:cursor-default"
          >
            <ArrowUpRight className="size-3 shrink-0" />
            <span className="truncate">{hostOf(b.url)}</span>
          </button>
        </>
      )}
    </div>
  );
}

// ---- sketch ----

function SketchContent({ b, ctx }: { b: Extract<Block, { type: "sketch" }>; ctx: BlockCtx }) {
  const editing = ctx.editingId === b.id;
  const live = useRef<number[] | null>(null);
  const [, redraw] = useState(0);
  const svg = useRef<SVGSVGElement | null>(null);
  const point = (e: React.PointerEvent) => {
    const r = svg.current!.getBoundingClientRect();
    return [Math.round(((e.clientX - r.left) / r.width) * b.vw * 10) / 10, Math.round(((e.clientY - r.top) / r.height) * b.vh * 10) / 10];
  };
  const d = (s: number[]) => {
    let out = `M${s[0]} ${s[1]}`;
    for (let i = 2; i < s.length; i += 2) out += ` L${s[i]} ${s[i + 1]}`;
    if (s.length === 2) out += ` L${s[0]! + 0.1} ${s[1]}`;
    return out;
  };
  return (
    <div className={cn("size-full overflow-hidden rounded-md border bg-card shadow-sm", editing ? "border-ring" : "border-border")}>
      <svg
        ref={svg}
        viewBox={`0 0 ${b.vw} ${b.vh}`}
        preserveAspectRatio="none"
        className={cn("size-full text-foreground", editing && "cursor-crosshair")}
        onPointerDown={(e) => {
          if (!editing || e.button !== 0) return;
          e.stopPropagation();
          e.currentTarget.setPointerCapture(e.pointerId);
          live.current = point(e);
          redraw((n) => n + 1);
        }}
        onPointerMove={(e) => {
          if (!live.current) return;
          const [x, y] = point(e);
          live.current = [...live.current, x!, y!];
          redraw((n) => n + 1);
        }}
        onPointerUp={() => {
          if (!live.current) return;
          const stroke = live.current;
          live.current = null;
          put(ctx, b, { strokes: [...b.strokes, stroke] });
        }}
      >
        {b.strokes.map((s, i) => (
          <path key={i} d={d(s)} fill="none" stroke="currentColor" strokeWidth={2.5} strokeLinecap="round" strokeLinejoin="round" vectorEffect="non-scaling-stroke" />
        ))}
        {live.current && <path d={d(live.current)} fill="none" stroke="currentColor" strokeWidth={2.5} strokeLinecap="round" strokeLinejoin="round" vectorEffect="non-scaling-stroke" />}
      </svg>
      {editing && (
        <div data-no-drag className="absolute right-1.5 top-1.5 flex gap-1">
          <button type="button" className="rounded bg-background/90 px-1.5 py-0.5 text-[11px] hover:bg-muted" onClick={() => put(ctx, b, { strokes: b.strokes.slice(0, -1) })}>
            Undo stroke
          </button>
          <button type="button" className="rounded bg-background/90 px-1.5 py-0.5 text-[11px] hover:bg-muted" onClick={ctx.stopEditing}>
            Done
          </button>
        </div>
      )}
    </div>
  );
}

// ---- colour swatch ----

function SwatchContent({ b, ctx }: { b: Extract<Block, { type: "swatch" }>; ctx: BlockCtx }) {
  const valid = /^#[0-9a-fA-F]{6}$/.test(b.hex);
  return (
    <div className="flex h-full flex-col overflow-hidden rounded-lg border border-border bg-card shadow-sm">
      <label className="relative min-h-0 flex-1 cursor-pointer" style={{ background: valid ? b.hex : "transparent" }} data-no-drag>
        <input
          type="color"
          value={valid ? b.hex : "#000000"}
          onChange={(e) => put(ctx, b, { hex: e.target.value })}
          aria-label="Pick a color"
          className="absolute inset-0 size-full cursor-pointer opacity-0"
        />
      </label>
      <div className="flex flex-col gap-0.5 p-1.5">
        <input
          value={b.hex}
          onChange={(e) => put(ctx, b, { hex: e.target.value })}
          spellCheck={false}
          className={cn(field, "font-mono text-xs uppercase", !valid && "text-destructive")}
        />
        <input value={b.label} placeholder="Name" onChange={(e) => put(ctx, b, { label: e.target.value })} className={cn(field, "text-xs")} />
      </div>
    </div>
  );
}

// ---- table ----

function TableContent({ b, ctx }: { b: Extract<Block, { type: "table" }>; ctx: BlockCtx }) {
  const cols = Math.max(1, ...b.rows.map((r) => r.length));
  const setRows = (rows: string[][]) => put(ctx, b, { rows });
  const setCell = (r: number, c: number, v: string) =>
    setRows(b.rows.map((row, ri) => (ri === r ? Array.from({ length: cols }, (_, ci) => (ci === c ? v : (row[ci] ?? ""))) : row)));
  const btn = "rounded px-1.5 py-0.5 text-[11px] text-muted-foreground hover:bg-foreground/10 hover:text-foreground disabled:opacity-40";
  return (
    <div className="group/table rounded-lg border border-border bg-card p-1.5 shadow-sm">
      <table className="w-full table-fixed border-collapse text-xs">
        <tbody>
          {b.rows.map((row, r) => (
            <tr key={r}>
              {Array.from({ length: cols }, (_, c) => (
                <td key={c} className="border border-border/70 p-0">
                  <input
                    value={row[c] ?? ""}
                    onChange={(e) => setCell(r, c, e.target.value)}
                    className={cn("w-full bg-transparent px-1.5 py-1 outline-none focus:bg-foreground/5", r === 0 && "font-medium")}
                  />
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
      <div data-no-drag className="mt-1 flex gap-0.5 opacity-0 transition-opacity group-hover/table:opacity-100">
        <button type="button" className={btn} onClick={() => setRows([...b.rows, Array.from({ length: cols }, () => "")])}>
          + Row
        </button>
        <button type="button" className={btn} onClick={() => setRows(b.rows.map((r) => [...r, ""]))}>
          + Column
        </button>
        <button type="button" className={btn} disabled={b.rows.length <= 1} onClick={() => setRows(b.rows.slice(0, -1))}>
          − Row
        </button>
        <button type="button" className={btn} disabled={cols <= 1} onClick={() => setRows(b.rows.map((r) => r.slice(0, cols - 1)))}>
          − Column
        </button>
      </div>
    </div>
  );
}

// ---- text label ----

const TEXT_SIZE = { s: "text-sm", m: "text-xl font-semibold", l: "text-4xl font-bold tracking-tight" } as const;

function TextContent({ b, ctx }: { b: Extract<Block, { type: "text" }>; ctx: BlockCtx }) {
  return (
    <Editable
      value={b.text}
      editing={ctx.editingId === b.id}
      placeholder="Text"
      className={cn("px-1", TEXT_SIZE[b.size] ?? TEXT_SIZE.m)}
      onChange={(text) => put(ctx, b, { text })}
      onDone={ctx.stopEditing}
    />
  );
}

// ---- comment ----

function CommentContent({ b, ctx }: { b: Extract<Block, { type: "comment" }>; ctx: BlockCtx }) {
  return (
    <div className="relative flex items-start gap-1.5 rounded-2xl rounded-bl-sm border border-amber-400/60 bg-amber-100 px-2.5 py-2 text-xs text-amber-950 shadow-sm">
      <MessageCircle className="mt-0.5 size-3.5 shrink-0 opacity-60" />
      <div className="min-w-0 flex-1">
        <Editable
          value={b.text}
          editing={ctx.editingId === b.id}
          placeholder="Comment"
          onChange={(text) => put(ctx, b, { text })}
          onDone={ctx.stopEditing}
        />
      </div>
    </div>
  );
}
