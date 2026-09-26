#!/usr/bin/env node
// InfinaBox end-to-end run: drives the real desktop app (through
// tauri-driver + WebKitWebDriver), a real Godot, real git, and the real
// file system. See README.md for prerequisites and options.
//
//   node run.mjs [--scenario core|install|all] [--keep]

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { startApp } from "./lib/app.mjs";
import { Run } from "./lib/runner.mjs";
import { coreLoop } from "./scenarios/core-loop.mjs";
import { managedInstall } from "./scenarios/managed-install.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, "..");

function arg(name, fallback) {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 ? process.argv[i + 1] : fallback;
}

function which(cmd) {
  try {
    return execFileSync("which", [cmd]).toString().trim() || null;
  } catch {
    return null;
  }
}

const config = {
  binary: path.resolve(process.env.INFINABOX_APP || path.join(repoRoot, "target/debug/tauri-app")),
  tauriDriver:
    process.env.TAURI_DRIVER || which("tauri-driver") || path.join(os.homedir(), ".cargo/bin/tauri-driver"),
  nativeDriver: process.env.WEBKIT_DRIVER || which("WebKitWebDriver") || undefined,
  // The Godot binary the core scenario hands the app via INFINABOX_GODOT.
  godot: process.env.INFINABOX_GODOT || null,
  display: process.env.DISPLAY || ":99",
  scenario: arg("scenario", "all"),
  keep: process.argv.includes("--keep"),
  artifacts: path.resolve(arg("artifacts", path.join(here, "artifacts", new Date().toISOString().replace(/[:.]/g, "-")))),
};
process.env.DISPLAY = config.display;

for (const [label, file] of [
  ["app binary (INFINABOX_APP)", config.binary],
  ["tauri-driver (TAURI_DRIVER)", config.tauriDriver],
]) {
  if (!fs.existsSync(file)) {
    console.error(`Missing ${label}: ${file}`);
    process.exit(2);
  }
}

/**
 * A disposable home for one app launch. HOME and the XDG dirs point inside
 * it, so the app's data dir (where a managed Godot goes), the webview's
 * localStorage (recent projects, layouts), and GTK's settings all start
 * empty and vanish afterwards. PATH is kept, so `claude`/`git` detection
 * still sees the machine's real tools.
 *
 * GTK's file chooser is told to open in the current folder instead of
 * "Recent" (`startup-mode='cwd'` through the keyfile GSettings backend),
 * which is what lets `chooseFolderInGtkDialog` type a path and accept it.
 */
function makeAppHome(label) {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), `ibx-e2e-${label}-`));
  const settings = path.join(home, ".config/glib-2.0/settings");
  fs.mkdirSync(settings, { recursive: true });
  fs.writeFileSync(path.join(settings, "keyfile"), "[org/gtk/settings/file-chooser]\nstartup-mode='cwd'\n");
  // Headless CI boxes have no sound card, and Godot then reports a real
  // engine ERROR on every run (ALSA "ERR_CANT_OPEN" in
  // audio_driver_alsa.cpp) that would show in the Play panel's error list.
  // A null default PCM gives ALSA a device to open, so the only errors left
  // are ones the game itself causes.
  fs.writeFileSync(path.join(home, ".asoundrc"), "pcm.!default { type null }\n");
  return home;
}

function appEnv(home, extra = {}) {
  const env = {
    ...process.env,
    HOME: home,
    XDG_DATA_HOME: path.join(home, ".local/share"),
    XDG_CONFIG_HOME: path.join(home, ".config"),
    XDG_CACHE_HOME: path.join(home, ".cache"),
    GSETTINGS_BACKEND: "keyfile",
    DISPLAY: config.display,
    INFINABOX_GODOT: undefined,
    ...extra,
  };
  return env;
}

async function launch(label, extraEnv) {
  const home = makeAppHome(label);
  const app = await startApp({
    binary: config.binary,
    tauriDriver: config.tauriDriver,
    nativeDriver: config.nativeDriver,
    env: appEnv(home, extraEnv),
    logDir: run.dir,
    label,
  });
  return { ...app, home };
}

const run = new Run(config.artifacts);
console.log(`Artifacts: ${run.dir}`);
console.log(`App: ${config.binary}`);

const cleanups = [];
try {
  if (config.scenario === "core" || config.scenario === "all") {
    if (!config.godot) {
      console.error("The core scenario needs INFINABOX_GODOT pointing at a Godot 4 binary.");
      process.exit(2);
    }
    const app = await launch("core", { INFINABOX_GODOT: config.godot });
    cleanups.push(app);
    run.driver = app.driver;
    await coreLoop(run, app, config);
    await app.stop();
    cleanups.pop();
    if (!config.keep) fs.rmSync(app.home, { recursive: true, force: true });
  }
  if (config.scenario === "install" || config.scenario === "all") {
    const app = await launch("install", {});
    cleanups.push(app);
    run.driver = app.driver;
    await managedInstall(run, app, config);
    await app.stop();
    cleanups.pop();
    if (!config.keep) fs.rmSync(app.home, { recursive: true, force: true });
  }
} finally {
  for (const app of cleanups) await app.stop();
}

process.exit(run.writeReport() ? 0 : 1);
