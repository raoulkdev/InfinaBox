// Phase C: the whole-game features, driven through the real app with fake
// providers. The fakes are local HTTP servers; nothing here reaches a real
// service, so what it proves is InfinaBox's own behaviour (files, cards,
// license records, keychain-free key handling, the API runtime's tool loop),
// not that any provider's real API matches.

import fs from "node:fs";
import http from "node:http";
import path from "node:path";
import zlib from "node:zlib";
import { By, Key } from "selenium-webdriver";
import { clickWhenEnabled, invokeCommand, tid, waitUntil, waitVisible } from "../lib/ui.mjs";
import { createProjectFromHome, git, snapshotSubjects, tempParent } from "./common.mjs";

const SECRET = "test-key-PHASEC-do-not-log";

function crc32(buf) {
  let c;
  let crc = 0xffffffff;
  for (const b of buf) {
    c = (crc ^ b) & 0xff;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    crc = (crc >>> 8) ^ c;
  }
  return (crc ^ 0xffffffff) >>> 0;
}

/** A solid-colour PNG, made here so the fake needs no image library. */
export function makePng(width, height, [r, g, b]) {
  const chunk = (type, data) => {
    const body = Buffer.concat([Buffer.from(type), data]);
    const out = Buffer.alloc(body.length + 8);
    out.writeUInt32BE(data.length, 0);
    body.copy(out, 4);
    out.writeUInt32BE(crc32(body), body.length + 4);
    return out;
  };
  const header = Buffer.alloc(13);
  header.writeUInt32BE(width, 0);
  header.writeUInt32BE(height, 4);
  header[8] = 8;
  header[9] = 2;
  const row = Buffer.concat([Buffer.from([0]), Buffer.from(Array.from({ length: width }, () => [r, g, b]).flat())]);
  const raw = Buffer.concat(Array.from({ length: height }, () => row));
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", header),
    chunk("IDAT", zlib.deflateSync(raw)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

function listen(handler) {
  return new Promise((resolve) => {
    const server = http.createServer(handler);
    server.listen(0, "127.0.0.1", () => resolve({ server, port: server.address().port }));
  });
}

/** Starts the fake Cloudflare and the fake OpenAI-compatible server. Returns
 * their ports and what they saw. Call before launching the app (the Cloudflare
 * base URL is an environment variable). */
export async function startFakes() {
  const seen = { cloudflare: [], chat: [] };
  const cloudflare = await listen((req, res) => {
    let body = "";
    req.on("data", (d) => (body += d));
    req.on("end", () => {
      seen.cloudflare.push({ url: req.url, auth: req.headers.authorization, body });
      const png = makePng(64, 64, [200, 40, 40]).toString("base64");
      res.setHeader("content-type", "application/json");
      res.end(JSON.stringify({ result: { image: png }, success: true, errors: [], messages: [] }));
    });
  });
  const chat = await listen((req, res) => {
    let body = "";
    req.on("data", (d) => (body += d));
    req.on("end", () => {
      const parsed = body ? JSON.parse(body) : {};
      seen.chat.push({ url: req.url, auth: req.headers.authorization ?? null, parsed });
      res.setHeader("content-type", "application/json");
      if (req.url.endsWith("/models")) return res.end(JSON.stringify({ data: [{ id: "fake-model" }] }));
      const sawToolResult = (parsed.messages ?? []).some((m) => m.role === "tool");
      const wantsNote = JSON.stringify(parsed.messages ?? []).includes("save a note");
      const message = !wantsNote
        ? { role: "assistant", content: "hello" }
        : sawToolResult
        ? { role: "assistant", content: "I saved a note file for you." }
        : {
            role: "assistant",
            content: null,
            tool_calls: [
              {
                id: "call_1",
                type: "function",
                function: { name: "write_file", arguments: JSON.stringify({ path: "notes.txt", content: "hello from the fake model\n" }) },
              },
            ],
          };
      res.end(
        JSON.stringify({
          choices: [{ index: 0, message, finish_reason: message.tool_calls ? "tool_calls" : "stop" }],
          usage: { prompt_tokens: 10, completion_tokens: 5, total_tokens: 15 },
        }),
      );
    });
  });
  return { seen, cloudflarePort: cloudflare.port, chatPort: chat.port, close: () => [cloudflare, chat].forEach((s) => s.server.close()) };
}

function walk(dir, out = []) {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    if (e.name === ".git" || e.name === ".godot") continue;
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, out);
    else out.push(p);
  }
  return out;
}

export async function phaseC(run, app, config, fakes) {
  const { driver } = app;
  const state = { project: null };
  const call = (cmd, args) => invokeCommand(driver, cmd, args);

  await run.step("c1", "A new project starts with a Context of typed cards; the Context section lists them", async () => {
    await waitVisible(driver, tid("new-project"), { timeoutMs: 30_000 });
    state.project = await createProjectFromHome(run, driver, tempParent("phase-c"), "Phase C Game");
    const templates = await call("onboarding_templates", {});
    run.note(`templates: ${templates.map((t) => `${t.id} (${t.dimension})`).join(", ")}`);
    if (templates.length < 8) throw new Error(`expected 8 templates, got ${templates.length}`);
    await call("context_write", {
      projectPath: state.project,
      path: "mechanics/jumping.md",
      meta: { type: "mechanic", title: "Jumping", status: "working", links: ["concept.md"], implemented_in: ["player.gd"], tags: [], extra: {} },
      body: "# Jumping\n\nHold the button to jump higher.\n",
    });
    await call("context_write", {
      projectPath: state.project,
      path: "tasks/add-coins.md",
      meta: { type: "task", title: "Add coins", status: "todo", links: ["mechanics/jumping.md", "nope/missing.md"], implemented_in: [], tags: [], extra: {} },
      body: "Coins to collect.\n",
    });
    const cards = await call("context_list", { projectPath: state.project });
    const jumping = cards.find((c) => c.path === "mechanics/jumping.md");
    if (!jumping) throw new Error(`jumping card missing from ${JSON.stringify(cards.map((c) => c.path))}`);
    const task = cards.find((c) => c.path === "tasks/add-coins.md");
    if (!task.broken_links.includes("nope/missing.md")) throw new Error("broken link not reported");
    if (!jumping.backlinks.includes("tasks/add-coins.md")) throw new Error("backlink not reported");
    await call("context_set_status", { projectPath: state.project, path: "tasks/add-coins.md", status: "doing" });
    const board = await call("context_board", { projectPath: state.project, types: ["task"] });
    const doing = board.columns.find((c) => c.status === "doing");
    if (!doing?.cards.some((c) => c.path === "tasks/add-coins.md")) throw new Error(`task not on the Doing column: ${JSON.stringify(board)}`);
    const graph = await call("context_graph", { projectPath: state.project });
    if (!graph.edges.some((e) => e.broken)) throw new Error("graph shows no broken edge");
    run.note(`${cards.length} cards; board, backlinks, broken link and graph all reported by the real commands`);
    await clickWhenEnabled(driver, tid("nav-context"));
    await waitVisible(driver, tid("context-section"));
    await waitVisible(driver, tid("canvas"));
    await waitUntil(async () => (await driver.findElements(tid("block-doc"))).length >= 1, { what: "the canvas to show the project's document" });
    await run.shot("context-cards");
  });

  await run.step("c2", "Importing an image records its license; Assets shows it, health flags gaps, credits list it", async () => {
    const tmp = path.join(path.dirname(state.project), "incoming");
    fs.mkdirSync(tmp, { recursive: true });
    const src = path.join(tmp, "hero.png");
    fs.writeFileSync(src, makePng(32, 48, [10, 120, 240]));
    const info = await call("assets_import", {
      projectPath: state.project,
      source: src,
      destSubdir: null,
      title: "Hero",
      license: { name: "CC0-1.0", source: "My own files: incoming", author: "Me", url: null, generated_by: null },
    });
    if (info.width !== 32 || info.height !== 48) throw new Error(`image size not read: ${JSON.stringify(info)}`);
    if (!info.license || info.license.name !== "CC0-1.0") throw new Error("license not attached to the asset");
    if (!fs.existsSync(path.join(state.project, info.path))) throw new Error(`${info.path} is not on disk`);
    const cardPath = path.join(state.project, ".ibproject/context", info.card);
    if (!fs.readFileSync(cardPath, "utf8").includes("CC0-1.0")) throw new Error("Asset card lacks the license");
    // A stray file with no license record → unlicensed + unused.
    fs.mkdirSync(path.join(state.project, "assets/images"), { recursive: true });
    fs.writeFileSync(path.join(state.project, "assets/images/stray.png"), makePng(8, 8, [1, 2, 3]));
    const health = await call("assets_health", { projectPath: state.project });
    if (!health.unlicensed.includes("assets/images/stray.png")) throw new Error(`stray not unlicensed: ${JSON.stringify(health)}`);
    if (health.unlicensed.includes(info.path)) throw new Error("licensed asset reported as unlicensed");
    const credits = await call("assets_credits", { projectPath: state.project });
    if (!credits.includes("Hero")) throw new Error(`credits lack Hero: ${credits}`);
    const payload = await call("assets_read_base64", { projectPath: state.project, path: info.path, maxBytes: null });
    if (!payload.mime.startsWith("image/") || payload.truncated) throw new Error(`preview payload wrong: ${payload.mime}`);
    // The library "My own files" provider reads a folder.
    const found = await call("library_search", { provider: "local_folder", query: { text: tmp, kind: null, limit: null } });
    run.note(`local folder library found ${found.length} item(s)`);
    if (found.length !== 1) throw new Error(`expected 1 library item, got ${found.length}`);
    await clickWhenEnabled(driver, tid("nav-assets"));
    await waitVisible(driver, tid("assets-section"));
    await waitUntil(async () => (await driver.findElements(tid("asset-grid"))).length + (await driver.findElements(tid("asset-list"))).length > 0, { what: "the asset grid" });
    await run.shot("assets-project");
  }, { needs: ["c1"] });

  await run.step("c3", "Generating an image (fake Cloudflare): key stays out of files, result is accepted with a generated license", async () => {
    await call("credential_set", { name: "cloudflare_account_id", value: "acct-123" });
    await call("credential_set", { name: "cloudflare_api_token", value: SECRET });
    const status = await call("credential_status", { names: ["cloudflare_api_token", "fish_audio_api_key"] });
    if (!status.find((s) => s.name === "cloudflare_api_token")?.connected) throw new Error("token not reported connected");
    if (status.find((s) => s.name === "fish_audio_api_key")?.connected) throw new Error("unset key reported connected");
    if (JSON.stringify(status).includes(SECRET)) throw new Error("credential_status leaked the value");
    const providers = await call("generate_providers", {});
    if (!providers.find((p) => p.id === "cloudflare")?.connected) throw new Error(`cloudflare not connected: ${JSON.stringify(providers)}`);
    const preview = await call("generate_run", {
      projectPath: state.project,
      request: { kind: "image", prompt: "a red slime", options: { width: 64, height: 64, seed: null, pixel_art: true, transparent: false, model: null, duration_seconds: null, voice_id: null, looping: false }, style_guide: null },
    });
    if (!preview.base64 || preview.extension !== "png") throw new Error(`preview wrong: ${preview.extension}`);
    const sent = fakes.seen.cloudflare.at(-1);
    if (sent.auth !== `Bearer ${SECRET}`) throw new Error("fake Cloudflare did not get the bearer token");
    if (!sent.body.includes("a red slime")) throw new Error("prompt not sent");
    const accepted = await call("generate_accept", { projectPath: state.project, tempId: preview.temp_id, title: "Red slime", destSubdir: null });
    if (!fs.existsSync(path.join(state.project, accepted.path))) throw new Error("accepted file missing");
    if (!accepted.license?.generated_by?.toLowerCase().includes("cloudflare")) throw new Error(`no generated_by: ${JSON.stringify(accepted.license)}`);
    // A discard of an unknown id is harmless.
    await call("generate_discard", { tempId: "nope" });
    // The secret must not be in any project file, the app's settings, or its logs.
    const leaks = walk(state.project).filter((f) => {
      try {
        return fs.readFileSync(f).includes(SECRET);
      } catch {
        return false;
      }
    });
    const homeLeaks = walk(app.home).filter((f) => {
      try {
        return fs.readFileSync(f).includes(SECRET);
      } catch {
        return false;
      }
    });
    if (leaks.length || homeLeaks.length) throw new Error(`secret found in: ${[...leaks, ...homeLeaks].join(", ")}`);
    run.note("the key appears in no project file and nothing under the app's home");
    await call("credential_clear", { name: "cloudflare_api_token" });
    const after = await call("credential_status", { names: ["cloudflare_api_token"] });
    if (after[0].connected) throw new Error("credential_clear did not clear");
  }, { needs: ["c1"] });

  await run.step("c4", "A local model (fake OpenAI-compatible server) runs a real turn with the sandboxed file tools", async () => {
    const settings = await call("app_settings_get", {});
    await call("app_settings_set", {
      settings: { ...settings, ai_provider: "local-model", models: { "local-model": { base_url: `http://127.0.0.1:${fakes.chatPort}/v1`, model: "fake-model" } } },
    });
    const test = await call("ai_test_connection", { provider: "local-model" });
    if (!test.ok) throw new Error(`connection test failed: ${test.message}`);
    run.note(`connection test: ${JSON.stringify({ ok: test.ok, message: test.message })}`);
    const thread = await call("chat_create_thread", { projectPath: state.project, title: "Local" });
    await call("project_settings_set", { projectPath: state.project, settings: { ...(await call("project_settings_get", { projectPath: state.project })), plan_policy: "small_changes_direct" } });
    await call("agent_send", { projectPath: state.project, threadId: thread.id, message: "Please save a note file called notes.txt", origin: null, role: "designer" });
    await waitUntil(async () => fs.existsSync(path.join(state.project, "notes.txt")), { timeoutMs: 60_000, what: "the model's write_file to land on disk" });
    let loaded;
    await waitUntil(
      async () => {
        loaded = await call("chat_load_thread", { projectPath: state.project, threadId: thread.id });
        return loaded.records.some((r) => r.kind === "event" && JSON.stringify(r.event).includes("saved a note file"));
      },
      { timeoutMs: 60_000, what: "the model's final reply in the chat" },
    );
    const written = fs.readFileSync(path.join(state.project, "notes.txt"), "utf8");
    if (!written.includes("hello from the fake model")) throw new Error(`notes.txt: ${written}`);
    const asked = fakes.seen.chat.filter((c) => c.url.endsWith("/chat/completions"));
    // (The connection test above also called the server, with its own prompt.)
    const turnCall = asked.find((c) => JSON.stringify(c.parsed).includes("save a note"));
    run.attach("turn-request", JSON.stringify(turnCall?.parsed ?? null, null, 1));
    const system = JSON.stringify(turnCall?.parsed?.messages?.[0] ?? "");
    run.attach("system-prompt", system);
    if (!/designer/i.test(system)) throw new Error(`the Designer role prompt did not reach the model: ${system.slice(0, 300)} (calls: ${asked.length})`);
    if (asked.some((c) => c.auth)) throw new Error("a keyless local model was sent an Authorization header");
    await waitUntil(async () => snapshotSubjects(state.project).length >= 2, { timeoutMs: 30_000, what: "a snapshot for the model's change" });
    run.note(`chat replayed ${loaded.records.length} records; ${asked.length} model calls; snapshots: ${snapshotSubjects(state.project).join(" | ")}`);
    if (git(state.project, "status", "--porcelain").replace(/.*\.ibproject\/chat.*\n?/g, "").trim()) throw new Error("working tree not clean after the turn");
  }, { needs: ["c1"] });

  await run.step("c5", "Producer journey: real evidence, manual ticks, shown under Playtest & Launch", async () => {
    const journey = await call("journey_get", { projectPath: state.project });
    const all = journey.stages.flatMap((s) => s.criteria);
    run.note(`current stage ${journey.current}; ${all.filter((c) => c.done).length}/${all.length} done`);
    const auto = all.filter((c) => c.signal === "auto");
    if (auto.some((c) => c.done && !c.evidence)) throw new Error("an automatic criterion is done with no evidence");
    const manual = all.find((c) => c.signal === "manual");
    const ticked = await call("journey_set_manual", { projectPath: state.project, id: manual.id, done: true });
    if (!ticked.stages.flatMap((s) => s.criteria).find((c) => c.id === manual.id).done) throw new Error("manual tick not saved");
    const refused = await call("journey_set_manual", { projectPath: state.project, id: auto[0].id, done: true }).then(() => null, (e) => e.message);
    if (!refused) throw new Error("an automatic criterion could be ticked by hand");
    await clickWhenEnabled(driver, tid("nav-launch"));
    await waitVisible(driver, tid("journey-panel"));
  }, { needs: ["c1"] });

  await run.step("c6", "Home is a list of games with an inspector; Settings has its own pages", async () => {
    // An empty project doesn't end the first run; the list layout is what Home shows after it.
    const current = await call("app_settings_get", {});
    await call("app_settings_set", { settings: { ...current, first_run_done: true } });
    await clickWhenEnabled(driver, tid("nav-home"));
    await waitVisible(driver, tid("home-layout"), { timeoutMs: 30_000 });
    await waitVisible(driver, tid("home-sidebar")); // Home has its own sidebar
    if ((await driver.findElements(tid("nav-studio"))).length > 0) throw new Error("the game sidebar shows on Home");
    await waitUntil(async () => (await driver.findElements(tid("project-row"))).length >= 1, { what: "a game in the list" });
    const facts = await waitVisible(driver, tid("inspector-facts"), { timeoutMs: 30_000 });
    const text = await driver.executeScript("return arguments[0].innerText", facts);
    for (const label of ["Last opened", "Stage", "Context cards", "Saved versions"]) {
      if (!text.includes(label)) throw new Error(`inspector lacks ${label}: ${text}`);
    }
    if ((await driver.findElements(tid("status-ai"))).length > 0) throw new Error("the Setup block is still on Home");
    await run.shot("home-list-and-inspector");
    await clickWhenEnabled(driver, tid("home-nav-settings"));
    await waitVisible(driver, tid("settings-section"));
    for (const id of ["ai", "godot", "accounts", "about"]) {
      await clickWhenEnabled(driver, tid(`settings-${id}`));
      await new Promise((r) => setTimeout(r, 700));
      await run.shot(`settings-${id}`);
    }
  }, { needs: ["c1"] });

  await run.step("c7", "Documents: boards, blocks, arrows, columns, nesting, undo, search, graph, images", async () => {
    // Back to the game from Settings (Home → Open).
    await clickWhenEnabled(driver, tid("home-nav-home"));
    await clickWhenEnabled(driver, tid("inspector-open"), { timeoutMs: 30_000 });
    await clickWhenEnabled(driver, tid("nav-context"));
    await waitVisible(driver, tid("canvas"));
    const ctx = (rel) => path.join(state.project, ".ibproject/context", rel);
    const boardFile = (id) => path.join(state.project, ".ibproject/boards", `${id}.json`);
    const readBoard = (id = "root") => JSON.parse(fs.readFileSync(boardFile(id), "utf8"));
    const saved = (check, what) =>
      waitUntil(async () => {
        try {
          return check(readBoard());
        } catch {
          return false;
        }
      }, { timeoutMs: 8000, what });
    const centre = async (el) => driver.executeScript("const r = arguments[0].getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2];", el);
    const drag = async (el, to) => {
      // `to` is another element (drop on it) or a [dx, dy] offset.
      const [x1, y1] = await centre(el);
      let dx;
      let dy;
      if (Array.isArray(to)) [dx, dy] = to;
      else {
        const [x2, y2] = await centre(to);
        dx = x2 - x1;
        dy = y2 - y1;
      }
      const a = driver.actions({ async: true });
      await a.move({ origin: el }).press().move({ origin: "pointer", x: Math.round(dx / 3), y: Math.round(dy / 3) }).move({ origin: "pointer", x: Math.round(dx / 3), y: Math.round(dy / 3) }).move({ origin: "pointer", x: dx - 2 * Math.round(dx / 3), y: dy - 2 * Math.round(dy / 3) }).release().perform();
    };
    const clickEmpty = async () => {
      const canvas = await driver.findElement(tid("canvas"));
      const { width, height } = await canvas.getRect();
      await driver.actions().move({ origin: canvas, x: Math.floor(width / 2) - 24, y: -Math.floor(height / 2) + 24 }).click().perform();
    };
    const blocks = (type) => driver.findElements(tid(`block-${type}`));

    // The first time, what the project had becomes a board.
    await waitUntil(async () => fs.existsSync(boardFile("root")), { what: "the root board to be saved" });
    const root0 = readBoard();
    if (!root0.blocks.some((b) => b.type === "doc")) throw new Error("existing documents weren't placed on the root board");
    await run.shot("documents-canvas");

    // A document: a card on disk, a block on the board, a focused editor.
    await clickWhenEnabled(driver, tid("add-doc"));
    await run.shot("new-document-menu");
    await clickWhenEnabled(driver, tid("new-doc-blank"));
    await waitUntil(async () => fs.existsSync(ctx("untitled.md")), { what: "the new document" });
    await waitVisible(driver, tid("doc-editor"));
    const title = await waitVisible(driver, tid("page-title"));
    await driver.executeScript("arguments[0].focus(); arguments[0].select();", title);
    await title.sendKeys("Captain Bolt");
    await waitUntil(async () => fs.readFileSync(ctx("untitled.md"), "utf8").includes("title: Captain Bolt"), { timeoutMs: 8000, what: "the title to be saved" });
    await waitUntil(
      async () => {
        try {
          const editable = await driver.findElement(By.css('[data-testid="page-body"] .mdx-editor-content'));
          await editable.click();
          await editable.sendKeys("Loves the sea. See [[The Lighthouse]].");
          return true;
        } catch {
          return false;
        }
      },
      { what: "the page body to take typing" },
    );
    await waitUntil(async () => fs.readFileSync(ctx("untitled.md"), "utf8").includes("Loves the sea."), { timeoutMs: 8000, what: "autosave" });
    await run.shot("document-editor");
    await clickWhenEnabled(driver, tid("page-icon"));
    await clickWhenEnabled(driver, tid("icon-tab-mono"));
    await (await driver.findElements(tid("icon-choice-mono")))[26].click();
    await waitUntil(async () => /icon: "?lucide:/.test(fs.readFileSync(ctx("untitled.md"), "utf8")), { what: "the document's icon to be saved" });
    if (!fs.readFileSync(ctx("untitled.md"), "utf8").includes("Loves the sea.")) throw new Error("choosing an icon lost the text");
    await clickWhenEnabled(driver, tid("ask-ai"));
    await waitVisible(driver, tid("doc-action-draft"));
    await driver.actions().sendKeys(Key.ESCAPE).perform();
    await clickWhenEnabled(driver, tid("doc-close"));
    await waitUntil(async () => (await driver.findElements(tid("doc-editor"))).length === 0, { what: "the editor to close" });
    await saved((b) => b.blocks.some((x) => x.type === "doc" && x.ref === "untitled.md"), "the document block to be saved");
    const docBlock = (await blocks("doc")).length;
    if (docBlock < 1) throw new Error("no document block on the canvas");

    // Notes: type in place, drag, snap.
    await clickWhenEnabled(driver, tid("add-note"));
    await driver.actions().sendKeys("Remember the lighthouse").perform();
    await clickEmpty();
    await saved((b) => JSON.stringify(b).includes("Remember the lighthouse"), "the note's text to be saved");
    await clickWhenEnabled(driver, tid("add-note"));
    await driver.actions().sendKeys("Second note").perform();
    await clickEmpty();
    let notes = await blocks("note");
    if (notes.length !== 2) throw new Error(`expected 2 notes, found ${notes.length}`);
    const before = readBoard().blocks.filter((b) => b.type === "note").map((b) => b.x);
    await drag(notes[1], [260, 90]);
    await saved((b) => b.blocks.filter((x) => x.type === "note")[1].x !== before[1], "the dragged note's position to be saved");
    await run.shot("notes-on-canvas");

    // Colour from the selection bar.
    notes = await blocks("note");
    await notes[0].click();
    await clickWhenEnabled(driver, tid("note-color-pink"));
    await saved((b) => b.blocks.some((x) => x.type === "note" && x.color === "pink"), "the note colour");

    // Align two blocks (shift-click to select both).
    notes = await blocks("note");
    await driver.actions().keyDown(Key.SHIFT).click(notes[0]).click(notes[1]).keyUp(Key.SHIFT).perform();
    await clickWhenEnabled(driver, tid("align-left"));
    await saved((b) => {
      const n = b.blocks.filter((x) => x.type === "note");
      return n[0].x === n[1].x;
    }, "the notes to line up");
    await clickEmpty();

    // An arrow between them.
    await clickWhenEnabled(driver, tid("tool-arrow"));
    notes = await blocks("note");
    await drag(notes[0], notes[1]);
    await saved((b) => b.blocks.some((x) => x.type === "arrow"), "the arrow to be saved");
    await waitUntil(async () => (await blocks("arrow")).length === 1, { what: "the arrow on the canvas" });
    await run.shot("arrow-between-notes");
    const arrow = readBoard().blocks.find((x) => x.type === "arrow");
    const noteIds = readBoard().blocks.filter((x) => x.type === "note").map((x) => x.id);
    if (!noteIds.includes(arrow.from) || !noteIds.includes(arrow.to)) throw new Error("the arrow isn't attached to the notes");

    // A column takes blocks dropped on it.
    await clickWhenEnabled(driver, tid("add-column"));
    await run.shot("column-added");
    notes = await blocks("note");
    const body = await driver.findElement(By.css("[data-column-body]"));
    await drag(notes[1], body);
    await saved((b) => {
      const col = b.blocks.find((x) => x.type === "column");
      return col && col.children.length === 1 && b.blocks.find((x) => x.id === col.children[0]).col === col.id;
    }, "the note to snap into the column");
    await run.shot("note-in-column");
    const arrowAfter = (await blocks("arrow")).length;
    if (arrowAfter !== 1) throw new Error("the arrow was lost when its note moved into the column");

    // A board inside a board.
    await clickWhenEnabled(driver, tid("add-board"));
    await saved((b) => b.blocks.some((x) => x.type === "board"), "the board block to be saved");
    const boardBlock = (await blocks("board"))[0];
    await driver.actions().move({ origin: boardBlock, x: 0, y: -30 }).doubleClick().perform();
    await waitUntil(async () => (await driver.findElements(tid("crumb"))).length === 1, { what: "the breadcrumb to show the nested board" });
    const name = await waitVisible(driver, tid("board-name"));
    await driver.executeScript("arguments[0].focus(); arguments[0].select();", name);
    await name.sendKeys("Crew");
    const childId = readBoard().blocks.find((x) => x.type === "board").ref;
    await waitUntil(async () => fs.existsSync(boardFile(childId)) && readBoard(childId).title === "Crew", { timeoutMs: 8000, what: "the board's name to be saved" });
    await run.shot("nested-board");
    await clickWhenEnabled(driver, tid("crumb"));
    await waitUntil(async () => (await driver.findElements(tid("crumb"))).length === 0, { what: "back at the top board" });
    // Drag a note onto the board block: it moves in.
    notes = await blocks("note");
    const free = readBoard().blocks.filter((x) => x.type === "note" && !x.col)[0];
    const target = await driver.findElement(By.css(`[data-block-id="${free.id}"]`));
    await drag(target, (await blocks("board"))[0]);
    await saved((b) => !b.blocks.some((x) => x.id === free.id), "the note to leave the top board");
    if (!readBoard(childId).blocks.some((x) => x.id === free.id)) throw new Error("the note didn't arrive in the nested board");
    const arrowsLeft = readBoard().blocks.filter((x) => x.type === "arrow").length;
    if (arrowsLeft !== 0) throw new Error("an arrow to a note that moved away was kept");

    // Undo puts it back; redo moves it again.
    await clickWhenEnabled(driver, tid("canvas-undo"));
    await saved((b) => b.blocks.some((x) => x.id === free.id), "undo to put the note back");
    if (readBoard(childId).blocks.some((x) => x.id === free.id)) throw new Error("undo left the note in both boards");
    await clickWhenEnabled(driver, tid("canvas-undo"));
    await saved((b) => b.blocks.some((x) => x.type === "arrow"), "a second undo to bring the arrow back");
    await clickWhenEnabled(driver, tid("canvas-redo"));
    await saved((b) => !b.blocks.some((x) => x.type === "arrow"), "redo to remove the arrow again");
    await clickWhenEnabled(driver, tid("canvas-undo"));
    await saved((b) => b.blocks.some((x) => x.type === "arrow"), "undo once more");

    // A picture dropped (through the file picker's input) and a file.
    const png = makePng(40, 20, [200, 40, 90]).toString("base64");
    await driver.executeScript(
      `const input = document.querySelector('[data-testid="canvas-file-input"]');
       const bytes = Uint8Array.from(atob(arguments[0]), (c) => c.charCodeAt(0));
       const dt = new DataTransfer();
       dt.items.add(new File([bytes], "red box.png", { type: "image/png" }));
       dt.items.add(new File([new Uint8Array([1, 2, 3])], "notes.bin", { type: "application/octet-stream" }));
       input.files = dt.files;
       input.dispatchEvent(new Event("change", { bubbles: true }));`,
      png,
    );
    await waitUntil(async () => (await blocks("image")).length === 1 && (await blocks("file")).length === 1, { what: "the picture and the file on the canvas" });
    await saved((b) => b.blocks.some((x) => x.type === "image" && x.src.startsWith(".ibproject/boards/files/")), "the image block to be saved");
    const imageBlock = readBoard().blocks.find((x) => x.type === "image");
    if (!fs.existsSync(path.join(state.project, imageBlock.src))) throw new Error("the picture file isn't in the project");
    // A link and a to-do list, a swatch and a table.
    for (const type of ["link", "todo", "swatch", "table", "text", "comment", "sketch"]) await clickWhenEnabled(driver, tid(`add-${type}`));
    await clickEmpty();
    await saved((b) => ["link", "todo", "swatch", "table", "text", "comment", "sketch"].every((t) => b.blocks.some((x) => x.type === t)), "every kind of block to be saved");
    await clickWhenEnabled(driver, tid("zoom-fit"));
    await new Promise((r) => setTimeout(r, 600));
    await run.shot("every-block-type");

    // Links between documents: [[wiki links]] are edges.
    await call("context_write", {
      projectPath: state.project,
      path: "the-lighthouse.md",
      meta: { type: null, title: "The Lighthouse", status: null, links: [], implemented_in: [], tags: [], extra: {} },
      body: "Back to [[Captain Bolt]].",
    });
    const graph = await call("context_graph", { projectPath: state.project });
    const linked = (a, b) => graph.edges.some((e) => e.from === a && e.to === b && !e.broken);
    if (!linked("untitled.md", "the-lighthouse.md") || !linked("the-lighthouse.md", "untitled.md")) throw new Error(`wiki links aren't edges: ${JSON.stringify(graph.edges)}`);

    // Search finds blocks and documents; picking a block lands on it.
    await clickWhenEnabled(driver, tid("search-open"));
    await waitVisible(driver, tid("unplaced-doc")); // the lighthouse card isn't on a board
    await run.shot("search-unplaced");
    const input = await waitVisible(driver, tid("search-input"));
    await input.sendKeys("lighthouse");
    await waitUntil(async () => (await driver.findElements(tid("hit-block"))).length >= 1 && (await driver.findElements(tid("hit-doc"))).length >= 1, { what: "search results for blocks and documents" });
    await run.shot("search-results");
    await clickWhenEnabled(driver, tid("hit-block"));
    await waitVisible(driver, tid("selection-bar"));

    // The same boards as a graph; clicking a node opens it.
    await clickWhenEnabled(driver, tid("mode-graph"));
    await waitVisible(driver, tid("graph-view"));
    await waitUntil(async () => /\d+ nodes/.test(await (await driver.findElement(tid("graph-count"))).getText()), { what: "graph nodes" });
    const counts = await (await driver.findElement(tid("graph-count"))).getText();
    run.note(`graph: ${counts}`);
    await new Promise((r) => setTimeout(r, 900));
    await run.shot("graph-view");
    await clickWhenEnabled(driver, By.css(".react-flow__node"));
    await waitVisible(driver, tid("canvas"));

    // Deleting a document block keeps the document.
    const docs = await blocks("doc");
    await docs[docs.length - 1].click();
    await driver.actions().sendKeys(Key.DELETE).perform();
    await saved((b) => b.blocks.filter((x) => x.type === "doc").length < docBlock + root0.blocks.filter((x) => x.type === "doc").length - 0, "the document block to be removed");
    if (!fs.existsSync(ctx("untitled.md")) && !fs.existsSync(ctx("the-lighthouse.md"))) throw new Error("deleting a block deleted its document");
  }, { needs: ["c1"] });

  await run.step("c7b", "Model choice and an attached picture reach the AI", async () => {
    // Model choice and an attached picture reach the AI (fake local model).
    await clickWhenEnabled(driver, tid("nav-studio"));
    await waitVisible(driver, tid("chat-composer"));
    await clickWhenEnabled(driver, tid("model-picker"));
    const custom = await waitVisible(driver, tid("model-custom"));
    await custom.sendKeys("fake-model-2");
    await run.shot("model-picker");
    await driver.actions().sendKeys("\uE00C").perform(); // Escape closes the picker
    const png = makePng(16, 16, [9, 200, 60]).toString("base64");
    await driver.executeScript(
      `const input = document.querySelector('[data-testid="attach-input"]');
       const bytes = Uint8Array.from(atob(arguments[0]), (c) => c.charCodeAt(0));
       const dt = new DataTransfer();
       dt.items.add(new File([bytes], "my sketch.png", { type: "image/png" }));
       input.files = dt.files;
       input.dispatchEvent(new Event("change", { bubbles: true }));`,
      png,
    );
    await waitVisible(driver, tid("attachment-chip"));
    await run.shot("attachment-chip");
    const box = await waitVisible(driver, By.css('[data-testid="chat-composer"] textarea'));
    await box.sendKeys("Please save a note about the attached sketch");
    await driver.actions().sendKeys("\uE007").perform();
    await waitUntil(async () => fakes.seen.chat.some((c) => JSON.stringify(c.parsed).includes("my sketch")), { timeoutMs: 60_000, what: "the model call carrying the attachment" });
    const call2 = fakes.seen.chat.find((c) => JSON.stringify(c.parsed).includes("my sketch"));
    if (call2.parsed.model !== "fake-model-2") throw new Error(`model was ${call2.parsed.model}, not the one chosen`);
    const saved = fs.readdirSync(path.join(state.project, ".ibproject/chat/attachments"));
    if (!saved.some((f) => f.endsWith("my sketch.png"))) throw new Error(`attachment not saved: ${saved}`);
    const tracked = git(state.project, "ls-files").split("\n");
    if (tracked.some((f) => f.includes("chat/attachments"))) throw new Error("attachments were committed");
    await waitUntil(async () => (await driver.findElements(tid("message-attachments"))).length > 0, { what: "the attachment shown in the message" });
    await run.shot("message-with-attachment");
  }, { needs: ["c4"] });

  await run.step("c8", "Code page and Settings from inside a game; Tasks tab", async () => {
    await clickWhenEnabled(driver, tid("nav-code"));
    await new Promise((r) => setTimeout(r, 2500)); // the terminal starts
    await run.shot("code-page");
    await clickWhenEnabled(driver, tid("nav-settings"));
    await waitVisible(driver, tid("settings-section"));
    if ((await driver.findElements(tid("home-sidebar"))).length > 0) throw new Error("Settings from a game shows the Home sidebar");
    await waitVisible(driver, tid("nav-studio")); // the game sidebar frames it
    await run.shot("settings-from-game");
    await clickWhenEnabled(driver, tid("nav-context"));
    await waitUntil(
      async () => {
        await clickWhenEnabled(driver, By.xpath("//button[@role='tab'][normalize-space()='Tasks']"));
        return (await driver.findElements(tid("board-tab"))).length > 0;
      },
      { what: "the Tasks tab to open" },
    );
    await run.shot("context-tasks");
  }, { needs: ["c7"] });
}
