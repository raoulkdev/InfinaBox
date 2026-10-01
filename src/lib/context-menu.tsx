import { createContext, useCallback, useContext, useEffect, useLayoutEffect, useMemo, useRef, useState, type MouseEvent as ReactMouseEvent, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { cn } from "@/lib/utils";

// Right-click menus. The window never shows the browser's own menu: every
// right-click lands here. A page or panel gives its own entries with
// `useContextMenu()` (the innermost one wins); anywhere else the app-wide
// entries are shown, and a text field always gets Cut / Copy / Paste on top.

export interface MenuItem {
  label: string;
  onSelect: () => void | Promise<void>;
  icon?: ReactNode;
  shortcut?: string;
  disabled?: boolean;
  destructive?: boolean;
  testId?: string;
}
export type MenuEntry = MenuItem | "separator" | { heading: string };

type Show = (e: ReactMouseEvent | MouseEvent, items: MenuEntry[]) => void;
const Ctx = createContext<Show | null>(null);

const FIELD = 'input:not([type="checkbox"]):not([type="radio"]):not([type="color"]):not([type="range"]):not([type="file"]),textarea,[contenteditable="true"],[contenteditable="plaintext-only"]';

function fieldOf(target: EventTarget | null): HTMLElement | null {
  const el = target as HTMLElement | null;
  if (el?.closest?.(".xterm")) return null; // the terminal has its own menu
  return el?.closest?.(FIELD) ?? null;
}

function selectionText(field: HTMLElement | null): string {
  if (field instanceof HTMLInputElement || field instanceof HTMLTextAreaElement) {
    return (field.value ?? "").slice(field.selectionStart ?? 0, field.selectionEnd ?? 0);
  }
  return window.getSelection()?.toString() ?? "";
}

function editEntries(target: EventTarget | null): MenuEntry[] {
  const field = fieldOf(target);
  const selected = selectionText(field);
  const readOnly = field instanceof HTMLInputElement || field instanceof HTMLTextAreaElement ? field.readOnly || field.disabled : false;
  if (!field) {
    return selected ? [{ label: "Copy", shortcut: "⌘C", onSelect: () => void navigator.clipboard.writeText(selected).catch(() => {}) }] : [];
  }
  return [
    { label: "Cut", shortcut: "⌘X", disabled: !selected || readOnly, onSelect: () => void document.execCommand("cut") },
    { label: "Copy", shortcut: "⌘C", disabled: !selected, onSelect: () => void document.execCommand("copy") },
    {
      label: "Paste",
      shortcut: "⌘V",
      disabled: readOnly,
      onSelect: async () => {
        try {
          const text = await navigator.clipboard.readText();
          if (text) document.execCommand("insertText", false, text);
        } catch {
          // Clipboard unavailable: nothing to paste.
        }
      },
    },
    {
      label: "Select all",
      shortcut: "⌘A",
      onSelect: () => {
        if (field instanceof HTMLInputElement || field instanceof HTMLTextAreaElement) field.select();
        else {
          const range = document.createRange();
          range.selectNodeContents(field);
          const sel = window.getSelection();
          sel?.removeAllRanges();
          sel?.addRange(range);
        }
      },
    },
  ];
}

interface Open {
  x: number;
  y: number;
  items: MenuEntry[];
}

export function ContextMenuHost({ fallback, children }: { fallback: () => MenuEntry[]; children: ReactNode }) {
  const [open, setOpen] = useState<Open | null>(null);
  const fallbackRef = useRef(fallback);
  fallbackRef.current = fallback;

  const show = useCallback<Show>((e, items) => {
    e.preventDefault();
    if ("stopPropagation" in e) e.stopPropagation();
    const edit = editEntries(e.target);
    const merged: MenuEntry[] = edit.length && items.length ? [...edit, "separator", ...items] : [...edit, ...items];
    if (merged.length) setOpen({ x: e.clientX, y: e.clientY, items: merged });
    else setOpen(null);
  }, []);

  // Right-clicks nobody claimed: the app-wide entries.
  useEffect(() => {
    const onNative = (e: MouseEvent) => {
      if ((e.target as HTMLElement | null)?.closest?.("[data-native-menu]")) return;
      // Behind an open dialog, only text-field entries make sense.
      const modal = !!document.querySelector('[role="dialog"][data-state="open"],[role="alertdialog"][data-state="open"]');
      show(e, modal ? [] : fallbackRef.current());
    };
    document.addEventListener("contextmenu", onNative);
    return () => document.removeEventListener("contextmenu", onNative);
  }, [show]);

  const close = useCallback(() => setOpen(null), []);
  return (
    <Ctx.Provider value={show}>
      {children}
      {open && <MenuView open={open} onClose={close} />}
    </Ctx.Provider>
  );
}

/** `onContextMenu` for a panel: pass the entries to show, or nothing to leave it to the app-wide menu. */
export function useContextMenu(): Show {
  const show = useContext(Ctx);
  return useCallback<Show>(
    (e, items) => {
      show?.(e, items);
    },
    [show],
  );
}

function MenuView({ open, onClose }: { open: Open; onClose: () => void }) {
  const ref = useRef<HTMLDivElement | null>(null);
  const [pos, setPos] = useState({ x: open.x, y: open.y });
  const [active, setActive] = useState(-1);
  const actionable = useMemo(() => open.items.map((it, i) => ({ it, i })).filter((x): x is { it: MenuItem; i: number } => typeof x.it === "object" && "onSelect" in x.it && !x.it.disabled), [open.items]);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    setPos({ x: Math.max(6, Math.min(open.x, window.innerWidth - r.width - 6)), y: Math.max(6, Math.min(open.y, window.innerHeight - r.height - 6)) });
  }, [open]);

  const choose = useCallback(
    (item: MenuItem) => {
      onClose();
      void item.onSelect();
    },
    [onClose],
  );

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        onClose();
      } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        e.stopPropagation();
        setActive((a) => {
          const at = actionable.findIndex((x) => x.i === a);
          const next = e.key === "ArrowDown" ? (at + 1) % actionable.length : (at <= 0 ? actionable.length : at) - 1;
          return actionable[next]?.i ?? -1;
        });
      } else if (e.key === "Enter") {
        const hit = actionable.find((x) => x.i === active);
        if (hit) {
          e.preventDefault();
          e.stopPropagation();
          choose(hit.it);
        }
      }
    };
    const onDown = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node)) onClose();
    };
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("pointerdown", onDown, true);
    window.addEventListener("blur", onClose);
    window.addEventListener("resize", onClose);
    window.addEventListener("wheel", onClose, { passive: true });
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("pointerdown", onDown, true);
      window.removeEventListener("blur", onClose);
      window.removeEventListener("resize", onClose);
      window.removeEventListener("wheel", onClose);
    };
  }, [actionable, active, choose, onClose]);

  return createPortal(
    <div
      ref={ref}
      role="menu"
      data-testid="context-menu"
      // Keep the caret and selection where they are while the menu is used.
      onMouseDown={(e) => e.preventDefault()}
      onContextMenu={(e) => e.preventDefault()}
      onPointerDown={(e) => e.stopPropagation()}
      className="fixed z-[200] min-w-44 max-w-72 select-none rounded-lg border border-border bg-popover p-1 text-sm text-popover-foreground shadow-lg"
      style={{ left: pos.x, top: pos.y, pointerEvents: "auto" }}
    >
      {open.items.map((it, i) => {
        if (it === "separator") return <div key={i} role="separator" className="-mx-1 my-1 h-px bg-border" />;
        if ("heading" in it) return <div key={i} className="px-2 py-1 text-xs text-muted-foreground">{it.heading}</div>;
        return (
          <button
            key={i}
            type="button"
            role="menuitem"
            disabled={it.disabled}
            data-testid={it.testId}
            onMouseEnter={() => setActive(i)}
            onClick={() => choose(it)}
            className={cn(
              "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left outline-none disabled:pointer-events-none disabled:opacity-40",
              active === i && "bg-foreground/10",
              it.destructive && "text-destructive",
            )}
          >
            {it.icon && <span className="flex size-4 shrink-0 items-center justify-center [&_svg]:size-4">{it.icon}</span>}
            <span className="min-w-0 flex-1 truncate">{it.label}</span>
            {it.shortcut && <span className="text-xs text-muted-foreground">{it.shortcut}</span>}
          </button>
        );
      })}
    </div>,
    document.body,
  );
}
