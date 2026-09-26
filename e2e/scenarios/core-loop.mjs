// The Phase A exit criterion: Home → New Project → Studio → Play, with
// Godot provided through INFINABOX_GODOT, then either
// - with --real-ai: a real AI chat turn, undo, and a script error the AI is
//   asked to fix (ai-loop.mjs), or
// - without it: a script error → "Ask AI to fix" → undo / go back, with the
//   AI's edit and snapshot stood in for (steps 4–5c below).

import fs from "node:fs";
import path from "node:path";
import { By } from "selenium-webdriver";
import {
  clickWhenEnabled,
  invokeCommand,
  textOf,
  tid,
  waitAttr,
  waitUntil,
  waitVisible,
} from "../lib/ui.mjs";
import { xdotool } from "../lib/x11.mjs";
import { aiLoop } from "./ai-loop.mjs";
import {
  createProjectFromHome,
  gamePids,
  gameProcesses,
  appWindows,
  gameWindows,
  git,
  snapshotSubjects,
  tempParent,
  visibleTexts,
} from "./common.mjs";

const PROJECT_NAME = "E2E Game";
const BREAK_TITLE = "Make the player faster";
const READY_LINE = "[infinabox] ready 1";

export async function coreLoop(run, app, config) {
  const { driver } = app;
  // Saves what the Play panel's output log and error list show, and the
  // Godot processes running this project, after every game step.
  const saveGameLog = (label) => async () => {
    const panels = await driver.findElements(tid("game-output"));
    if (panels.length === 0) return;
    const output = await textOf(driver, tid("game-output"));
    const errors = await visibleTexts(driver, tid("game-error"));
    const procs = state.project ? gameProcesses(state.project, { includeHeadless: true }).map((p) => `${p.pid} (parent ${p.ppid}): ${p.args}`) : [];
    run.attach(
      `${label}-game-log`,
      `== game processes:\n${procs.join("\n") || "none"}\n== error rows:\n${errors.join("\n--\n")}\n== output:\n${output}\n`,
    );
  };
  const state = { project: null, originalPlayer: null, brokenPlayer: null, errorLine: null };

  await run.step("1", "App starts on Home; claude and git show as installed", async () => {
    await waitVisible(driver, tid("new-project"), { timeoutMs: 30_000 });
    const claude = await waitAttr(driver, tid("cli-tool-claude"), "data-status", ["installed", "missing"]);
    const gitStatus = await waitAttr(driver, tid("cli-tool-git"), "data-status", ["installed", "missing"]);
    run.note(`claude: ${claude}, git: ${gitStatus}`);
    if (claude !== "installed") throw new Error(`claude shows as ${claude}`);
    if (gitStatus !== "installed") throw new Error(`git shows as ${gitStatus}`);
    const studio = await driver.findElements(tid("play-panel"));
    if (studio.length > 0 && (await studio[0].isDisplayed())) throw new Error("Studio is showing, not Home");
  });

  await run.step("2", "New Project creates a Godot project with one snapshot and opens Studio", async () => {
    // A deliberately long folder name, so the New Project dialog has to
    // cope with a long location (see createProjectFromHome).
    const parent = tempParent("core-in-a-folder-with-a-rather-long-name-to-check-the-dialog-wraps");
    state.project = await createProjectFromHome(run, driver, parent, PROJECT_NAME);
    const p = state.project;
    for (const rel of [
      "project.godot",
      ".ibproject/.ibx",
      "addons/infinabox/plugin.cfg",
      "addons/infinabox/infinabox_runtime.gd",
      "player.gd",
    ]) {
      if (!fs.existsSync(path.join(p, rel))) throw new Error(`missing ${rel} in the new project`);
    }
    const subjects = snapshotSubjects(p);
    run.note(`git log: ${JSON.stringify(subjects)}`);
    if (subjects.length !== 1 || subjects[0] !== "New project") {
      throw new Error(`expected exactly one snapshot "New project", got ${JSON.stringify(subjects)}`);
    }
    // Opening Studio creates the project's first chat thread ("Main") under
    // .ibproject/chat/; it's committed with the next snapshot, so it's the
    // one thing allowed to be new here.
    const chatOnly = git(p, "status", "--porcelain", "--", ".ibproject/chat").trim();
    if (chatOnly) run.note(`not yet committed (expected): ${chatOnly.replace(/\n/g, " ")}`);
    const dirty = git(p, "status", "--porcelain", "--", ".", ":(exclude).ibproject/chat").trim();
    if (dirty) throw new Error(`working tree not clean after create:\n${dirty}`);
    const godotCfg = fs.readFileSync(path.join(p, "project.godot"), "utf8");
    run.note(`project.godot config/name: ${godotCfg.match(/config\/name=.*/)?.[0]}`);
    state.originalPlayer = fs.readFileSync(path.join(p, "player.gd"), "utf8");

    await waitAttr(driver, tid("history-panel"), "data-list-status", "ready");
    const titles = await visibleTexts(driver, tid("snapshot-title"));
    run.note(`History panel: ${JSON.stringify(titles)}`);
    if (titles.length !== 1 || titles[0] !== "New project") {
      throw new Error(`History panel should list just "New project", shows ${JSON.stringify(titles)}`);
    }
    await expectGripClear(run, driver);
  });

  await run.step(
    "3",
    "Play: Godot installed, game runs with the ready line and its own window, Stop stops it",
    async () => {
      await waitAttr(driver, tid("play-panel"), "data-godot", "installed");
      const panelText = await textOf(driver, tid("play-panel"));
      run.note(`Play panel footer: ${panelText.split("\n").filter((l) => /Godot/.test(l)).join(" | ")}`);
      await clickWhenEnabled(driver, tid("game-play"));
      await waitAttr(driver, tid("play-panel"), "data-game-state", "running", { timeoutMs: 90_000 });
      await waitUntil(async () => (await textOf(driver, tid("game-output"))).includes(READY_LINE), {
        timeoutMs: 60_000,
        what: `"${READY_LINE}" in the game output`,
      });
      run.note(`output shows "${READY_LINE}"`);
      const windows = await waitUntil(() => gameWindows(state.project).length > 0 && gameWindows(state.project), {
        timeoutMs: 30_000,
        what: "the Godot game window",
      });
      for (const w of windows) run.note(`game window ${w.id} "${w.name}" ${w.geometry}`);
      // The app places the game beside its own window when the screen has
      // room for a usable one (at least 480px wide plus 16px gaps — see
      // `hint_beside` in src-tauri/src/commands/godot.rs); then the two
      // must not overlap. Without room, Godot places it itself.
      // This launch's own window, found through the app binary's process —
      // another InfinaBox (e.g. a parallel run on the same display) has a
      // window with the same name.
      const [main] = appWindows(config.binary);
      if (!main) throw new Error(`couldn't find the InfinaBox window of ${config.binary}`);
      run.note(`InfinaBox window ${main.geometry}`);
      const box = (g) => {
        const m = g.match(/Position: (-?\d+),(-?\d+).*Geometry: (\d+)x(\d+)/);
        return m && { x: +m[1], y: +m[2], w: +m[3], h: +m[4] };
      };
      const a = box(main.geometry);
      const b = box(windows[0].geometry);
      const [screenW] = (xdotool("getdisplaygeometry") || "0 0").trim().split(/\s+/).map(Number);
      const room = Math.max(screenW - (a.x + a.w), a.x) - 32 >= 480;
      const overlaps = a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
      run.note(`screen width ${screenW}; room beside the app: ${room ? "yes" : "no"}`);
      if (overlaps && room) throw new Error("the game window overlaps the InfinaBox window");
      run.note(overlaps ? "the game window overlaps the app (no room beside it on this screen)" : "the game window is beside the app, not over it");
      // Give the renderer a moment to draw its first frames before the capture.
      await new Promise((r) => setTimeout(r, 1500));
      await run.shot("game-window", windows[0].id);
      await run.shot("desktop-with-game");

      await clickWhenEnabled(driver, tid("game-stop"));
      await waitAttr(driver, tid("play-panel"), "data-game-state", "stopped", { timeoutMs: 30_000 });
      await waitUntil(() => gamePids(state.project).length === 0, { what: "the Godot process to exit" });
      run.note("stopped; Godot process gone");
    },
    { needs: ["2"], after: saveGameLog("3") },
  );

  if (config.realAi) {
    await aiLoop(run, app, config, state, saveGameLog);
    return;
  }

  await run.step(
    "4",
    'A script error shows in the Play panel with res:// file and line; "Ask AI to fix" reaches the chat',
    async () => {
      const file = path.join(state.project, "player.gd");
      // As the AI would: edit the script on disk, then the turn's automatic
      // snapshot (through the same `snapshot_create` command the app uses).
      const lines = state.originalPlayer.replace(/\n$/, "").split("\n");
      lines.push("", "", "func _describe_speed() -> void:", '\tvar label_speed: int = "fast"', "\tprint(label_speed)");
      state.errorLine = lines.length - 1;
      state.brokenPlayer = `${lines.join("\n")}\n`;
      fs.writeFileSync(file, state.brokenPlayer);
      const snap = await invokeCommand(driver, "snapshot_create", {
        projectPath: state.project,
        title: BREAK_TITLE,
      });
      run.note(`snapshot_create → ${snap?.id?.slice(0, 8)} "${snap?.title}" (${snap?.files_changed} file)`);
      await waitUntil(async () => (await visibleTexts(driver, tid("snapshot-title")))[0] === BREAK_TITLE, {
        what: "History to list the new snapshot on top",
      });

      await clickWhenEnabled(driver, tid("game-play"));
      await waitAttr(driver, tid("play-panel"), "data-game-state", "running", { timeoutMs: 90_000 });
      await waitVisible(driver, tid("game-error"), { timeoutMs: 60_000 });
      const errors = await visibleTexts(driver, tid("game-error"));
      for (const e of errors) run.note(`error row: ${e.replace(/\n/g, " | ")}`);
      const want = `res://player.gd, line ${state.errorLine}`;
      if (!errors.some((e) => e.includes(want))) {
        throw new Error(`no error row mentions "${want}"; rows: ${JSON.stringify(errors)}`);
      }
      // One broken line is one problem: Godot's follow-up blocks about the
      // same script ("Failed to load script …", a second wording of the
      // parse error) are grouped under the parse error, not rows of their own.
      await expectOneRowFor(driver, "res://player.gd", want);
      await run.shot("error-in-play-panel");
      await expectGripClear(run, driver);

      // "Ask AI to fix" on the row that carries the user's file and line.
      let row = null;
      for (const el of await driver.findElements(tid("game-error"))) {
        const t = await driver.executeScript("return arguments[0].textContent", el);
        if (t.includes(want)) row = el;
      }
      const composer = await driver.findElement(By.css('textarea[aria-label="Message the AI"]'));
      const draftBefore = await composer.getAttribute("value");
      const askButton = await row.findElement(tid("ask-ai-to-fix"));
      await askButton.click();
      // The button says what really happened: sent to the AI, or only put
      // in the chat box as a draft (e.g. while the chat can't send).
      const outcome = await waitUntil(async () => (await askButton.getAttribute("data-outcome")) || null, {
        what: "the Ask AI to fix button to report what happened",
      });
      const label = (await askButton.getText()).trim();
      run.note(`Ask AI to fix → ${outcome}: "${label}"`);
      const reached = await waitUntil(
        async () => {
          const draft = await composer.getAttribute("value");
          if (draft.includes(want)) return { where: "composer draft", text: draft };
          const logs = await driver.findElements(By.css('[role="log"][aria-label="Conversation"]'));
          for (const log of logs) {
            const t = await driver.executeScript("return arguments[0].innerText", log);
            if (t.includes(want)) return { where: "conversation", text: t };
          }
          return null;
        },
        { what: "the fix request to reach the chat" },
      );
      run.note(`fix request reached the ${reached.where}: ${JSON.stringify(reached.text.slice(0, 160))}…`);
      const expected = reached.where === "composer draft" && !draftBefore.includes(want)
        ? { outcome: "drafted", label: "Added to the chat box" }
        : { outcome: "sent", label: "Sent to chat" };
      if (outcome !== expected.outcome || label !== expected.label) {
        throw new Error(
          `the request landed in the ${reached.where}, but the button says ${outcome} "${label}" (expected ${expected.outcome} "${expected.label}")`,
        );
      }
      // Whatever the chat says about its own state (e.g. an honest "Couldn't
      // open the conversation" while the chat backend is unfinished).
      const bodyText = await driver.executeScript("return document.body.innerText");
      const problem = bodyText
        .split("\n")
        .filter((l) => /couldn't open the conversation|not implemented|claude code/i.test(l));
      run.note(`chat status lines: ${JSON.stringify(problem.slice(0, 4))}`);
      // Not a crash: the rest of Studio is still there and live.
      await waitVisible(driver, tid("play-panel"));
      await run.shot("ask-ai-to-fix");
      // A sent request starts a real AI turn (the machine's `claude`), which
      // this stand-in run doesn't want editing files — and History rightly
      // holds off undo while a turn runs. Stop it and wait until it's over.
      if (outcome === "sent") {
        const stops = await driver.findElements(By.xpath('//*[@data-testid="chat-panel"]//button[normalize-space()="Stop"]'));
        if (stops.length > 0) await stops[0].click().catch(() => {});
        await waitAttr(driver, tid("chat-panel"), "data-busy", "false", { timeoutMs: 60_000 });
        const dirty = git(state.project, "status", "--porcelain", "--", ".", ":(exclude).ibproject/chat").trim();
        run.note(`stopped the AI turn the request started; uncommitted project changes: ${JSON.stringify(dirty)}`);
      }
    },
    { needs: ["3"], after: saveGameLog("4") },
  );

  await run.step(
    "5a",
    'History "Undo last change" restores the file, updates the list, and restarts the running game',
    async () => {
      const before = gamePids(state.project);
      if (before.length === 0) throw new Error("expected the game to be running before undo");
      await clickWhenEnabled(driver, tid("undo-last"));
      const notice = await waitVisible(driver, tid("history-notice"), { timeoutMs: 30_000 });
      run.note(`notice: ${await notice.getText()}`);
      const onDisk = fs.readFileSync(path.join(state.project, "player.gd"), "utf8");
      if (onDisk !== state.originalPlayer) throw new Error("player.gd on disk wasn't restored to the original");
      run.note("player.gd on disk matches the original again");
      const titles = await waitUntil(
        async () => {
          const t = await visibleTexts(driver, tid("snapshot-title"));
          return t.length >= 3 ? t : null;
        },
        { what: "History to list the undo" },
      );
      run.note(`History panel: ${JSON.stringify(titles)}`);
      run.note(`git log: ${JSON.stringify(snapshotSubjects(state.project))}`);
      const restarted = await waitUntil(
        () => {
          const now = gamePids(state.project);
          return now.length > 0 && now.every((p) => !before.includes(p)) ? now : null;
        },
        { timeoutMs: 90_000, what: "a new Godot process (the restart)" },
      );
      run.note(`game restarted: pid ${before.join(",")} → ${restarted.join(",")}`);
      for (const p of gameProcesses(state.project, { includeHeadless: true })) {
        run.note(`  process ${p.pid} (parent ${p.ppid}): ${p.args}`);
      }
      await waitAttr(driver, tid("play-panel"), "data-game-state", "running", { timeoutMs: 90_000 });
      await waitUntil(async () => (await textOf(driver, tid("game-output"))).includes(READY_LINE), {
        timeoutMs: 60_000,
        what: `"${READY_LINE}" after the restart`,
      });
      // Give the restarted game a moment to report any error it would have.
      await new Promise((r) => setTimeout(r, 2000));
      const errors = await visibleTexts(driver, tid("game-error"));
      if (errors.length > 0) throw new Error(`errors still listed after undo: ${JSON.stringify(errors)}`);
      run.note("no errors after the restart");
    },
    { needs: ["4"], after: saveGameLog("5a") },
  );

  await run.step(
    "5b",
    'History "Go back" to the broken snapshot restores it (confirmed through the dialog)',
    async () => {
      const rows = await driver.findElements(tid("snapshot-row"));
      let target = null;
      for (const row of rows) {
        const title = await driver.executeScript("return arguments[0].textContent.trim()", await row.findElement(tid("snapshot-title")));
        if (title === BREAK_TITLE) target = row;
      }
      if (!target) throw new Error(`no "${BREAK_TITLE}" row`);
      const before = gamePids(state.project);
      await driver.executeScript("arguments[0].scrollIntoView({block: 'center'})", target);
      await target.findElement(tid("snapshot-go-back")).click();
      const dialog = await waitVisible(driver, By.css('[role="alertdialog"]'));
      run.note(`dialog: ${(await dialog.getText()).split("\n")[0]}`);
      await run.shot("go-back-dialog");
      await dialog.findElement(By.xpath('.//button[normalize-space()="Go back"]')).click();
      const notice = await waitUntil(
        async () => {
          for (const n of await driver.findElements(tid("history-notice"))) {
            const t = await n.getText();
            if (t.startsWith("Went back") || t.startsWith("Couldn't")) return t;
          }
          return null;
        },
        { timeoutMs: 30_000, what: "the go-back notice" },
      );
      run.note(`notice: ${notice}`);
      const onDisk = fs.readFileSync(path.join(state.project, "player.gd"), "utf8");
      if (onDisk !== state.brokenPlayer) throw new Error("player.gd wasn't put back to the broken version");
      run.note("player.gd on disk is the broken version again");
      run.note(`git log: ${JSON.stringify(snapshotSubjects(state.project))}`);
      if (before.length > 0) {
        await waitUntil(
          () => {
            const now = gamePids(state.project);
            return now.length > 0 && now.every((p) => !before.includes(p));
          },
          { timeoutMs: 90_000, what: "the game to restart after going back" },
        );
        const want = `res://player.gd, line ${state.errorLine}`;
        await waitUntil(async () => (await visibleTexts(driver, tid("game-error"))).some((e) => e.includes(want)), {
          timeoutMs: 60_000,
          what: `the error to come back (${want})`,
        });
        run.note("game restarted and the error is back");
      }
      await clickWhenEnabled(driver, tid("game-stop"));
      await waitAttr(driver, tid("play-panel"), "data-game-state", "stopped", { timeoutMs: 30_000 });
    },
    { needs: ["5a"], after: saveGameLog("5b") },
  );

  await run.step(
    "5c",
    'An engine error with no file in the project gets no "Ask AI to fix"; the game\'s own error still does',
    async () => {
      // A real engine error that isn't about the game: without the null
      // sound device this launch's home provides (see makeAppHome in
      // run.mjs), a machine with no sound card makes Godot report ALSA's
      // ERR_CANT_OPEN from engine C++ source on every run.
      const asoundrc = path.join(app.home, ".asoundrc");
      const saved = fs.readFileSync(asoundrc, "utf8");
      fs.rmSync(asoundrc);
      try {
        await clickWhenEnabled(driver, tid("game-play"));
        await waitAttr(driver, tid("play-panel"), "data-game-state", ["running", "crashed"], { timeoutMs: 90_000 });
        const want = `res://player.gd, line ${state.errorLine}`;
        await waitUntil(async () => (await visibleTexts(driver, tid("game-error"))).some((e) => e.includes(want)), {
          timeoutMs: 60_000,
          what: `the script error (${want})`,
        });
        // Engine errors can trail the script one; give them a moment.
        await new Promise((r) => setTimeout(r, 3000));
        const rows = await driver.executeScript(`
          return [...document.querySelectorAll('[data-testid="game-error"]')].map((row) => ({
            text: (row.innerText || row.textContent || "").trim(),
            fixable: row.dataset.fixable,
            hasButton: !!row.querySelector('[data-testid="ask-ai-to-fix"]'),
            hasNote: !!row.querySelector('[data-testid="game-error-engine-note"]'),
          }));
        `);
        for (const r of rows) run.note(`row (fixable=${r.fixable}, button=${r.hasButton}): ${r.text.replace(/\n/g, " | ")}`);
        const wrong = rows.filter((r) => r.hasButton !== r.text.includes("res://") || r.hasNote === r.hasButton);
        if (wrong.length > 0) {
          throw new Error(`"Ask AI to fix" should show exactly on rows naming a res:// file: ${JSON.stringify(wrong)}`);
        }
        const engine = rows.filter((r) => !r.hasButton);
        if (engine.length === 0) {
          run.note("no engine error appeared on this machine (it has a working sound device?) — only the game's own error was checked");
        } else {
          run.note(`${engine.length} engine error row(s) without "Ask AI to fix"`);
        }
        await run.shot("engine-error");
      } finally {
        fs.writeFileSync(asoundrc, saved);
        const stop = await driver.findElements(tid("game-stop"));
        if (stop.length > 0) {
          await clickWhenEnabled(driver, tid("game-stop"));
          await waitAttr(driver, tid("play-panel"), "data-game-state", "stopped", { timeoutMs: 30_000 });
        }
      }
    },
    { needs: ["5b"], after: saveGameLog("5c") },
  );
}

/** Exactly one error row mentions `file`, and it shows `location`. */
async function expectOneRowFor(driver, file, location) {
  const rows = (await visibleTexts(driver, tid("game-error"))).filter((t) => t.includes(file));
  if (rows.length !== 1 || !rows[0].includes(location)) {
    throw new Error(`expected one error row about ${file} showing "${location}", got ${JSON.stringify(rows)}`);
  }
}

/** The panels' reorder grips don't sit on top of any control (the Play
 * header's Restart, the Chat header's conversation picker, …). Measured
 * from the real layout; the grips are laid out even while invisible. */
async function expectGripClear(run, driver) {
  const clashes = await driver.executeScript(`
    const hit = (a, b) => a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom;
    const out = [];
    const grips = [...document.querySelectorAll('[data-testid="panel-reorder-grip"]')];
    const controls = [...document.querySelectorAll('button, [role="button"], [role="combobox"], a, input, textarea, select')];
    for (const grip of grips) {
      const g = grip.getBoundingClientRect();
      if (g.width === 0 || grip.closest('[inert]')) continue;
      for (const c of controls) {
        if (c.closest('[inert]')) continue;
        const r = c.getBoundingClientRect();
        if (r.width > 0 && r.height > 0 && hit(g, r)) out.push((c.innerText || c.getAttribute('aria-label') || c.tagName).trim());
      }
    }
    return { grips: grips.filter((g) => !g.closest('[inert]')).length, clashes: out };
  `);
  run.note(`reorder grips checked: ${clashes.grips}; overlapping controls: ${JSON.stringify(clashes.clashes)}`);
  if (clashes.clashes.length > 0) throw new Error(`a panel's reorder grip covers ${JSON.stringify(clashes.clashes)}`);
}
