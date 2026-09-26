// Steps shared by more than one scenario.

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { By } from "selenium-webdriver";
import { clickWhenEnabled, isInert, textOf, tid, waitUntil, waitVisible } from "../lib/ui.mjs";
import { chooseFolderInGtkDialog, findWindows } from "../lib/x11.mjs";

/** Makes a fresh parent folder under the system temp dir — never inside
 * this repository, which core refuses ("inside another git repository"). */
export function tempParent(label) {
  return fs.mkdtempSync(path.join(os.tmpdir(), `ibx-e2e-${label}-projects-`));
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
  await clickWhenEnabled(driver, tid("new-project-create"));

  // Studio is one of App.tsx's permanently mounted sections: it counts as
  // "landed" when its Play panel is displayed and not inside an inert
  // (parked) layer.
  const panel = await waitVisible(driver, tid("play-panel"), { timeoutMs: 30_000 });
  await waitUntil(async () => !(await isInert(driver, panel)), { what: "Studio to become the active section" });
  const projectPath = path.join(parentDir, name);
  run.note(`landed in Studio for ${projectPath}`);
  return projectPath;
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
  return out
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
