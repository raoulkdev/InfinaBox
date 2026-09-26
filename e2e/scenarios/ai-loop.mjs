// Exit-criterion steps 4–6 with the real AI (`--real-ai`): a real chat turn
// through the user's own `claude` CLI changes the game, is snapshotted, and
// restarts it; "Undo last change" takes it back out; and a hand-made script
// error goes to the AI through "Ask AI to fix", which the AI then fixes.
// Runs after core-loop.mjs's steps 1–3 (project created, Godot there, the
// game runs), sharing their state.
//
// Nothing here stands in for the AI: the turn, its tools (the InfinaBox MCP
// server the app launches as `<app> --mcp-server`, reaching the game over
// the app's bridge), the snapshot and the restart are all the app's own.

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { By, Key } from "selenium-webdriver";
import { clickWhenEnabled, textOf, tid, waitAttr, waitUntil, waitVisible } from "../lib/ui.mjs";
import { gamePids, gameProcesses, gameWindows, git, visibleTexts } from "./common.mjs";

export const AI_REQUEST = "make the background dark blue and add a label that says Hello";
const READY_LINE = "[infinabox] ready 1";
// A real turn reads the project, edits files, runs the game and checks it.
const TURN_TIMEOUT_MS = 5 * 60_000;
const COMPOSER = By.css('textarea[aria-label="Message the AI"]');
const SEND = By.css('button[aria-label="Send"]');
// Everything but the chat, which a snapshot always carries along and a
// restore never rewinds (see crates/core/src/snapshot.rs).
const NOT_CHAT = [".", ":(exclude).ibproject/chat"];

/** The History title an AI turn's snapshot gets: the message's first
 * non-empty line, cut to about 60 characters on a word boundary. Mirrors
 * `snapshot_title` in src-tauri/src/commands/agent.rs. */
export function snapshotTitleFor(message) {
  const MAX = 60;
  const line = message.split("\n").map((l) => l.trim()).find(Boolean) ?? "";
  if (!line) return "AI change";
  const chars = [...line];
  if (chars.length <= MAX) return line;
  let cut = chars.slice(0, MAX - 1).join("");
  const space = cut.lastIndexOf(" ");
  if (space >= MAX / 2) cut = cut.slice(0, space);
  return `${cut.trimEnd()}…`;
}

/** Every record of every chat thread saved in the project. */
function chatRecords(project) {
  const dir = path.join(project, ".ibproject/chat");
  if (!fs.existsSync(dir)) return [];
  return fs
    .readdirSync(dir)
    .filter((f) => f.endsWith(".jsonl"))
    .flatMap((f) =>
      fs
        .readFileSync(path.join(dir, f), "utf8")
        .split("\n")
        .filter(Boolean)
        .map((l) => ({ file: f, ...JSON.parse(l) })),
    );
}

/** The events of the last turn in the saved chat (after the last user message). */
function lastTurnEvents(project) {
  const records = chatRecords(project);
  const lastUser = records.map((r) => r.kind).lastIndexOf("user");
  return records.slice(lastUser + 1).filter((r) => r.kind === "event").map((r) => r.event);
}

/** "name → ok|failed: summary" for every tool the turn used. */
function toolLines(events) {
  const results = new Map(events.filter((e) => e.type === "tool_result").map((e) => [e.id, e]));
  return events
    .filter((e) => e.type === "tool_use")
    .map((e) => {
      const r = results.get(e.id);
      return `${e.name} (${e.summary}) → ${r ? `${r.ok ? "ok" : "failed"}: ${r.summary}` : "no result"}`;
    });
}

/** Average colour of a PNG, as [r, g, b] 0–255 (ImageMagick). */
function averageColor(file) {
  try {
    const out = execFileSync("convert", [file, "-resize", "1x1!", "-format", "%[fx:int(255*r)],%[fx:int(255*g)],%[fx:int(255*b)]", "info:"]).toString();
    return out.trim().split(",").map(Number);
  } catch {
    return null;
  }
}

/** Waits for the chat to be idle and ready, types `text`, and sends it. */
async function sendChat(driver, text) {
  await waitAttr(driver, tid("chat-panel"), "data-ready", "true", { timeoutMs: 30_000 });
  await waitAttr(driver, tid("chat-panel"), "data-busy", "false", { timeoutMs: 30_000 });
  const composer = await waitVisible(driver, COMPOSER);
  await composer.click();
  await composer.sendKeys(text);
  await clickWhenEnabled(driver, SEND);
}

/**
 * Waits for the AI turn that was just started to finish: the chat goes
 * busy, then (once the backend's `agent-turn-finished` arrives) idle
 * again. Returns the transcript items after the last user message.
 */
async function waitForTurn(run, driver, { startedBy }) {
  const t0 = Date.now();
  // The busy flag is set synchronously on send; seeing it confirms the
  // message went to the AI rather than into a draft.
  await waitAttr(driver, tid("chat-panel"), "data-busy", "true", { timeoutMs: 10_000 }).catch(() => {
    run.note(`(didn't catch the chat going busy after ${startedBy}; it may have finished already)`);
  });
  await waitAttr(driver, tid("chat-panel"), "data-busy", "false", { timeoutMs: TURN_TIMEOUT_MS });
  run.note(`AI turn finished after ${((Date.now() - t0) / 1000).toFixed(0)}s`);
  const items = await driver.executeScript(`
    const all = [...document.querySelectorAll('[data-testid="chat-item"]')].map((el) => ({
      kind: el.dataset.kind,
      text: (el.innerText || el.textContent || '').trim(),
    }));
    const lastUser = all.map((i) => i.kind).lastIndexOf('user');
    return all.slice(lastUser + 1);
  `);
  for (const i of items) run.note(`chat ${i.kind}: ${i.text.replace(/\s+/g, " ").slice(0, 300)}`);
  const errors = items.filter((i) => i.kind === "error");
  if (errors.length > 0) throw new Error(`the AI turn ended with an error: ${errors.map((e) => e.text).join(" | ")}`);
  if (!items.some((i) => i.kind === "assistant")) throw new Error("the AI turn finished without any reply text");
  return items;
}

/** Waits until the game runs under a process id that's not in `before`,
 * and has printed its ready line. */
async function waitForRestart(run, driver, project, before, what) {
  const now = await waitUntil(
    () => {
      const pids = gamePids(project);
      return pids.length > 0 && pids.every((p) => !before.includes(p)) ? pids : null;
    },
    { timeoutMs: 120_000, what },
  );
  run.note(`game restarted: pid ${before.join(",") || "none"} → ${now.join(",")}`);
  await waitAttr(driver, tid("play-panel"), "data-game-state", "running", { timeoutMs: 90_000 });
  await waitUntil(async () => (await textOf(driver, tid("game-output"))).includes(READY_LINE), {
    timeoutMs: 60_000,
    what: `"${READY_LINE}" after the restart`,
  });
  return now;
}

/** Screenshot of the game's own window, with its average colour noted. */
async function shootGame(run, project, label) {
  const windows = await waitUntil(() => gameWindows(project).length > 0 && gameWindows(project), {
    timeoutMs: 30_000,
    what: "the game window",
  });
  // Let the renderer draw a few frames first.
  await new Promise((r) => setTimeout(r, 2000));
  const file = await run.shot(label, windows[0].id);
  const avg = averageColor(file);
  if (avg) run.note(`game window ${label}: average colour rgb(${avg.join(", ")})`);
  return avg;
}

/** Lines a commit range added, outside the chat. */
function addedLines(project, from, to) {
  return git(project, "diff", "--unified=0", from, to, "--", ...NOT_CHAT)
    .split("\n")
    .filter((l) => l.startsWith("+") && !l.startsWith("+++"));
}

export async function aiLoop(run, app, config, state, saveGameLog) {
  const { driver } = app;
  const p = () => state.project;

  await run.step(
    "4",
    `Real AI turn: "${AI_REQUEST}" changes the game, is snapshotted with its chat, and the game restarts`,
    async () => {
      // The game is running when the request goes in, as a user would have it.
      if (gamePids(p()).length === 0) {
        await clickWhenEnabled(driver, tid("game-play"));
        await waitAttr(driver, tid("play-panel"), "data-game-state", "running", { timeoutMs: 90_000 });
        await waitUntil(async () => (await textOf(driver, tid("game-output"))).includes(READY_LINE), {
          timeoutMs: 60_000,
          what: `"${READY_LINE}"`,
        });
      }
      state.beforeAiCommit = git(p(), "rev-parse", "HEAD").trim();
      const pidsBefore = gamePids(p());
      run.note(`before: HEAD ${state.beforeAiCommit.slice(0, 8)}, game pid ${pidsBefore.join(",")}`);

      await sendChat(driver, AI_REQUEST);
      await run.shot("ai-turn-started");
      await waitForTurn(run, driver, { startedBy: "typing the request" });

      const events = lastTurnEvents(p());
      const tools = toolLines(events);
      state.toolNames = [...(state.toolNames ?? []), ...events.filter((e) => e.type === "tool_use").map((e) => e.name)];
      run.attach("ai-turn-tools", tools.join("\n"));
      for (const t of tools) run.note(`tool: ${t.slice(0, 240)}`);

      const want = snapshotTitleFor(AI_REQUEST);
      await waitUntil(async () => (await visibleTexts(driver, tid("snapshot-title")))[0] === want, {
        timeoutMs: 30_000,
        what: `History to list "${want}" on top`,
      });
      run.note(`History shows "${want}" on top`);
      state.aiCommit = git(p(), "rev-parse", "HEAD").trim();
      const subject = git(p(), "log", "-1", "--format=%s%n%b", state.aiCommit).trim();
      run.note(`snapshot commit ${state.aiCommit.slice(0, 8)}: ${subject.replace(/\n+/g, " | ")}`);
      if (state.aiCommit === state.beforeAiCommit) throw new Error("no new snapshot commit");

      const changed = git(p(), "diff", "--name-only", state.beforeAiCommit, state.aiCommit).trim().split("\n").filter(Boolean);
      run.note(`files in the snapshot: ${JSON.stringify(changed)}`);
      const chatFiles = changed.filter((f) => f.startsWith(".ibproject/chat/"));
      if (chatFiles.length === 0) throw new Error("the snapshot doesn't carry the chat file");
      const dirtyChat = git(p(), "status", "--porcelain", "--", ".ibproject/chat").trim();
      if (dirtyChat) throw new Error(`the chat has changes the snapshot didn't commit:\n${dirtyChat}`);
      run.note(`chat committed: ${chatFiles.join(", ")}; nothing uncommitted under .ibproject/chat`);

      const added = addedLines(p(), state.beforeAiCommit, state.aiCommit);
      run.attach("ai-change-diff", git(p(), "diff", state.beforeAiCommit, state.aiCommit, "--", ...NOT_CHAT));
      const hasLabel = added.some((l) => /\bLabel\b/.test(l));
      const hasHello = added.some((l) => /Hello/.test(l));
      const colorLines = added.filter((l) => /clear_color|Color\(|ColorRect|color/i.test(l));
      run.note(`added lines: Label ${hasLabel}, "Hello" ${hasHello}, colour lines: ${JSON.stringify(colorLines.slice(0, 4))}`);
      if (!hasLabel || !hasHello) throw new Error("the change on disk has no Label saying Hello");
      if (colorLines.length === 0) throw new Error("the change on disk sets no colour");

      await waitForRestart(run, driver, p(), pidsBefore, "the game to restart with the AI's change");
      await new Promise((r) => setTimeout(r, 2000));
      const errors = await visibleTexts(driver, tid("game-error"));
      run.note(`error rows after the restart: ${JSON.stringify(errors)}`);
      state.aiColor = await shootGame(run, p(), "game-after-ai");
      if (state.aiColor) {
        const [r, g, b] = state.aiColor;
        run.note(b > r && b > g ? "the game window is predominantly blue" : "the game window isn't predominantly blue");
      }
      state.aiFiles = Object.fromEntries(
        changed.filter((f) => !f.startsWith(".ibproject/chat/")).map((f) => [f, fs.existsSync(path.join(p(), f)) ? fs.readFileSync(path.join(p(), f), "utf8") : null]),
      );
      // Open the turn's work row so the screenshot shows the tools it used.
      const work = await driver.findElements(By.css('[data-testid="chat-item"][data-kind="work"] button'));
      if (work.length > 0) await work[work.length - 1].click().catch(() => {});
      await run.shot("ai-turn-transcript");
    },
    { needs: ["3"], after: saveGameLog("4") },
  );

  await run.step(
    "5",
    '"Undo last change" takes the AI\'s change back out and restarts the game without it',
    async () => {
      const before = gamePids(p());
      if (before.length === 0) throw new Error("expected the game to be running before undo");
      await clickWhenEnabled(driver, tid("undo-last"));
      const notice = await waitVisible(driver, tid("history-notice"), { timeoutMs: 30_000 });
      run.note(`notice: ${await notice.getText()}`);
      const diff = git(p(), "diff", "--stat", state.beforeAiCommit, "HEAD", "--", ...NOT_CHAT).trim();
      if (diff) throw new Error(`the project (chat aside) doesn't match the state before the AI's change:\n${diff}`);
      const dirty = git(p(), "status", "--porcelain").trim();
      if (dirty) throw new Error(`working tree not clean after undo:\n${dirty}`);
      run.note("project files (chat aside) match the state before the AI's change; working tree clean");
      run.note(`git log: ${JSON.stringify(git(p(), "log", "--format=%s").trim().split("\n"))}`);
      const chatKept = chatRecords(p()).some((r) => r.kind === "user" && r.text === AI_REQUEST);
      run.note(`the chat still holds the request: ${chatKept}`);
      if (!chatKept) throw new Error("undo rewound the chat");
      await waitForRestart(run, driver, p(), before, "the game to restart after undo");
      await new Promise((r) => setTimeout(r, 2000));
      const errors = await visibleTexts(driver, tid("game-error"));
      if (errors.length > 0) throw new Error(`errors listed after undo: ${JSON.stringify(errors)}`);
      const avg = await shootGame(run, p(), "game-after-undo");
      if (avg && state.aiColor) run.note(`average colour ${state.aiColor.join(",")} → ${avg.join(",")}`);
    },
    { needs: ["4"], after: saveGameLog("5") },
  );

  await run.step(
    "6",
    'A hand-made script error shows in the Play panel; "Ask AI to fix" sends it and the AI fixes it',
    async () => {
      await clickWhenEnabled(driver, tid("game-stop"));
      await waitAttr(driver, tid("play-panel"), "data-game-state", "stopped", { timeoutMs: 30_000 });
      const file = path.join(p(), "player.gd");
      const lines = state.originalPlayer.replace(/\n$/, "").split("\n");
      lines.push("", "", "func _describe_speed() -> void:", '\tvar label_speed: int = "fast"', "\tprint(label_speed)");
      const errorLine = lines.length - 1;
      state.brokenPlayer = `${lines.join("\n")}\n`;
      fs.writeFileSync(file, state.brokenPlayer);
      run.note(`player.gd edited by hand: line ${errorLine} assigns a String to an int`);

      await clickWhenEnabled(driver, tid("game-play"));
      await waitAttr(driver, tid("play-panel"), "data-game-state", "running", { timeoutMs: 90_000 });
      const where = `res://player.gd, line ${errorLine}`;
      await waitUntil(async () => (await visibleTexts(driver, tid("game-error"))).some((e) => e.includes(where)), {
        timeoutMs: 60_000,
        what: `an error row showing ${where}`,
      });
      for (const e of await visibleTexts(driver, tid("game-error"))) run.note(`error row: ${e.replace(/\n/g, " | ")}`);
      await run.shot("error-in-play-panel");
      const pidsBefore = gamePids(p());
      const headBefore = git(p(), "rev-parse", "HEAD").trim();

      let askButton = null;
      for (const row of await driver.findElements(tid("game-error"))) {
        const t = await driver.executeScript("return arguments[0].textContent", row);
        if (t.includes(where)) askButton = await row.findElement(tid("ask-ai-to-fix"));
      }
      if (!askButton) throw new Error(`no "Ask AI to fix" on the ${where} row`);
      await waitAttr(driver, tid("chat-panel"), "data-busy", "false");
      await askButton.click();
      const outcome = await waitUntil(async () => (await askButton.getAttribute("data-outcome")) || null, {
        what: "the Ask AI to fix button to report what happened",
      });
      const label = (await askButton.getText()).trim();
      run.note(`Ask AI to fix → ${outcome}: "${label}"`);
      if (outcome !== "sent") throw new Error(`the fix request was ${outcome}, not sent`);
      const draft = await (await driver.findElement(COMPOSER)).getAttribute("value");
      if (draft.includes(where)) throw new Error("the fix request is sitting in the chat box as a draft");
      await run.shot("fix-request-sent");

      await waitForTurn(run, driver, { startedBy: "Ask AI to fix" });
      const events = lastTurnEvents(p());
      const tools = toolLines(events);
      state.toolNames = [...(state.toolNames ?? []), ...events.filter((e) => e.type === "tool_use").map((e) => e.name)];
      run.attach("fix-turn-tools", tools.join("\n"));
      for (const t of tools) run.note(`tool: ${t.slice(0, 240)}`);
      const userText = chatRecords(p()).filter((r) => r.kind === "user").pop()?.text ?? "";
      run.note(`message the AI got: ${JSON.stringify(userText.slice(0, 200))}`);
      if (!userText.includes(where)) throw new Error(`the saved chat's last message doesn't mention ${where}`);

      const onDisk = fs.readFileSync(file, "utf8");
      run.note(onDisk === state.brokenPlayer ? "player.gd unchanged by the AI" : "player.gd changed by the AI");
      run.attach("fix-diff", git(p(), "diff", headBefore, "HEAD", "--", ...NOT_CHAT));
      const titles = await visibleTexts(driver, tid("snapshot-title"));
      run.note(`History top: ${JSON.stringify(titles.slice(0, 3))}`);

      // The game was running, so the turn's change restarts it.
      await waitForRestart(run, driver, p(), pidsBefore, "the game to restart after the fix");
      // Give the restarted game a moment to report any error it has.
      await new Promise((r) => setTimeout(r, 3000));
      const errors = await visibleTexts(driver, tid("game-error"));
      run.note(`error rows after the fix: ${JSON.stringify(errors)}`);
      await shootGame(run, p(), "game-after-fix");
      if (errors.some((e) => e.includes("res://"))) throw new Error("errors in the game's own files remain after the AI's fix");
    },
    { needs: ["5"], after: saveGameLog("6") },
  );

  await run.step(
    "5b",
    `"Go back" to the AI's change restores it (confirmed through the dialog)`,
    async () => {
      const want = snapshotTitleFor(AI_REQUEST);
      let target = null;
      for (const row of await driver.findElements(tid("snapshot-row"))) {
        const id = await row.getAttribute("data-snapshot-id");
        if (id === state.aiCommit) target = row;
      }
      if (!target) throw new Error(`no History row for the AI's snapshot "${want}"`);
      const before = gamePids(p());
      await driver.executeScript("arguments[0].scrollIntoView({block: 'center'})", target);
      await target.findElement(tid("snapshot-go-back")).click();
      const dialog = await waitVisible(driver, By.css('[role="alertdialog"]'));
      run.note(`dialog: ${(await dialog.getText()).split("\n")[0]}`);
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
      const diff = git(p(), "diff", "--stat", state.aiCommit, "HEAD", "--", ...NOT_CHAT).trim();
      if (diff) throw new Error(`the project (chat aside) doesn't match the AI's snapshot:\n${diff}`);
      run.note("project files (chat aside) match the AI's snapshot again");
      if (before.length > 0) {
        await waitForRestart(run, driver, p(), before, "the game to restart after going back");
        await shootGame(run, p(), "game-after-go-back");
      }
      await clickWhenEnabled(driver, tid("game-stop"));
      await waitAttr(driver, tid("play-panel"), "data-game-state", "stopped", { timeoutMs: 30_000 });
    },
    { needs: ["4"], after: saveGameLog("5b") },
  );

  await run.step("mcp", "The AI used the InfinaBox tools (MCP server + bridge)", async () => {
    const all = chatRecords(p()).filter((r) => r.kind === "event" && r.event.type === "tool_use").map((r) => r.event.name);
    const counts = {};
    for (const n of all) counts[n] = (counts[n] ?? 0) + 1;
    run.note(`tools used across all turns: ${JSON.stringify(counts)}`);
    const ours = all.filter((n) => n.startsWith("mcp__infinabox__"));
    if (ours.length === 0) throw new Error("the AI never called an InfinaBox (mcp__infinabox__*) tool");
    // A game tool's result proves the MCP server reached the app's bridge.
    const events = chatRecords(p()).filter((r) => r.kind === "event").map((r) => r.event);
    const results = new Map(events.filter((e) => e.type === "tool_result").map((e) => [e.id, e]));
    const game = events.filter((e) => e.type === "tool_use" && /mcp__infinabox__(run_game|get_game_errors|get_game_status|get_game_output|stop_game)/.test(e.name));
    for (const e of game) {
      const r = results.get(e.id);
      run.note(`${e.name}: ${r ? `${r.ok ? "ok" : "FAILED"} — ${r.summary.replace(/\s+/g, " ").slice(0, 200)}` : "no result"}`);
    }
    if (game.length === 0) throw new Error("the AI never used a game tool (run_game / get_game_errors …), so the bridge went unexercised");
    if (!game.some((e) => results.get(e.id)?.ok)) throw new Error("no game tool call succeeded");
    // The saved transcript, for the record.
    for (const f of fs.readdirSync(path.join(p(), ".ibproject/chat"))) {
      run.attach(`chat-${f}`, fs.readFileSync(path.join(p(), ".ibproject/chat", f), "utf8"));
    }
    run.note(`game processes now: ${JSON.stringify(gameProcesses(p()).map((g) => g.pid))}`);
  }, { needs: ["4"] });

  await run.step(
    "stop",
    "While a turn runs History holds off undo; Stop keeps the chat on \"Stopping…\" until the turn really ends, then a new message goes through",
    async () => {
      await sendChat(driver, "Read every file in this project and describe each one in detail, one paragraph per file.");
      await waitAttr(driver, tid("chat-panel"), "data-busy", "true", { timeoutMs: 10_000 });
      // History: undo and go back are off, and say why.
      await waitVisible(driver, tid("history-ai-working"));
      const undoEnabled = await (await driver.findElement(tid("undo-last"))).isEnabled();
      const goBackEnabled = await driver.executeScript(
        `return [...document.querySelectorAll('[data-testid="snapshot-go-back"]')].some((b) => !b.disabled)`,
      );
      const why = (await textOf(driver, tid("history-ai-working"))).trim();
      if (why !== "Wait for the AI to finish") throw new Error(`History's reason reads ${JSON.stringify(why)}`);
      run.note(`during the turn: undo enabled ${undoEnabled}, any go back enabled ${goBackEnabled}, History says "${why}"`);
      if (undoEnabled || goBackEnabled) throw new Error("History allowed undo/go back while the AI was working");
      await run.shot("history-while-ai-works");

      // Let the AI get going, then stop it.
      await new Promise((r) => setTimeout(r, 4000));
      if ((await driver.findElement(tid("chat-panel")).getAttribute("data-busy")) !== "true") {
        run.note("the turn finished before Stop could be pressed; Stop not exercised");
        return;
      }
      const stop = await driver.findElement(By.xpath('//*[@data-testid="chat-panel"]//button[normalize-space()="Stop"]'));
      const t0 = Date.now();
      await stop.click();
      const stopping = await waitUntil(
        async () => {
          const busy = await driver.findElement(tid("chat-panel")).getAttribute("data-busy");
          const buttons = await driver.findElements(By.xpath('//*[@data-testid="chat-panel"]//button[normalize-space()="Stopping…"]'));
          return busy === "true" && buttons.length > 0 ? "shown" : busy === "false" ? "already-finished" : null;
        },
        { timeoutMs: 5000, what: '"Stopping…" in the composer' },
      );
      run.note(`after Stop: ${stopping === "shown" ? '"Stopping…" shown while the turn winds down' : "the turn had already finished"}`);
      await run.shot("chat-stopping");
      await waitAttr(driver, tid("chat-panel"), "data-busy", "false", { timeoutMs: 30_000 });
      run.note(`the chat freed itself ${((Date.now() - t0) / 1000).toFixed(1)}s after Stop`);

      // Straight away: a new message must be taken, not refused as "still working".
      await sendChat(driver, "Reply with just the word: ok");
      await waitForTurn(run, driver, { startedBy: "the message right after Stop" });
    },
    { needs: ["4"] },
  );
}
