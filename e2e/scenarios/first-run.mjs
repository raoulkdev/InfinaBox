// The Phase B exit criterion (docs/superpowers/plans/2026-09-28-phase-b-first-run.md,
// "Wave 3"): a first run from fresh app data. Home's checklist → Connect
// your AI (real detection; "Say hello" and "Use this one") → the onboarding
// interview → the review → "Create my game" → Studio. Then, with
// --real-ai: the first build runs by itself, the game plays, a plan turn
// ("add a double jump") shows a plan card and changes nothing until it's
// approved, and a script error made by hand is fixed automatically.
// Without --real-ai the first build is stopped as soon as it starts, and
// the scenario covers the interview, the template scaffold on disk, the
// navigation, the Context cards and the Studio settings.
//
// Godot comes from INFINABOX_GODOT, so the checklist's "Set up Godot" step
// is already done.

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { By, Key } from "selenium-webdriver";
import { clickWhenEnabled, invokeCommand, isInert, textOf, tid, waitAttr, waitUntil, waitVisible } from "../lib/ui.mjs";
import { chooseFolderInGtkDialog } from "../lib/x11.mjs";
import {
  approvePlanIfAny,
  chatRecords,
  expectStoppedNote,
  lastTurnEvents,
  sendChat,
  snapshotTitleFor,
  toolLines,
  waitForTurn,
  waitingPlan,
} from "./ai-loop.mjs";
import {
  gamePids,
  gameProcesses,
  gameWindows,
  git,
  projectSettingsOnDisk,
  snapshotSubjects,
  tempParent,
  useStudioSettings,
  visibleTexts,
  waitForStudio,
} from "./common.mjs";

const GAME_NAME = "Sky Hopper";
const ANSWERS = {
  idea: "A little robot who hops between floating islands to collect lost stars",
};
const PLAN_REQUEST = "add an Options button to the main menu";
const READY_LINE = "[infinabox] ready 1";
const NOT_CHAT = [".", ":(exclude).ibproject/chat", ":(exclude).ibproject/boards"];

const chip = (text) => By.xpath(`//button[@data-chip][normalize-space()=${JSON.stringify(text)}]`);
const chipStartingWith = (text) => By.xpath(`//button[@data-chip][starts-with(normalize-space(), ${JSON.stringify(text)})]`);

function onPath(cmd, pathVar) {
  try {
    execFileSync("which", [cmd], { env: { PATH: pathVar } });
    return true;
  } catch {
    return false;
  }
}

/** Presses the interview's Next (or Skip / Review the plan) and waits for
 * the next question to be the one showing. */
async function next(driver, from) {
  await clickWhenEnabled(driver, tid("onboarding-next"));
  await waitVisible(driver, tid(`onboarding-step-${from + 1}`));
}

/** Error rows that point at the project's own files (res://…). */
async function ownErrors(driver) {
  return (await visibleTexts(driver, tid("game-error"))).filter((e) => e.includes("res://"));
}

/** Makes sure the game runs (pressing Play when it doesn't) and has printed
 * its ready line; returns its process ids. */
async function ensureGameRunning(run, driver, project) {
  const state = await driver.findElement(tid("play-panel")).getAttribute("data-game-state");
  if (state !== "running" && state !== "starting") {
    await clickWhenEnabled(driver, tid("game-play"));
  }
  await waitAttr(driver, tid("play-panel"), "data-game-state", "running", { timeoutMs: 120_000 });
  await waitUntil(async () => (await textOf(driver, tid("game-output"))).includes(READY_LINE), {
    timeoutMs: 60_000,
    what: `"${READY_LINE}" in the game output`,
  });
  const pids = gamePids(project);
  run.note(`game running (pid ${pids.join(",")})`);
  return pids;
}

/** Screenshot of the game's own window. */
async function shootGame(run, project, label) {
  const windows = await waitUntil(() => gameWindows(project).length > 0 && gameWindows(project), {
    timeoutMs: 30_000,
    what: "the game window",
  });
  await new Promise((r) => setTimeout(r, 2000));
  return run.shot(label, windows[0].id);
}

export async function firstRun(run, app, config) {
  const { driver } = app;
  const state = { project: null, created: null };
  const p = () => state.project;
  const saveGameLog = (label) => async () => {
    if ((await driver.findElements(tid("game-output"))).length === 0) return;
    const output = await textOf(driver, tid("game-output"));
    const errors = await visibleTexts(driver, tid("game-error"));
    const procs = p() ? gameProcesses(p(), { includeHeadless: true }).map((g) => `${g.pid} (parent ${g.ppid}): ${g.args}`) : [];
    run.attach(`${label}-game-log`, `== game processes:\n${procs.join("\n") || "none"}\n== error rows:\n${errors.join("\n--\n")}\n== output:\n${output}\n`);
  };

  await run.step("F1", "Fresh app data: Home shows the first-run checklist, with Godot already set up", async () => {
    await waitVisible(driver, tid("first-run-checklist"), { timeoutMs: 30_000 });
    const steps = {};
    for (const id of ["step-connect", "step-godot", "step-first-game"]) {
      steps[id] = await (await waitVisible(driver, tid(id))).getAttribute("data-done");
    }
    run.note(`checklist: ${JSON.stringify(steps)}`);
    // INFINABOX_GODOT is set, so Godot counts as installed.
    await waitAttr(driver, tid("step-godot"), "data-done", "true", { timeoutMs: 30_000 });
    if (steps["step-connect"] === "true") throw new Error("no AI has been chosen yet, but step 1 shows done");
    // Home has its own sidebar; the game sidebar isn't shown.
    await waitVisible(driver, tid("home-sidebar"));
    const nav = await driver.findElements(tid("nav-studio"));
    if (nav.length > 0 && (await nav[0].isDisplayed())) throw new Error("the game sidebar shows on Home");
  });

  await run.step("F2", "Connect your AI: Claude Code is detected installed and signed in; Codex as it really is", async () => {
    const claude = await waitVisible(driver, tid("provider-claude-code"), { timeoutMs: 60_000 });
    const codex = await waitVisible(driver, tid("provider-codex"));
    const read = async (el) => ({
      installed: await el.getAttribute("data-installed"),
      signedIn: await el.getAttribute("data-signed-in"),
      text: (await driver.executeScript("return arguments[0].innerText", el)).replace(/\s+/g, " ").slice(0, 300),
    });
    const c = await read(claude);
    const x = await read(codex);
    run.note(`Claude Code: installed ${c.installed}, signed in ${c.signedIn} — ${c.text}`);
    run.note(`Codex: installed ${x.installed}, signed in ${x.signedIn} — ${x.text}`);
    if (c.installed !== "true" || c.signedIn !== "true") throw new Error("Claude Code should show as installed and signed in");
    // What the app really finds: its PATH is this run's PATH (plus a login
    // shell's, which on this machine adds nothing for codex).
    const codexThere = onPath("codex", process.env.PATH);
    run.note(`codex on PATH: ${codexThere}`);
    if (String(codexThere) !== x.installed) throw new Error(`Codex shows installed=${x.installed}, but it is ${codexThere ? "" : "not "}on PATH`);
    if (!codexThere) {
      const install = await codex.findElements(tid("provider-install-codex"));
      run.note(`Codex offers Install: ${install.length > 0}`);
    }
    const recommended = (await driver.executeScript("return arguments[0].innerText", claude)).includes("Recommended");
    run.note(`Claude Code carries the "Recommended" badge: ${recommended}`);
    if (!recommended) throw new Error("the installed, signed-in Claude Code isn't the recommended one");
  });

  await run.step("F2b", "Installing Codex shows the exact command first, then its real output in the terminal; Cancel stops it", async () => {
    const install = await driver.findElements(tid("provider-install-codex"));
    if (install.length === 0) {
      run.note("Codex offers no Install here (it's installed, or can't be installed from here) — skipped");
      return;
    }
    await install[0].click();
    const command = (await textOf(driver, tid("connect-install-command"))).trim();
    run.note(`the command shown first: ${command}`);
    if (!command) throw new Error("no install command shown before running it");
    await clickWhenEnabled(driver, tid("connect-install-confirm"));
    await waitVisible(driver, tid("connect-terminal"));
    // The installer runs in the app's temp HOME, so nothing on this machine
    // changes. Whatever it prints (download progress, or a network error)
    // must reach the terminal: its output is only sent once the terminal is
    // listening, so none of it is lost.
    const text = await waitUntil(
      async () => {
        const t = await driver.executeScript(
          "const r = document.querySelector('[data-testid=\"connect-terminal\"] .xterm-rows'); return r ? r.innerText : ''",
        );
        return t.trim() ? t : null;
      },
      { timeoutMs: 30_000, what: "the installer's output in the terminal" },
    );
    run.note(`terminal shows: ${JSON.stringify(text.replace(/\s+/g, " ").trim().slice(0, 300))}`);
    await run.shot("connect-terminal");
    const cancel = await driver.findElements(tid("connect-cancel"));
    if (cancel.length > 0 && (await cancel[0].isEnabled())) await cancel[0].click();
    const result = await waitVisible(driver, tid("connect-run-result"), { timeoutMs: 30_000 });
    const success = await result.getAttribute("data-success");
    run.note(`run result (success=${success}): ${(await result.getText()).replace(/\s+/g, " ")}`);
    // Re-detected after the run: "finished installing" only when it's there.
    const installed = await waitAttr(driver, tid("provider-codex"), "data-installed", ["true", "false"]);
    run.note(`Codex installed after the run: ${installed}`);
    if (success === "true" && installed !== "true") {
      throw new Error("the installer run is reported as a success, but Codex isn't installed");
    }
    const close = await driver.findElements(By.xpath('//*[@data-testid="provider-codex"]//button[normalize-space()="Close"]'));
    if (close.length > 0) await close[0].click();
  }, { needs: ["F2"] });

  if (config.realAi) {
    await run.step("F3", '"Say hello" gets a real reply from Claude Code', async () => {
      await clickWhenEnabled(driver, tid("provider-test-claude-code"));
      const result = await waitVisible(driver, By.css('[data-testid="provider-claude-code"] [data-testid="connect-test-result"]'), {
        timeoutMs: 180_000,
      });
      const ok = await result.getAttribute("data-ok");
      const text = (await driver.executeScript("return arguments[0].innerText", result)).replace(/\s+/g, " ");
      run.note(`test result (ok=${ok}): ${text}`);
      if (ok !== "true") throw new Error(`"Say hello" failed: ${text}`);
      if (!/hello/i.test(text)) throw new Error(`the reply doesn't say hello: ${text}`);
    }, { needs: ["F2"] });
  }

  await run.step("F4", '"Use this one" chooses Claude Code; step 1 is done', async () => {
    await clickWhenEnabled(driver, tid("provider-use-claude-code"));
    await waitAttr(driver, tid("step-connect"), "data-done", "true", { timeoutMs: 30_000 });
    const settings = await invokeCommand(driver, "app_settings_get", {});
    run.note(`app settings: ${JSON.stringify(settings)}`);
    if (settings.ai_provider !== "claude-code") throw new Error(`ai_provider is ${settings.ai_provider}`);
    if (settings.first_run_done) throw new Error("first_run_done is already set before any game was made");
    const file = path.join(app.home, ".local/share/com.infinabox.app/settings.json");
    const onDisk = fs.existsSync(file) ? fs.readFileSync(file, "utf8") : null;
    run.note(`app settings file: ${onDisk?.replace(/\s+/g, " ")}`);
    if (!onDisk || !onDisk.includes("claude-code")) throw new Error(`the choice isn't in ${file}`);
  }, { needs: ["F2"] });

  await run.step("F5", '"Make my first game": the interview, answered question by question', async () => {
    await clickWhenEnabled(driver, tid("make-first-game"));
    await waitVisible(driver, tid("onboarding-flow"));
    // 1. The idea.
    const idea = await waitVisible(driver, tid("onboarding-idea"));
    await idea.sendKeys(ANSWERS.idea);
    await run.shot("interview-idea");
    await next(driver, 0);
    // 2. 2D or 3D: both are offered and pickable; 2D is picked.
    await clickWhenEnabled(driver, tid("onboarding-dimension-3d"), { timeoutMs: 30_000 });
    await waitAttr(driver, tid("onboarding-dimension-3d"), "aria-pressed", "true");
    await clickWhenEnabled(driver, tid("onboarding-dimension-2d"));
    await waitAttr(driver, tid("onboarding-dimension-2d"), "aria-pressed", "true");
    await run.shot("interview-dimension");
    await next(driver, 1);
    // 3. Games it's like: optional, skipped.
    await run.shot("interview-references");
    const skip = (await textOf(driver, tid("onboarding-next"))).trim();
    run.note(`references button reads "${skip}"`);
    if (skip !== "Skip") throw new Error(`with no references typed the button should read Skip, reads ${skip}`);
    await next(driver, 2);
    // 4. How technical: balanced to begin with; technical is picked.
    await waitAttr(driver, tid("onboarding-level-balanced"), "aria-pressed", "true");
    await clickWhenEnabled(driver, tid("onboarding-level-technical"));
    await waitAttr(driver, tid("onboarding-level-technical"), "aria-pressed", "true");
    await run.shot("interview-technical");
    await next(driver, 3);
    // 5. Name and folder.
    const name = await waitVisible(driver, tid("onboarding-name"));
    run.note(`suggested name: "${await name.getAttribute("value")}"`);
    run.note(`default folder: "${(await textOf(driver, tid("onboarding-parent-dir"))).trim()}"`);
    await name.sendKeys(Key.chord(Key.CONTROL, "a"), Key.BACK_SPACE);
    await name.sendKeys(GAME_NAME);
    state.parent = tempParent("first-run");
    await clickWhenEnabled(driver, tid("onboarding-choose-folder"));
    await chooseFolderInGtkDialog(state.parent);
    await waitUntil(async () => (await textOf(driver, tid("onboarding-parent-dir"))).trim() === state.parent, {
      what: `the chosen folder to read ${state.parent}`,
    });
    run.note(`folder chosen through the GTK dialog: ${state.parent}`);
    await run.shot("interview-name");
    await clickWhenEnabled(driver, tid("onboarding-next"));
    await waitVisible(driver, tid("onboarding-plan"), { timeoutMs: 30_000 });
  }, { needs: ["F4"] });

  await run.step("F6", "The review shows the 2D foundation with its reason, the notes it will write, and what gets set up (no build)", async () => {
    const template = await waitVisible(driver, tid("onboarding-template"));
    const templateId = await template.getAttribute("data-template-id");
    const reason = (await textOf(driver, tid("onboarding-template-reason"))).trim();
    const cards = await driver.executeScript(
      `return [...document.querySelectorAll('[data-testid="onboarding-card"]')].map((e) => e.dataset.path)`,
    );
    const steps = await visibleTexts(driver, tid("onboarding-build-step"));
    const where = (await textOf(driver, tid("onboarding-project-path"))).trim();
    run.note(`foundation: ${templateId} "${(await template.getText()).trim()}" — ${reason}`);
    run.note(`cards: ${JSON.stringify(cards)}`);
    run.note(`setup steps: ${JSON.stringify(steps)}`);
    run.note(`goes to: ${where}`);
    await run.shot("interview-review");
    if (templateId !== "foundation-2d") throw new Error(`the review picked ${templateId}, not the 2D foundation`);
    if (!reason) throw new Error("the review gives no reason for the foundation");
    for (const card of ["concept.md", "style-guide.md", "systems/overview.md", "tasks/define-the-pillars.md"]) {
      if (!cards.includes(card)) throw new Error(`the review doesn't list ${card}`);
    }
    if (steps.length === 0) throw new Error("the review lists no setup steps");
    if (!steps.some((t) => t.includes("Build nothing else"))) throw new Error("the review doesn't say nothing else is built");
    if (where !== path.join(state.parent, GAME_NAME)) throw new Error(`the game would go to ${where}`);
  }, { needs: ["F5"] });

  await run.step("F7", '"Create my game" lands on Studio; the project is on disk with one snapshot, the foundation and no gameplay', async () => {
    await clickWhenEnabled(driver, tid("onboarding-create"));
    await waitForStudio(driver);
    state.project = path.join(state.parent, GAME_NAME);
    run.note(`landed in Studio for ${state.project}`);
    const subjects = snapshotSubjects(p());
    run.note(`git log: ${JSON.stringify(subjects)}`);
    if (subjects.length !== 1 || subjects[0] !== `New game: ${GAME_NAME}`) {
      throw new Error(`expected exactly one snapshot "New game: ${GAME_NAME}", got ${JSON.stringify(subjects)}`);
    }
    const tracked = git(p(), "ls-files").trim().split("\n");
    const want = [
      "project.godot",
      ".ibproject/.ibx",
      ".ibproject/settings.json",
      ".ibproject/context/concept.md",
      ".ibproject/context/style-guide.md",
      ".ibproject/context/systems/overview.md",
      ".ibproject/context/tasks/define-the-core-loop.md",
      "core/events.gd",
      "core/game_state.gd",
      "core/save_system.gd",
      "levels/sandbox.tscn",
      "ui/main_menu.tscn",
      "addons/infinabox/plugin.cfg",
      "addons/infinabox/infinabox_runtime.gd",
    ];
    const missing = want.filter((f) => !tracked.includes(f));
    run.note(`${tracked.length} files committed`);
    if (missing.length > 0) throw new Error(`not committed in the new game: ${JSON.stringify(missing)}`);
    if (tracked.some((f) => f.includes("player"))) throw new Error("the new game already has a player");
    const concept = fs.readFileSync(path.join(p(), ".ibproject/context/concept.md"), "utf8");
    if (!concept.includes(ANSWERS.idea)) throw new Error("concept.md doesn't carry the idea");
    if (!concept.includes("Not decided yet")) throw new Error("concept.md invents decisions the person didn't make");
    const level = JSON.parse(fs.readFileSync(path.join(p(), ".ibproject/settings.json"), "utf8")).technical_level;
    if (level !== "technical") throw new Error(`the chosen technical level was saved as ${level}`);
    const godotCfg = fs.readFileSync(path.join(p(), "project.godot"), "utf8");
    if (!godotCfg.includes(`config/name="${GAME_NAME}"`)) throw new Error("project.godot isn't named after the game");
    if (!godotCfg.includes("SaveSystem=")) throw new Error("the foundation's autoloads are missing from project.godot");
    const settings = await invokeCommand(driver, "app_settings_get", {});
    if (!settings.first_run_done) throw new Error("first_run_done wasn't set after making the first game");
  }, { needs: ["F6"] });

  await run.step("F8", "Nothing starts by itself: no AI turn, no first build, no changes", async () => {
    await new Promise((r) => setTimeout(r, 5000));
    await waitAttr(driver, tid("chat-panel"), "data-busy", "false");
    if ((await driver.findElements(By.css('[data-testid="system-note"]'))).length > 0) throw new Error("a system message appeared in the chat");
    const messages = chatRecords(p()).filter((r) => r.kind === "user");
    if (messages.length > 0) throw new Error(`the app sent a message on its own: ${JSON.stringify(messages[0].text.slice(0, 120))}`);
    const subjects = snapshotSubjects(p());
    if (subjects.length !== 1) throw new Error(`a snapshot was made on its own: ${JSON.stringify(subjects)}`);
    const dirty = git(p(), "status", "--porcelain", "--", ".", ":!.ibproject").trim();
    if (dirty) throw new Error(`files changed on their own:\n${dirty}`);
    await run.shot("studio-new-game");
    const shown = await useStudioSettings(driver, {});
    run.note(`settings shown: ${JSON.stringify(shown)}`);
  }, { needs: ["F7"] });

  await run.step("F8b", "The foundation runs in Godot with no errors: the menu opens", async () => {
    await ensureGameRunning(run, driver, p());
    await new Promise((r) => setTimeout(r, 4000));
    const errors = await ownErrors(driver);
    run.note(`error rows about the project's files: ${JSON.stringify(errors)}`);
    if (errors.length > 0) throw new Error(`the foundation has errors: ${JSON.stringify(errors)}`);
    await shootGame(run, p(), "foundation-menu");
    const stop = await driver.findElements(tid("game-stop"));
    if (stop.length > 0 && (await stop[0].isEnabled())) await stop[0].click();
    await waitAttr(driver, tid("play-panel"), "data-game-state", "stopped", { timeoutMs: 30_000 });
  }, { needs: ["F8"], after: saveGameLog("F8b") });

  if (config.realAi) {
    await run.step("F9", "A first message is a conversation: the AI talks and asks, and builds nothing", async () => {
      await sendChat(driver, "Let's talk about my game. What would you want to know first, before we build anything?");
      await waitForTurn(run, driver, { startedBy: "the first message", timeoutMs: 10 * 60_000 });
      const changed = git(p(), "status", "--porcelain", "--", ".", ":!.ibproject").trim();
      run.note(`changes outside .ibproject: ${JSON.stringify(changed)}`);
      if (changed) throw new Error(`the AI changed the game while the conversation was still about what to make:\n${changed}`);
      if (snapshotSubjects(p()).length !== 1 && changed) throw new Error("the AI made a snapshot of game changes");
      await run.shot("first-conversation");
    }, { needs: ["F8"] });

    await run.step("F10", `A plan turn ("${PLAN_REQUEST}") shows a plan card and changes nothing; Approve builds it`, async () => {
      const headBefore = git(p(), "rev-parse", "HEAD").trim();
      await sendChat(driver, PLAN_REQUEST);
      await waitForTurn(run, driver, { startedBy: "the double jump request" });
      const plan = await waitingPlan(driver);
      if (!plan) throw new Error("the turn ended without a plan card");
      const dirty = git(p(), "status", "--porcelain", "--", ...NOT_CHAT).trim();
      const headNow = git(p(), "rev-parse", "HEAD").trim();
      run.note(`after the plan: HEAD ${headNow === headBefore ? "unchanged" : "moved"}; uncommitted outside the chat: ${JSON.stringify(dirty)}`);
      if (dirty) throw new Error(`the plan turn changed files:\n${dirty}`);
      if (headNow !== headBefore) throw new Error("the plan turn made a snapshot");
      const title = await approvePlanIfAny(run, driver);
      const want = snapshotTitleFor(title);
      await waitUntil(async () => (await visibleTexts(driver, tid("snapshot-title")))[0] === want, {
        timeoutMs: 30_000,
        what: `History to list "${want}" on top`,
      });
      const subjects = snapshotSubjects(p());
      run.note(`git log: ${JSON.stringify(subjects.slice(0, 3))}`);
      if (subjects[0] !== want) throw new Error(`the newest snapshot is ${subjects[0]}, not "${want}"`);
      run.attach("double-jump-diff", git(p(), "diff", headBefore, "HEAD", "--", ...NOT_CHAT));
      const approved = await driver.findElements(By.css('[data-testid="plan-card"][data-status="approved"]'));
      if (approved.length === 0) throw new Error('no plan card shows "Approved"');
      await run.shot("plan-approved");
    }, { needs: ["F9"], after: saveGameLog("F10") });

    await run.step("F11", "A script error made by hand is fixed automatically", async () => {
      const stop = await driver.findElements(tid("game-stop"));
      if (stop.length > 0 && (await stop[0].isDisplayed()) && (await stop[0].isEnabled())) {
        await stop[0].click();
        await waitAttr(driver, tid("play-panel"), "data-game-state", "stopped", { timeoutMs: 30_000 });
      }
      const settings = projectSettingsOnDisk(p());
      run.note(`project settings on disk: ${JSON.stringify(settings)} (missing = the defaults, auto-fix on)`);
      const file = path.join(p(), "ui/main_menu.gd");
      const original = fs.readFileSync(file, "utf8");
      const lines = original.replace(/\n$/, "").split("\n");
      lines.push("", "", "func _describe_speed() -> void:", '\tvar label_speed: int = "fast"', "\tprint(label_speed)");
      const errorLine = lines.length - 1;
      fs.writeFileSync(file, `${lines.join("\n")}\n`);
      const where = `res://ui/main_menu.gd, line ${errorLine}`;
      run.note(`ui/main_menu.gd edited by hand: line ${errorLine} assigns a String to an int`);
      const headBefore = git(p(), "rev-parse", "HEAD").trim();

      await clickWhenEnabled(driver, tid("game-play"));
      await waitUntil(async () => (await visibleTexts(driver, tid("game-error"))).some((e) => e.includes(where)), {
        timeoutMs: 60_000,
        what: `an error row showing ${where}`,
      });
      const t0 = Date.now();
      await run.shot("error-before-auto-fix");
      // Nobody presses anything: the fix starts by itself.
      await waitAttr(driver, tid("chat-panel"), "data-busy", "true", { timeoutMs: 30_000 });
      run.note(`an AI turn started by itself ${((Date.now() - t0) / 1000).toFixed(1)}s after the error showed`);
      await waitVisible(driver, By.css('[data-testid="system-note"][data-origin="auto_fix"]'), { timeoutMs: 30_000 });
      const banner = await driver.findElements(tid("autofix-banner"));
      run.note(`banner: ${banner.length > 0 ? (await banner[0].getText()).replace(/\s+/g, " ") : "none"}`);
      await run.shot("auto-fix-running");
      const fixMessage = chatRecords(p()).filter((r) => r.kind === "user").pop();
      run.note(`fix message (origin ${fixMessage?.origin}): ${JSON.stringify(fixMessage?.text.slice(0, 240))}`);
      if (fixMessage?.origin !== "auto_fix") throw new Error("the turn that started isn't an auto-fix");
      if (!fixMessage.text.includes("main_menu.gd")) throw new Error("the auto-fix message doesn't name main_menu.gd");
      // A second attempt may follow if errors remain; wait until the chat
      // stays quiet.
      for (let i = 0; i < 3; i++) {
        await waitForTurn(run, driver, { startedBy: "the auto-fix" });
        await new Promise((r) => setTimeout(r, 6000));
        if ((await driver.findElement(tid("chat-panel")).getAttribute("data-busy")) !== "true") break;
        run.note("another auto-fix attempt started");
      }
      const subjects = snapshotSubjects(p());
      run.note(`git log: ${JSON.stringify(subjects.slice(0, 3))}`);
      if (git(p(), "rev-parse", "HEAD").trim() === headBefore) {
        // Undoing my hand edit exactly (the usual fix: delete the bad
        // function) leaves the project identical to its last snapshot, and
        // then no snapshot is made, by design. That's fine only if nothing
        // outside the chat is left uncommitted and main_menu.gd is back as
        // committed; anything else means the fix's edits were lost.
        const dirty = git(p(), "status", "--porcelain", "--", ".", ":!.ibproject/chat", ":!.ibproject/boards").trim();
        if (dirty) throw new Error(`the fix made no snapshot and left changes uncommitted:\n${dirty}`);
        run.note("the fix put main_menu.gd back exactly as committed, so no snapshot was needed");
      } else if (subjects[0] !== "Automatic fix for game errors") {
        run.note(`(newest snapshot is "${subjects[0]}")`);
      }
      await ensureGameRunning(run, driver, p());
      await new Promise((r) => setTimeout(r, 4000));
      const errors = await ownErrors(driver);
      run.note(`error rows about the project's files: ${JSON.stringify(errors)}`);
      if (errors.some((e) => e.includes("main_menu.gd"))) throw new Error("the main_menu.gd error is still there");
      if (errors.length > 0) throw new Error("errors in the game's own files remain");
      await shootGame(run, p(), "game-after-auto-fix");
      await run.shot("studio-after-auto-fix");
      const stopNow = await driver.findElements(tid("game-stop"));
      if (stopNow.length > 0 && (await stopNow[0].isEnabled())) await stopNow[0].click();
    }, { needs: ["F10"], after: saveGameLog("F11") });
  }

  await run.step("F12", "Studio settings: always plan first by default; Teach me is saved to .ibproject/settings.json", async () => {
    await waitAttr(driver, tid("chat-panel"), "data-busy", "false", { timeoutMs: 60_000 });
    const before = await useStudioSettings(driver, { teach: true });
    run.note(`settings shown: ${JSON.stringify(before)}`);
    if (before.plan !== "always_plan") throw new Error(`the plan policy shows ${before.plan}, not "always plan first"`);
    const saved = projectSettingsOnDisk(p());
    run.note(`.ibproject/settings.json: ${JSON.stringify(saved)}`);
    if (saved?.teach !== true) throw new Error("Teach me wasn't saved");
    // Opened again, it reads the file back.
    const again = await useStudioSettings(driver, { teach: false, plan: "small_changes_direct" });
    if (!again.teach) throw new Error("Teach me didn't read back as on");
    const after = projectSettingsOnDisk(p());
    run.note(`after switching Teach me off and small changes on: ${JSON.stringify(after)}`);
    if (after?.teach !== false || after?.plan_policy !== "small_changes_direct") throw new Error("the second change wasn't saved");
    await useStudioSettings(driver, { plan: "always_plan" });
    await run.shot("studio-settings");
  }, { needs: ["F7"] });

  await run.step("F13", "The sidebar has the six sections and Settings; Context lists the game's cards", async () => {
    const ids = ["home", "studio", "context", "assets", "launch", "code", "settings"];
    const labels = [];
    for (const id of ids) {
      const el = await waitVisible(driver, tid(`nav-${id}`));
      labels.push((await driver.executeScript("return arguments[0].innerText", el)).trim());
    }
    run.note(`sidebar: ${JSON.stringify(labels)}`);
    const all = await driver.executeScript(`return [...document.querySelectorAll('[data-testid^="nav-"]')].map((e) => e.dataset.testid)`);
    if (all.length !== ids.length) throw new Error(`the sidebar has ${all.length} sections: ${JSON.stringify(all)}`);
    await clickWhenEnabled(driver, tid("nav-context"));
    const section = await waitVisible(driver, tid("context-section"));
    await waitUntil(async () => !(await isInert(driver, section)), { what: "Context to become the active section" });
    // The starter Context is placed on the Documents canvas: its documents as blocks, its folders as boards.
    await waitVisible(driver, tid("canvas"));
    const text = await waitUntil(
      async () => {
        const t = await driver.executeScript("return arguments[0].innerText + '\\n' + [...arguments[0].querySelectorAll('input')].map((i) => i.value).join('\\n')", section);
        return [GAME_NAME, "systems", "tasks"].every((n) => t.toLowerCase().includes(n.toLowerCase())) ? t : null;
      },
      { timeoutMs: 20_000, what: "the starter documents to be on the canvas" },
    );
    run.note(`Documents shows: ${JSON.stringify(text.split("\n").filter(Boolean).slice(0, 12))}`);
    await run.shot("context-cards");
    for (const id of ["assets", "launch", "code", "settings"]) {
      await clickWhenEnabled(driver, tid(`nav-${id}`));
      await new Promise((r) => setTimeout(r, 600));
      await run.shot(`section-${id}`);
    }
  }, { needs: ["F7"] });

  await run.step("F14", "Back on Home, the checklist is gone: the game is listed with New game", async () => {
    await clickWhenEnabled(driver, tid("nav-home"));
    await waitVisible(driver, tid("new-game"), { timeoutMs: 30_000 });
    const checklist = await driver.findElements(tid("first-run-checklist"));
    if (checklist.length > 0) throw new Error("the first-run checklist still shows after the first game was made");
    const home = await driver.executeScript("return document.body.innerText");
    if (!home.includes(GAME_NAME)) throw new Error(`Home doesn't list ${GAME_NAME}`);
    run.note(`Home lists ${GAME_NAME}; status: ${home.split("\n").filter((l) => /^(AI|Godot)$|Claude Code ·|Godot 4/.test(l.trim())).join(" | ")}`);
  }, { needs: ["F7"] });
}
