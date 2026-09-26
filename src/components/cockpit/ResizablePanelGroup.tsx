import { Fragment, useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { debounce } from "@/lib/debounce";
import { cn } from "@/lib/utils";

// Same settle window TerminalPanel's own resize handler uses, and for the
// same reason: this group's container only resizes because something
// ANCESTRAL changed size (a window resize, the Sidebar's width animation) —
// never because of this component's own resize-handle drags, which update
// `layout.sizes` directly and never touch `containerWidth`. An animated
// ancestor fires this `ResizeObserver` on every intermediate frame, and
// every firing was a full re-render of every panel in the group; debouncing
// it costs nothing a user could notice (a live window drag still tracks,
// just ~1 frame's worth behind) while collapsing dozens of re-renders from
// one sidebar toggle down to two.
const CONTAINER_RESIZE_SETTLE_MS = 120;

export interface PanelSpec {
  /** Stable identity — used as the React key, the persistence key inside
   * this group's saved layout, and to look panels back up after a
   * reorder. Must be unique within one `panels` array. */
  id: string;
  /** Floor for this panel's share of the row, in percent. Falls back to
   * `DEFAULT_MIN_PERCENT` when omitted. */
  minPercent?: number;
  /** Starting share of the row, in percent, before the user has ever
   * resized this group (and after — see `loadLayout`). Panels that omit
   * this fall back to an equal split; give every panel one on purpose
   * when the natural default isn't an even split (e.g. a narrow list next
   * to a wide inspector). */
  defaultPercent?: number;
  content: ReactNode;
}

interface ResizablePanelGroupProps {
  /** Persists this group's sizes and order to `localStorage` under
   * `infinabox.layout.<storageKey>` — unique per call site (e.g. "build",
   * "filebrowser.documents") so unrelated panel groups never collide. */
  storageKey: string;
  panels: PanelSpec[];
  className?: string;
}

const DEFAULT_MIN_PERCENT = 15;
// Visual gap between blocks, in pixels. Can't be a CSS `gap` alongside
// percentage flex-basis columns — this rendering engine resolves
// percentage flex-basis against the row's FULL width and adds gap on top
// rather than subtracting it first, so N 100%-summing columns plus any gap
// overflow the row by exactly the gap's width. Instead every column's
// flex-basis is a real pixel width computed against (row width - total
// gap), tracked via ResizeObserver so columns still reflow on window
// resize the way percentage flex-basis would for free.
const GAP = 8;
// How far (as a fraction of the neighboring panel's own width) the pointer
// has to cross into that neighbor before a drag-to-reorder swaps them —
// past the halfway point reads as "further into that panel than out of
// mine," which is where a swap starts feeling expected rather than early
// or sticky.
const SWAP_THRESHOLD_RATIO = 0.5;
// Height of the reorder grip, in pixels — see where it's rendered for why
// it has to stay within a header's top 4px.
const GRIP_HEIGHT = 4;

interface LayoutState {
  order: string[];
  sizes: Record<string, number>;
}

function storageFullKey(storageKey: string): string {
  return `infinabox.layout.${storageKey}`;
}

function defaultLayout(panels: PanelSpec[]): LayoutState {
  const order = panels.map((p) => p.id);
  const withDefaults = panels.every((p) => typeof p.defaultPercent === "number");
  let sizes: Record<string, number>;
  if (withDefaults) {
    const total = panels.reduce((sum, p) => sum + (p.defaultPercent ?? 0), 0) || 1;
    sizes = Object.fromEntries(panels.map((p) => [p.id, ((p.defaultPercent ?? 0) / total) * 100]));
  } else {
    const equal = 100 / panels.length;
    sizes = Object.fromEntries(order.map((id) => [id, equal]));
  }
  return { order, sizes };
}

function loadLayout(storageKey: string, panels: PanelSpec[]): LayoutState {
  const fallback = defaultLayout(panels);
  try {
    const raw = localStorage.getItem(storageFullKey(storageKey));
    if (!raw) return fallback;
    const parsed = JSON.parse(raw) as Partial<LayoutState> | null;
    const ids = panels.map((p) => p.id);
    const order = parsed?.order;
    const sizes = parsed?.sizes;
    const valid =
      Array.isArray(order) &&
      order.length === ids.length &&
      ids.every((id) => order.includes(id)) &&
      typeof sizes === "object" &&
      sizes !== null &&
      ids.every((id) => typeof sizes[id] === "number");
    // A mismatched panel set (a saved layout from before a panel was
    // added/removed here) falls back to the default rather than rendering
    // a stale or missing panel.
    return valid ? { order: order as string[], sizes: sizes as Record<string, number> } : fallback;
  } catch {
    return fallback;
  }
}

function saveLayout(storageKey: string, layout: LayoutState): void {
  try {
    localStorage.setItem(storageFullKey(storageKey), JSON.stringify(layout));
  } catch {
    // Losing a saved layout just means it resets to the default split next
    // launch — not worth surfacing as an error.
  }
}

/** The one system every multi-block row in the app uses to let its blocks
 * be resized against each other and dragged into a different order —
 * everything except the Sidebar, which has its own fixed position and its
 * own (unrelated) collapse behavior. A single call site with N panels: N-1
 * drag handles for resizing, and a small grip strip on each panel for
 * reordering. Both the sizes and the order persist per `storageKey`, the
 * same `localStorage`-backed pattern the Sidebar's collapsed state uses. */
export function ResizablePanelGroup({ storageKey, panels, className }: ResizablePanelGroupProps) {
  const byId = useMemo(() => Object.fromEntries(panels.map((p) => [p.id, p])), [panels]);
  const idsKey = panels.map((p) => p.id).join("\u0000");

  const [layout, setLayout] = useState<LayoutState>(() => loadLayout(storageKey, panels));

  useEffect(() => {
    setLayout((prev) => {
      const ids = panels.map((p) => p.id);
      const valid = prev.order.length === ids.length && ids.every((id) => prev.order.includes(id));
      return valid ? prev : loadLayout(storageKey, panels);
    });
    // Re-validate only when the panel id SET actually changes (`idsKey`) —
    // `panels` is a fresh array/object from the caller on every render, so
    // depending on it directly would re-run (and no-op) constantly.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [storageKey, idsKey]);

  useEffect(() => {
    saveLayout(storageKey, layout);
  }, [storageKey, layout]);

  const containerRef = useRef<HTMLDivElement | null>(null);
  const [containerWidth, setContainerWidth] = useState(0);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const debouncedSetWidth = debounce(setContainerWidth, CONTAINER_RESIZE_SETTLE_MS, { leading: true });
    const observer = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (entry) debouncedSetWidth(entry.contentRect.width);
    });
    observer.observe(el);
    return () => {
      debouncedSetWidth.cancel();
      observer.disconnect();
    };
  }, []);

  const gapTotal = GAP * Math.max(panels.length - 1, 0);
  const availableWidth = Math.max(containerWidth - gapTotal, 0);

  // ---- Resize: a handle only ever touches its own two immediate
  // neighbors — the two percentages it holds always trade space with each
  // other, so every panel's share keeps summing to exactly 100 no matter
  // which handle moved. ----
  const resizeDragRef = useRef<{
    leftId: string;
    rightId: string;
    startX: number;
    startLeftPercent: number;
    startRightPercent: number;
  } | null>(null);

  const onResizePointerMove = useCallback(
    (e: PointerEvent) => {
      const drag = resizeDragRef.current;
      const container = containerRef.current;
      if (!drag || !container) return;
      // Defensive: if the primary button isn't held anymore, we missed the
      // real pointerup (e.g. it was released outside the webview window, or
      // over native OS chrome that never dispatches it back to us) — treat
      // this move as the end of the drag instead of leaving it "stuck"
      // (cursor pinned to col-resize, and a later stray mousemove with no
      // button down still resizing the panels).
      if ((e.buttons & 1) === 0) {
        onResizePointerUp();
        return;
      }
      const width = container.getBoundingClientRect().width - gapTotal;
      if (width <= 0) return;
      const deltaPercent = ((e.clientX - drag.startX) / width) * 100;
      const leftMin = byId[drag.leftId]?.minPercent ?? DEFAULT_MIN_PERCENT;
      const rightMin = byId[drag.rightId]?.minPercent ?? DEFAULT_MIN_PERCENT;
      const bounded = Math.min(
        Math.max(deltaPercent, leftMin - drag.startLeftPercent),
        drag.startRightPercent - rightMin,
      );
      setLayout((prev) => ({
        ...prev,
        sizes: {
          ...prev.sizes,
          [drag.leftId]: drag.startLeftPercent + bounded,
          [drag.rightId]: drag.startRightPercent - bounded,
        },
      }));
    },
    // onResizePointerUp is intentionally left out of the deps array here —
    // it's stable enough for this purpose and including it would require
    // hoisting these two above each other in a way that fights their
    // mutual reference; see the eslint-disable below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [byId, gapTotal],
  );

  const onResizePointerUp = useCallback(() => {
    resizeDragRef.current = null;
    window.removeEventListener("pointermove", onResizePointerMove);
    window.removeEventListener("pointerup", onResizePointerUp);
    window.removeEventListener("pointercancel", onResizePointerUp);
    document.body.style.removeProperty("cursor");
    document.body.style.removeProperty("user-select");
  }, [onResizePointerMove]);

  const startResize = useCallback(
    (leftId: string, rightId: string) => (e: React.PointerEvent) => {
      resizeDragRef.current = {
        leftId,
        rightId,
        startX: e.clientX,
        startLeftPercent: layout.sizes[leftId] ?? DEFAULT_MIN_PERCENT,
        startRightPercent: layout.sizes[rightId] ?? DEFAULT_MIN_PERCENT,
      };
      window.addEventListener("pointermove", onResizePointerMove);
      window.addEventListener("pointerup", onResizePointerUp);
      // A native drag-and-drop, a context menu, or the OS taking the
      // gesture away from us (e.g. a system gesture at the window edge)
      // fires `pointercancel` instead of `pointerup` — without this, the
      // drag ref and the `col-resize` cursor/user-select lockout would
      // never clear.
      window.addEventListener("pointercancel", onResizePointerUp);
      document.body.style.cursor = "col-resize";
      document.body.style.userSelect = "none";
    },
    [layout.sizes, onResizePointerMove, onResizePointerUp],
  );

  // ---- Reorder: an adjacent swap each time the pointer crosses far
  // enough into a neighbor, rather than computing an arbitrary drop
  // index — simpler to get right, and plenty for the 2-4 panels any one
  // row actually has. Sizes stay keyed by panel id, so a panel keeps its
  // own width when it moves. ----
  const reorderDragRef = useRef<{ id: string; startX: number } | null>(null);

  const onReorderPointerMove = useCallback(
    (e: PointerEvent) => {
      const drag = reorderDragRef.current;
      if (!drag) return;
      // Same defensive cleanup as the resize handler — see its comment.
      if ((e.buttons & 1) === 0) {
        onReorderPointerUp();
        return;
      }
      setLayout((prev) => {
        let order = prev.order;
        let index = order.indexOf(drag.id);
        if (index === -1) return prev;
        // Consume the full pointer delta in one pass rather than a single
        // swap, so a large, fast motion that crosses more than one
        // neighbor's threshold in a single pointermove event (dragging a
        // panel past both of its neighbors in one motion, or a low
        // pointer-event sample rate) still lands the panel where the
        // cursor actually is instead of requiring extra mousemoves for the
        // order to "catch up".
        let remaining = e.clientX - drag.startX;
        let swapped = false;
        while (remaining > 0 && index < order.length - 1) {
          const neighborId = order[index + 1];
          const neighborWidthPx = ((prev.sizes[neighborId] ?? 0) / 100) * availableWidth;
          if (remaining <= neighborWidthPx * SWAP_THRESHOLD_RATIO || neighborWidthPx <= 0) break;
          const nextOrder = [...order];
          [nextOrder[index], nextOrder[index + 1]] = [nextOrder[index + 1], nextOrder[index]];
          order = nextOrder;
          index += 1;
          remaining -= neighborWidthPx;
          swapped = true;
        }
        while (remaining < 0 && index > 0) {
          const neighborId = order[index - 1];
          const neighborWidthPx = ((prev.sizes[neighborId] ?? 0) / 100) * availableWidth;
          if (-remaining <= neighborWidthPx * SWAP_THRESHOLD_RATIO || neighborWidthPx <= 0) break;
          const nextOrder = [...order];
          [nextOrder[index], nextOrder[index - 1]] = [nextOrder[index - 1], nextOrder[index]];
          order = nextOrder;
          index -= 1;
          remaining += neighborWidthPx;
          swapped = true;
        }
        if (!swapped) return prev;
        reorderDragRef.current = { id: drag.id, startX: e.clientX };
        return { ...prev, order };
      });
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [availableWidth],
  );

  const onReorderPointerUp = useCallback(() => {
    reorderDragRef.current = null;
    window.removeEventListener("pointermove", onReorderPointerMove);
    window.removeEventListener("pointerup", onReorderPointerUp);
    window.removeEventListener("pointercancel", onReorderPointerUp);
    document.body.style.removeProperty("cursor");
    document.body.style.removeProperty("user-select");
  }, [onReorderPointerMove]);

  const startReorder = useCallback(
    (id: string) => (e: React.PointerEvent) => {
      reorderDragRef.current = { id, startX: e.clientX };
      window.addEventListener("pointermove", onReorderPointerMove);
      window.addEventListener("pointerup", onReorderPointerUp);
      window.addEventListener("pointercancel", onReorderPointerUp);
      document.body.style.cursor = "grabbing";
      document.body.style.userSelect = "none";
    },
    [onReorderPointerMove, onReorderPointerUp],
  );

  // If this whole group unmounts mid-drag (e.g. the user switches away from
  // a non-persistent section like Business/Marketing while a resize/reorder
  // is in progress — that section's FileBrowser really does unmount, unlike
  // the Build/Design/Graphs tabs which just go invisible), nothing else
  // would ever remove these `window` listeners: they're added imperatively
  // on pointerdown, not tied to any effect, so without this they'd leak for
  // the rest of the app's life and keep firing into stale closures.
  useEffect(() => {
    return () => {
      if (resizeDragRef.current) onResizePointerUp();
      if (reorderDragRef.current) onReorderPointerUp();
    };
    // Runs its cleanup only on unmount — the refs are read fresh at that
    // point, so they don't need to be dependencies.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  if (panels.length === 0) return null;

  // A single panel has nothing to resize or reorder against — skip the
  // grip/handle chrome entirely rather than showing controls that would
  // do nothing.
  if (panels.length === 1) {
    return (
      <div className={cn("flex h-full min-h-0 w-full min-w-0 flex-1 overflow-hidden", className)}>
        {panels[0].content}
      </div>
    );
  }

  return (
    <div
      ref={containerRef}
      className={cn("flex h-full min-h-0 w-full min-w-0 flex-1 overflow-hidden", className)}
    >
      {layout.order.map((id, i) => {
        const panel = byId[id];
        if (!panel) return null;
        const widthPx = ((layout.sizes[id] ?? 0) / 100) * availableWidth;
        const isLast = i === layout.order.length - 1;
        return (
          <Fragment key={id}>
            <div className="group relative flex h-full min-w-0 flex-col" style={{ flex: `0 0 ${widthPx}px` }}>
              <div className="h-full min-h-0 flex-1 overflow-hidden">{panel.content}</div>
              {/* The reorder grip floats over the panel's own top edge
               * instead of taking a layout row of its own — blocks stay
               * exactly as tall as they'd be without this system, and the
               * grip only appears (and only accepts pointer input) while
               * hovering the block. It's a thin bar inside the top
               * `GRIP_HEIGHT` pixels, above where any header's controls
               * start: headers are `h-9` rows with vertically centered
               * controls at most `h-7` tall, so their top edge sits 4px
               * down. A taller, centered pill used to cover whatever a
               * narrow header had in the middle (Studio's Play "Restart",
               * Chat's "Conversations" picker). Deliberately NOT
               * `data-tauri-drag-region`: that would hand the pointer-down
               * to the OS window-drag handler before this component's own
               * pointermove/pointerup ever sees it. */}
              <div className="pointer-events-none absolute inset-x-0 top-0 z-20 flex justify-center">
                <div
                  data-testid="panel-reorder-grip"
                  title="Drag to move this panel"
                  onPointerDown={startReorder(id)}
                  style={{ height: GRIP_HEIGHT }}
                  className="pointer-events-none w-16 cursor-grab rounded-b-full bg-muted-foreground/50 opacity-0 transition-opacity duration-150 group-hover:pointer-events-auto group-hover:opacity-100 hover:bg-muted-foreground active:cursor-grabbing"
                />
              </div>
              {!isLast && <ResizeHandle onPointerDown={startResize(id, layout.order[i + 1])} />}
            </div>
            {!isLast && <div style={{ flex: `0 0 ${GAP}px` }} />}
          </Fragment>
        );
      })}
    </div>
  );
}

// An invisible hit-area straddling this column's right edge, wide enough to
// cover the visual GAP between blocks (and a little of each block's own
// rounded border on either side) without needing any layout width of its
// own — it's an absolutely-positioned overlay, not a flex sibling.
//
// No `pointer-events-auto` here on purpose: this group can live inside one
// of App.tsx's persistent-tab wrappers (Documents/Graphs's inner list-vs-
// inspector split), which sets a real `pointer-events: none` on the whole
// subtree while that tab isn't active. An explicit `pointer-events-auto`
// here would override that and leave a hidden tab's resize handle
// draggable through the invisible overlay. Plain CSS default (`auto`) is
// exactly what this needs while the group IS visible, so omitting the
// class changes nothing for the active-tab case.
function ResizeHandle({ onPointerDown }: { onPointerDown: (e: React.PointerEvent) => void }) {
  return (
    <div
      onPointerDown={onPointerDown}
      className="absolute inset-y-0 left-full z-10 w-4 -translate-x-1/2 cursor-col-resize touch-none"
    />
  );
}
