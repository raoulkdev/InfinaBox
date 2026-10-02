import { invoke } from "@tauri-apps/api/core";

// Where the person put things (panel sizes and order, the sidebar's collapsed
// state) is saved in InfinaBox's settings file, through `layout_set`. Screens
// read it synchronously from a copy loaded before the app renders
// (`hydrateLayouts`); every change updates that copy at once and is written
// out a moment later. The browser's localStorage is only a fallback (and where
// layouts saved by earlier versions are picked up from, once).

const cache = new Map<string, unknown>();
const pending = new Map<string, unknown>();
let timer: ReturnType<typeof setTimeout> | null = null;
const LEGACY_PREFIX = "infinabox.layout.";
const WRITE_DELAY_MS = 300;

function legacy(key: string): unknown {
  try {
    const raw = localStorage.getItem(LEGACY_PREFIX + key);
    return raw === null ? undefined : JSON.parse(raw);
  } catch {
    return undefined;
  }
}

/** Loads the saved layouts. Never rejects: without them everything just starts at its defaults. */
export async function hydrateLayouts(): Promise<void> {
  try {
    const saved = await invoke<Record<string, unknown>>("layouts_get");
    for (const [key, value] of Object.entries(saved)) cache.set(key, value);
  } catch {
    // Settings unreadable: fall back to what this browser remembers.
  }
}

/** The saved layout for `key`, or undefined. */
export function getLayout<T>(key: string): T | undefined {
  if (cache.has(key)) return cache.get(key) as T;
  const old = legacy(key);
  if (old !== undefined) {
    // An earlier version saved it in the browser: move it into settings.
    setLayout(key, old);
    return old as T;
  }
  return undefined;
}

export function setLayout(key: string, value: unknown): void {
  cache.set(key, value);
  try {
    localStorage.setItem(LEGACY_PREFIX + key, JSON.stringify(value));
  } catch {
    // Settings are the real store.
  }
  pending.set(key, value);
  if (timer === null) timer = setTimeout(flush, WRITE_DELAY_MS);
}

function flush() {
  timer = null;
  const batch = [...pending.entries()];
  pending.clear();
  for (const [key, value] of batch) {
    invoke("layout_set", { key, value }).catch(() => {
      // Losing one layout write only means it resets next time.
    });
  }
}
