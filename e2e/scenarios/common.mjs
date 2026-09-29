// Steps shared by more than one scenario.

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { By, Key } from "selenium-webdriver";
import { clickWhenEnabled, isInert, textOf, tid, waitAttr, waitUntil, waitVisible } from "../lib/ui.mjs";
import { chooseFolderInGtkDialog, findWindows } from "../lib/x11.mjs";

let tempRoot = null;

/** The run's own temp folder (run.mjs makes one per run), which every
 * project and app home goes under. */
export function setTempRoot(dir) {
  tempRoot = dir;
}

/** Makes a fresh parent folder under this run's temp root — never inside
 * this repository, which core refuses ("inside another git repository"). */
export function tempParent(label) {
  return fs.mkdtempSync(path.join(tempRoot ?? os.tmpdir(), `projects-${label}-`));
}

/**
 * Home → New Project → Choose… (native GTK dialog, driven with xdotool) →
 * name → Create Project, then waits for Studio to be the visible section.
 * Returns the new project's path.
 */
export async function createProjectFromHome(run, driver, parentDir, name) {
  await clickWhenEnabled(driver, tid("new-project"));
  await clickWhenEnabled(driver, tid("new-project-choose-location"));
  await chooseFolderInGtkDialog(parentDir);
  let shown;
  await waitUntil(
    async () => {
      shown = (await textOf(driver, tid("new-project-location"))).trim();
      return shown === parentDir;
    },
    { what: `the chosen location to read ${parentDir}` },
  ).catch((err) => {
    throw new Error(`${err.message}; it reads ${JSON.stringify(shown)}`);
  });
  run.note(`location chosen through the GTK dialog: ${parentDir}`);
  const nameInput = await waitVisible(driver, tid("new-project-name"));
  await nameInput.sendKeys(name);
  await run.shot("new-project-dialog");
  // However long the path, everything stays inside the dialog (a long path
  // once pushed the location, preview and Create button past its edge).
  const outside = await driver.executeScript(`
    const dialog = document.querySelector('[role="dialog"]').getBoundingClientRect();
    return [...document.querySelectorAll('[role="dialog"] *')]
      .filter((el) => {
        const r = el.getBoundingClientRect();
        return r.width > 0 && (r.right > dialog.right + 1 || r.left < dialog.left - 1);
      })
      .map((el) => el.dataset.testid || el.tagName.toLowerCase() + ':' + (el.textContent || '').trim().slice(0, 30));
  `);
  if (outside.length > 0) throw new Error(`New Project dialog content runs past its edge: ${JSON.stringify(outside)}`);
  await clickWhenEnabled(driver, tid("new-project-create"));
  await waitForStudio(driver);
  const projectPath = path.join(parentDir, name);
  run.note(`landed in Studio for ${projectPath}`);
  return projectPath;
}

/** Studio is one of App.tsx's permanently mounted sections: it counts as
 * "landed" when its Play panel is displayed and not inside an inert
 * (parked) layer. */
export async function waitForStudio(driver) {
  const panel = await waitVisible(driver, tid("play-panel"), { timeoutMs: 30_000 });
  await waitUntil(async () => !(await isInert(driver, panel)), { what: "Studio to become the active section" });
}

/** The project's own settings file (`.ibproject/settings.json`), or null
 * while it doesn't exist (the defaults apply). */
export function projectSettingsOnDisk(project) {
  const file = path.join(project, ".ibproject/settings.json");
  return fs.existsSync(file) ? JSON.parse(fs.readFileSync(file, "utf8")) : null;
}

/** Opens the chat header's Studio settings, reads what it shows (freshly
 * loaded from disk on every open), applies `change` (a switch's test id →
 * the wanted on/off, or `plan` → a plan policy), and closes it again.
 * Returns what it showed before the change. */
export async function useStudioSettings(driver, change = {}) {
  await clickWhenEnabled(driver, tid("studio-settings-button"));
  const popover = await waitVisible(driver, tid("studio-settings"));
  // Loaded when the switches are enabled.
  await waitUntil(async () => (await driver.findElement(tid("setting-auto-fix"))).isEnabled(), {
    what: "the Studio settings to load",
  });
  const read = async () => ({
    plan: (await driver.findElement(tid("setting-plan-always_plan")).getAttribute("data-state")) === "checked"
      ? "always_plan"
      : (await driver.findElement(tid("setting-plan-small_changes_direct")).getAttribute("data-state")) === "checked"
        ? "small_changes_direct"
        : null,
    teach: (await driver.findElement(tid("setting-teach")).getAttribute("data-state")) === "checked",
    auto_fix: (await driver.findElement(tid("setting-auto-fix")).getAttribute("data-state")) === "checked",
  });
  const before = await read();
  for (const [key, want] of Object.entries(change)) {
    if (key === "plan") {
      await driver.findElement(tid(`setting-plan-${want}`)).click();
      await waitAttr(driver, tid(`setting-plan-${want}`), "data-state", "checked");
    } else {
      const id = key === "teach" ? "setting-teach" : "setting-auto-fix";
      if (before[key] !== want) await driver.findElement(tid(id)).click();
      await waitAttr(driver, tid(id), "data-state", want ? "checked" : "unchecked");
    }
  }
  // Every change saves as it's made; the spinner shows until it has.
  await waitUntil(async () => (await popover.findElements(By.css('[aria-label="Saving"]'))).length === 0, {
    what: "the settings to finish saving",
  });
  await driver.actions().sendKeys(Key.ESCAPE).perform();
  await waitUntil(async () => (await driver.findElements(tid("studio-settings"))).length === 0, {
    what: "the Studio settings to close",
  });
  return before;
}

export function git(project, ...args) {
  return execFileSync("git", ["-C", project, ...args]).toString();
}

export function snapshotSubjects(project) {
  return git(project, "log", "--format=%s").trim().split("\n").filter(Boolean);
}

/**
 * Godot processes running `project` as a game (not a headless import),
 * found by their real command line. Matching on the project path keeps
 * this from picking up any other Godot on the machine.
 */
export function gameProcesses(project, { includeHeadless = false } = {}) {
  const out = execFileSync("ps", ["-eo", "pid=,ppid=,args="]).toString();
  const matches = out
    .split("\n")
    .map((line) => line.trim().match(/^(\d+)\s+(\d+)\s+(.*)$/))
    .filter(
      (m) =>
        m &&
        /godot/i.test(m[3]) &&
        m[3].includes(project) &&
        (includeHeadless || !m[3].includes("--headless")),
    )
    .map((m) => ({ pid: m[1], ppid: m[2], args: m[3] }));
  // At startup Godot forks short-lived copies of itself (same command line,
  // parent = the game) to probe the GPUs; those aren't separate games.
  const pids = new Set(matches.map((m) => m.pid));
  return matches.filter((m) => !pids.has(m.ppid));
}

export function gamePids(project) {
  return gameProcesses(project).map((p) => p.pid);
}

/** The visible "InfinaBox" windows of processes running `binary` (this
 * run's app build, matched on its real command line — so another app build
 * running on the same display isn't mistaken for it). */
export function appWindows(binary) {
  const out = execFileSync("ps", ["-eo", "pid=,args="]).toString();
  return out
    .split("\n")
    .map((line) => line.trim().match(/^(\d+)\s+(.*)$/))
    .filter((m) => m && (m[2] === binary || m[2].startsWith(`${binary} `)))
    .flatMap((m) => findWindows(["--all", "--pid", m[1], "--name", "^InfinaBox$"]));
}

/** The game's visible X11 windows (Godot's own window, found by pid). */
export function gameWindows(project) {
  return gamePids(project).flatMap((pid) => findWindows(["--pid", pid]));
}

export async function visibleTexts(driver, locator) {
  const out = [];
  for (const el of await driver.findElements(locator)) {
    // innerText where the browser has laid the element out (keeps line
    // breaks between parts of a row), textContent otherwise — WebKit gives
    // an empty innerText for rows still fading in.
    out.push(
      await driver.executeScript(
        "const e = arguments[0]; return (e.innerText || e.textContent || '').trim()",
        el,
      ),
    );
  }
  return out;
}

export const byText = (tag, text) => By.xpath(`//${tag}[normalize-space()=${JSON.stringify(text)}]`);
