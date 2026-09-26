// Exit-criterion step 2: with fresh app data and no INFINABOX_GODOT, the
// Play panel offers "Install Godot", downloads the real release with real
// byte progress, and ends up installed (and able to run the game).

import fs from "node:fs";
import path from "node:path";
import { clickWhenEnabled, textOf, tid, waitAttr, waitUntil, waitVisible } from "../lib/ui.mjs";
import { createProjectFromHome, gamePids, tempParent } from "./common.mjs";

const READY_LINE = "[infinabox] ready 1";

// Godot-related files under `dir` (the webview keeps its own storage there too).
function listFiles(dir) {
  if (!fs.existsSync(dir)) return [];
  return fs.readdirSync(dir, { recursive: true, withFileTypes: true })
    .filter((d) => d.isFile() && /godot/i.test(path.join(d.parentPath ?? d.path, d.name)))
    .map((d) => path.join(d.parentPath ?? d.path, d.name));
}

export async function managedInstall(run, app) {
  const { driver, home } = app;
  const appData = path.join(home, ".local/share/com.infinabox.app");
  let project = null;

  await run.step("6a", "Fresh app data, no INFINABOX_GODOT: Play panel offers Install Godot", async () => {
    await waitVisible(driver, tid("new-project"), { timeoutMs: 30_000 });
    project = await createProjectFromHome(run, driver, tempParent("install"), "Install Test");
    await waitAttr(driver, tid("play-panel"), "data-godot", "missing", { timeoutMs: 30_000 });
    await waitVisible(driver, tid("godot-install"));
    run.note(`Godot files in the app data dir before install: ${JSON.stringify(listFiles(appData).map((f) => path.relative(home, f)))}`);
  });

  await run.step(
    "6b",
    "Install Godot: progress shows real bytes, ends installed",
    async () => {
      await clickWhenEnabled(driver, tid("godot-install"));
      const samples = [];
      let shotTaken = false;
      const status = await waitUntil(
        async () => {
          const godot = await driver.findElement(tid("play-panel")).getAttribute("data-godot");
          if (godot === "installed") return "installed";
          const errs = await driver.findElements(tid("godot-install-error"));
          if (errs.length > 0) throw Object.assign(new Error(await errs[0].getText()), { fatal: true });
          const bytes = await driver.findElements(tid("godot-install-bytes"));
          const phase = await driver.findElements(tid("godot-install-phase"));
          if (bytes.length > 0) {
            const inner = (el) => driver.executeScript("return arguments[0].textContent.trim()", el);
            const sample = `${phase.length ? await inner(phase[0]) : "?"} ${await inner(bytes[0])}`;
            if (samples[samples.length - 1] !== sample) samples.push(sample);
            if (!shotTaken && /MB of/.test(sample)) {
              await run.shot("install-progress");
              shotTaken = true;
            }
          }
          return null;
        },
        { timeoutMs: 15 * 60_000, intervalMs: 500, what: "Godot to finish installing" },
      ).catch((err) => {
        throw new Error(`${err.message}; progress samples: ${JSON.stringify(samples)}`);
      });
      run.note(`progress samples (${samples.length}): ${JSON.stringify(samples.slice(0, 3))} … ${JSON.stringify(samples.slice(-3))}`);
      if (samples.length < 2) throw new Error("fewer than two distinct progress readings were shown");
      run.note(`status: ${status}`);
      const panelText = await textOf(driver, tid("play-panel"));
      const footer = panelText.split("\n").filter((l) => /Godot \d/.test(l));
      run.note(`footer: ${JSON.stringify(footer)}`);
      if (footer.some((l) => l.includes("your own install"))) throw new Error("footer says it's the user's own install");
      const files = listFiles(appData).map((f) => path.relative(home, f));
      run.note(`Godot files in the app data dir after install: ${JSON.stringify(files.slice(0, 10))}`);
    },
    { needs: ["6a"] },
  );

  await run.step(
    "6c",
    "The managed Godot runs the game",
    async () => {
      await clickWhenEnabled(driver, tid("game-play"));
      await waitAttr(driver, tid("play-panel"), "data-game-state", "running", { timeoutMs: 120_000 });
      await waitUntil(async () => (await textOf(driver, tid("game-output"))).includes(READY_LINE), {
        timeoutMs: 60_000,
        what: `"${READY_LINE}" in the game output`,
      });
      run.note(`game pids: ${gamePids(project).join(",")}`);
      await new Promise((r) => setTimeout(r, 1500));
      await run.shot("managed-godot-running");
      await clickWhenEnabled(driver, tid("game-stop"));
      await waitAttr(driver, tid("play-panel"), "data-game-state", "stopped", { timeoutMs: 30_000 });
    },
    { needs: ["6b"] },
  );
}
