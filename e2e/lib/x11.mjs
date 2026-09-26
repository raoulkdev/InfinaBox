// X11 helpers: native windows the webview can't see (GTK file dialogs, the
// game's own window) and screenshots. Everything goes through `xdotool` and
// ImageMagick's `import`, against the display in `DISPLAY`.

import { execFileSync } from "node:child_process";

function env() {
  return { ...process.env, DISPLAY: process.env.DISPLAY || ":99" };
}

/** Runs xdotool; returns stdout, or null when it exits non-zero (which is
 * how `xdotool search` says "no match"). */
export function xdotool(...args) {
  try {
    return execFileSync("xdotool", args, { env: env(), stdio: ["ignore", "pipe", "pipe"] }).toString();
  } catch {
    return null;
  }
}

/** Visible windows matching `xdotool search` options, e.g. ["--name", "Select Folder"]. */
export function findWindows(searchArgs) {
  const out = xdotool("search", "--onlyvisible", ...searchArgs);
  if (!out) return [];
  return out
    .trim()
    .split("\n")
    .filter(Boolean)
    .map((id) => ({
      id,
      name: (xdotool("getwindowname", id) || "").trim(),
      geometry: (xdotool("getwindowgeometry", id) || "").replace(/\s+/g, " ").trim(),
    }));
}

export async function waitForWindows(searchArgs, { timeoutMs = 15_000, present = true } = {}) {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const found = findWindows(searchArgs);
    if (present ? found.length > 0 : found.length === 0) return found;
    if (Date.now() > deadline) {
      throw new Error(
        `Timed out after ${timeoutMs}ms waiting for window [${searchArgs.join(" ")}] to ${present ? "appear" : "go away"}`,
      );
    }
    await new Promise((r) => setTimeout(r, 200));
  }
}

/** Screenshot of the whole screen, or of one window when `windowId` is given. */
export function screenshot(file, windowId = "root") {
  execFileSync("import", ["-window", windowId, file], { env: env(), stdio: "ignore" });
  return file;
}

/**
 * Chooses `folder` in the open GTK "Select Folder" dialog (the Tauri dialog
 * plugin's native picker, which WebDriver can't reach): focus it, open the
 * location entry with Ctrl+L, type the path with a trailing slash (GTK
 * navigates into the folder as it's typed), and press Enter, which accepts
 * the current folder.
 *
 * This relies on the dialog starting in a real folder rather than GTK's
 * default "Recent" view, where Enter in the location entry does nothing;
 * `startApp` arranges that with the `startup-mode='cwd'` GSettings key (see
 * `appEnv` in run.mjs).
 */
export async function chooseFolderInGtkDialog(folder, { title = "Select Folder" } = {}) {
  const [dialog] = await waitForWindows(["--name", title]);
  xdotool("windowfocus", "--sync", dialog.id);
  await new Promise((r) => setTimeout(r, 300));
  xdotool("key", "--clearmodifiers", "ctrl+l");
  await new Promise((r) => setTimeout(r, 300));
  xdotool("type", "--delay", "15", folder.endsWith("/") ? folder : `${folder}/`);
  await new Promise((r) => setTimeout(r, 700));
  xdotool("key", "Return");
  await waitForWindows(["--name", title], { present: false, timeoutMs: 10_000 });
}
