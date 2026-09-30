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
    await waitUntil(async () => (await driver.findElements(tid("tree-page"))).length >= 2, { what: "the notes tree to show pages" });
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

  await run.step("c7", "Notes: pages, folders, templates, autosave, drag to move, delete; model choice and attachments reach the AI", async () => {
    // Back to the game from Settings (Home → Open).
    await clickWhenEnabled(driver, tid("home-nav-home"));
    await clickWhenEnabled(driver, tid("inspector-open"), { timeoutMs: 30_000 });
    await clickWhenEnabled(driver, tid("nav-context"));
    await waitVisible(driver, tid("notes-tree"));
    const ctx = (rel) => path.join(state.project, ".ibproject/context", rel);

    // A page from a template; its title is its name, and the file follows.
    await clickWhenEnabled(driver, tid("new-menu"));
    await run.shot("new-menu");
    await clickWhenEnabled(driver, tid("template-character"));
    await waitUntil(async () => fs.existsSync(ctx("character-sheet.md")), { what: "the templated page" });
    const first = fs.readFileSync(ctx("character-sheet.md"), "utf8");
    if (!first.includes("type: character") || !first.includes("## Behaviour")) throw new Error(`template not written: ${first.slice(0, 300)}`);
    const title = await waitVisible(driver, tid("page-title"));
    await driver.executeScript("arguments[0].focus(); arguments[0].select();", title);
    await title.sendKeys("Captain Bolt");
    run.note(`title now reads: ${await title.getAttribute("value")}`);
    await title.sendKeys(Key.ENTER);
    await waitUntil(async () => fs.existsSync(ctx("captain-bolt.md")) && !fs.existsSync(ctx("character-sheet.md")), { what: "the file to be renamed after its title" });
    if (!fs.readFileSync(ctx("captain-bolt.md"), "utf8").includes("title: Captain Bolt")) throw new Error("title not saved in the page");

    // Typing in the body saves by itself.
    // The page reopens under its new name; find the body again until it holds still.
    await waitUntil(
      async () => {
        try {
          const editable = await driver.findElement(By.css('[data-testid="page-body"] .mdx-editor-content'));
          await editable.click();
          await editable.sendKeys("Loves the sea.");
          return true;
        } catch {
          return false;
        }
      },
      { what: "the page body to take typing" },
    );
    await waitUntil(async () => fs.readFileSync(ctx("captain-bolt.md"), "utf8").includes("Loves the sea."), { timeoutMs: 8000, what: "autosave" });
    await run.shot("notes-page");

    // A page icon, saved in the page's header and shown in the tree.
    await clickWhenEnabled(driver, tid("page-icon"));
    await run.shot("icon-picker");
    await clickWhenEnabled(driver, tid("icon-tab-mono"));
    await run.shot("icon-picker-mono");
    await (await driver.findElements(tid("icon-choice-mono")))[26].click();
    await waitUntil(async () => /icon: "?lucide:/.test(fs.readFileSync(ctx("captain-bolt.md"), "utf8")), { what: "the page's black-and-white icon to be saved" });
    await waitVisible(driver, By.css('[data-testid="tree-page"][data-path="captain-bolt.md"] [data-testid="row-icon"]'));
    if (!fs.readFileSync(ctx("captain-bolt.md"), "utf8").includes("Loves the sea.")) throw new Error("choosing an icon lost the page text");

    // The AI menu.
    await clickWhenEnabled(driver, tid("ask-ai"));
    await waitVisible(driver, tid("doc-action-draft"));
    await run.shot("ask-ai-menu");
    await driver.actions().sendKeys(Key.ESCAPE).perform();

    // A folder of the person's own; dragging the page into it moves the file.
    await clickWhenEnabled(driver, tid("new-menu"));
    await clickWhenEnabled(driver, tid("new-folder"));
    await waitUntil(
      async () => {
        try {
          const rename = await driver.findElement(tid("rename-input"));
          await driver.executeScript("arguments[0].focus(); arguments[0].select();", rename);
          await rename.sendKeys("Crew");
          await rename.sendKeys(Key.ENTER);
          return true;
        } catch {
          return false;
        }
      },
      { what: "the new folder's name box to take typing" },
    );
    await waitUntil(async () => fs.existsSync(ctx("crew")), { what: "the Crew folder" });
    await waitVisible(driver, By.css('[data-testid="tree-folder"][data-path="crew"]'));
    // A folder icon, kept with the folder.
    const crew = await driver.findElement(By.css('[data-testid="tree-folder"][data-path="crew"]'));
    await driver.actions().move({ origin: crew }).perform();
    await clickWhenEnabled(driver, By.css('[data-path="crew"] [data-testid="row-menu"]'));
    await clickWhenEnabled(driver, tid("change-icon"));
    await (await driver.findElements(tid("icon-choice")))[20].click();
    await waitUntil(async () => fs.existsSync(ctx(".icons.json")) && fs.readFileSync(ctx(".icons.json"), "utf8").includes("crew"), { what: "the folder icon to be saved" });
    await waitVisible(driver, By.css('[data-testid="tree-folder"][data-path="crew"] [data-testid="row-icon"]'));
    await driver.executeScript(`
      const page = document.querySelector('[data-testid="tree-page"][data-path="captain-bolt.md"]');
      const folder = document.querySelector('[data-testid="tree-folder"][data-path="crew"]');
      const dt = new DataTransfer();
      page.dispatchEvent(new DragEvent("dragstart", { bubbles: true, dataTransfer: dt }));
      folder.dispatchEvent(new DragEvent("dragover", { bubbles: true, cancelable: true, dataTransfer: dt }));
      folder.dispatchEvent(new DragEvent("drop", { bubbles: true, cancelable: true, dataTransfer: dt }));`);
    await waitUntil(async () => fs.existsSync(ctx("crew/captain-bolt.md")), { what: "the page to move into the folder" });
    await run.shot("notes-tree-folder");
    const listed = await call("context_list", { projectPath: state.project });
    if (!listed.some((c) => c.path === "crew/captain-bolt.md" && c.card_type === "character")) throw new Error("moved page isn't listed as a character");

    // Delete asks first.
    const row = await driver.findElement(By.css('[data-testid="tree-page"][data-path="crew/captain-bolt.md"]'));
    await driver.actions().move({ origin: row }).perform();
    await clickWhenEnabled(driver, By.css('[data-path="crew/captain-bolt.md"] [data-testid="row-menu"]'));
    await clickWhenEnabled(driver, tid("delete"));
    await clickWhenEnabled(driver, tid("confirm-delete"));
    await waitUntil(async () => !fs.existsSync(ctx("crew/captain-bolt.md")), { what: "the page to be deleted" });

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

  await run.step("c8", "Code page and Settings from inside a game; Board and Map tabs", async () => {
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
        await clickWhenEnabled(driver, By.xpath("//button[@role='tab'][normalize-space()='Board']"));
        return (await driver.findElements(tid("board-tab"))).length > 0;
      },
      { what: "the Board tab to open" },
    );
    await run.shot("context-board");
    await waitUntil(
      async () => {
        await clickWhenEnabled(driver, By.xpath("//button[@role='tab'][normalize-space()='Map']"));
        return (await driver.findElements(tid("map-tab"))).length > 0;
      },
      { what: "the Map tab to open" },
    );
    await run.shot("context-map");
  }, { needs: ["c7"] });
}
