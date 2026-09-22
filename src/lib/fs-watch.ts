import { listen } from "@tauri-apps/api/event";

/** Fires whenever anything changes on disk under the currently watched
 * project root (see the Rust `watch_project_path` command, started once
 * per opened project in App.tsx) — debounced on the Rust side, so a burst
 * of writes (a git checkout, this app's own Save, a build script) collapses
 * into one event instead of a flood of them. Every panel that reads from
 * disk subscribes to this instead of polling, and re-fetches whatever it's
 * currently showing — the same round trip a manual refresh already does. */
export function onProjectFilesChanged(callback: () => void): () => void {
  let cancelled = false;
  const unlistenPromise = listen("project-fs-changed", () => {
    if (!cancelled) callback();
  });
  return () => {
    cancelled = true;
    void unlistenPromise.then((unlisten) => unlisten());
  };
}
