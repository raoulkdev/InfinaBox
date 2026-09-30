// Desktop-app behaviour, not features: nothing scrolls or selects the way a
// web page would, panels drag and resize, layout choices are saved in the
// app's settings, and controls have the sizes they should.

import { By } from "selenium-webdriver";
import { clickWhenEnabled, invokeCommand, tid, waitUntil, waitVisible } from "../lib/ui.mjs";
import { createProjectFromHome, tempParent } from "./common.mjs";

export async function uiChecks(run, app) {
  const { driver } = app;
  const state = { project: null };

  await run.step("u1", "A new project's chat box starts one line tall", async () => {
    await waitVisible(driver, tid("new-project"), { timeoutMs: 30_000 });
    state.project = await createProjectFromHome(run, driver, tempParent("ui"), "UI Game");
    const box = await waitVisible(driver, By.css('[data-testid="chat-composer"] textarea'));
    await new Promise((r) => setTimeout(r, 1500));
    const h = await driver.executeScript("return arguments[0].getBoundingClientRect().height", box);
    const composer = await driver.executeScript(
      "return document.querySelector('[data-testid=\"chat-composer\"]').getBoundingClientRect().height",
    );
    run.note(`textarea ${h}px, composer ${composer}px`);
    await run.shot("studio-fresh");
    if (h > 60) throw new Error(`the chat box is ${h}px tall before anything is typed`);
  });

  await run.step("u2", "Nothing scrolls except where content needs it; page text can't be selected", async () => {
    const info = await driver.executeScript(`
      const s = document.scrollingElement;
      const cs = (el) => getComputedStyle(el);
      return {
        pageScrolls: s.scrollHeight > s.clientHeight + 1 || s.scrollWidth > s.clientWidth + 1,
        htmlOverflow: cs(document.documentElement).overflow,
        bodyOverflow: cs(document.body).overflow,
        bodySelect: cs(document.body).userSelect || cs(document.body).webkitUserSelect,
        overscroll: cs(document.documentElement).overscrollBehavior,
      };`);
    run.note(JSON.stringify(info));
    if (info.pageScrolls) throw new Error("the page itself scrolls");
    if (info.bodySelect !== "none") throw new Error(`body user-select is ${info.bodySelect}`);
    await driver.executeScript("window.scrollTo(0, 500); document.body.scrollTop = 500;");
    const y = await driver.executeScript("return [window.scrollY, document.body.scrollTop, document.documentElement.scrollTop]");
    if (y.some((v) => v !== 0)) throw new Error(`scrolling the page moved it: ${y}`);
    // Typing still works and text in inputs is selectable.
    const box = await driver.findElement(By.css('[data-testid="chat-composer"] textarea'));
    const sel = await driver.executeScript("const c = getComputedStyle(arguments[0]); return c.userSelect || c.webkitUserSelect", box);
    if (sel === "none") throw new Error("the chat box can't select its text");
  }, { needs: ["u1"] });

  await run.step("u3", "Dragging a panel reorders it, and the layout is saved in the app's settings", async () => {
    await clickWhenEnabled(driver, tid("nav-context"));
    await waitVisible(driver, tid("context-section"));
    await waitUntil(async () => (await driver.findElements(tid("panel-reorder-grip"))).length >= 3, { what: "the panels' grips" });
    const before = (await invokeCommand(driver, "layouts_get", {}))["context.documents"];
    const grips = await driver.findElements(By.css('[data-testid="context-section"] [data-testid="panel-reorder-grip"]'));
    // The first visible panel's grip: hover its panel so the grip accepts the pointer.
    let moved = false;
    for (const grip of grips) {
      if (!(await grip.isDisplayed())) continue;
      const rect = await driver.executeScript("const r = arguments[0].getBoundingClientRect(); return [r.x, r.y, r.width, r.height]", grip);
      if (rect[2] === 0) continue;
      await driver.actions().move({ x: Math.round(rect[0] + rect[2] / 2), y: Math.round(rect[1] + 20) }).perform();
      await driver
        .actions()
        .move({ x: Math.round(rect[0] + rect[2] / 2), y: Math.round(rect[1] + rect[3] / 2) })
        .press()
        .move({ x: Math.round(rect[0] + rect[2] / 2 + 200), y: Math.round(rect[1] + rect[3] / 2), duration: 300 })
        .move({ x: Math.round(rect[0] + rect[2] / 2 + 700), y: Math.round(rect[1] + rect[3] / 2), duration: 300 })
        .release()
        .perform();
      moved = true;
      break;
    }
    if (!moved) throw new Error("no grip to drag");
    await new Promise((r) => setTimeout(r, 1200));
    const mid = (await invokeCommand(driver, "layouts_get", {}))["context.documents"];
    if (before && JSON.stringify(mid?.order) === JSON.stringify(before.order)) {
      // Pointer actions through the driver didn't move it: try the same gesture as synthetic events, to tell the app's logic from the driver.
      const synthetic = await driver.executeScript(`
        const grip = document.querySelector('[data-testid="context-section"] [data-testid="panel-reorder-grip"]');
        const r = grip.getBoundingClientRect();
        const x = r.x + r.width / 2, y = r.y + 2;
        const opts = (dx, buttons) => ({ bubbles: true, cancelable: true, clientX: x + dx, clientY: y, buttons, button: 0, pointerId: 1, pointerType: "mouse", isPrimary: true });
        grip.dispatchEvent(new PointerEvent("pointerdown", opts(0, 1)));
        for (const dx of [50, 200, 400, 700]) window.dispatchEvent(new PointerEvent("pointermove", opts(dx, 1)));
        window.dispatchEvent(new PointerEvent("pointerup", opts(700, 0)));
        return true;`);
      run.note(`driver drag did nothing; synthetic events sent: ${synthetic}`);
      await new Promise((r) => setTimeout(r, 1000));
    }
    const after = (await invokeCommand(driver, "layouts_get", {}))["context.documents"];
    run.note(`before ${JSON.stringify(before?.order)}, after ${JSON.stringify(after?.order)}`);
    if (!after) throw new Error("no layout was saved in the app's settings");
    if (before && JSON.stringify(before.order) === JSON.stringify(after.order)) throw new Error("dragging the grip didn't reorder the panels");
    await run.shot("panels-reordered");
    // The sidebar's collapsed state goes to the same place.
    const collapse = await driver.findElements(By.xpath("//button[normalize-space()='Collapse']"));
    if (collapse.length) {
      await collapse[0].click();
      await new Promise((r) => setTimeout(r, 900));
      const layouts = await invokeCommand(driver, "layouts_get", {});
      if (layouts["sidebar.collapsed"] !== true) throw new Error(`collapsed state not saved: ${JSON.stringify(layouts)}`);
    }
  }, { needs: ["u1"] });

  await run.step("u4", "Settings fills its page, and the AI cards in a row are the same height", async () => {
    await clickWhenEnabled(driver, tid("nav-settings"));
    await waitVisible(driver, tid("settings-section"));
    await waitUntil(async () => (await driver.findElements(By.css('[data-testid^="provider-"]'))).length >= 4, { what: "the AI cards" });
    await new Promise((r) => setTimeout(r, 800));
    const m = await driver.executeScript(`
      const rect = (el) => el.getBoundingClientRect();
      const cards = [...document.querySelectorAll('[data-testid^="provider-"]')].filter((e) => e.matches('div[data-testid]') && !e.dataset.testid.startsWith('provider-install') && !e.dataset.testid.startsWith('provider-signin') && !e.dataset.testid.startsWith('provider-test'));
      const rows = new Map();
      for (const c of cards) { const r = rect(c); const key = Math.round(r.y); rows.set(key, [...(rows.get(key) ?? []), Math.round(r.height)]); }
      const section = document.querySelector('[data-testid="settings-section"]');
      const page = [...section.querySelectorAll('h1')][0].parentElement.parentElement;
      const content = document.querySelector('[data-testid="connect-group-subscription"]');
      return { rows: [...rows.values()], pageWidth: rect(page).width, contentWidth: rect(content).width };`);
    run.note(JSON.stringify(m));
    for (const heights of m.rows) {
      if (heights.length > 1 && new Set(heights).size !== 1) throw new Error(`cards in one row differ in height: ${heights}`);
    }
    if (m.contentWidth < m.pageWidth - 60) throw new Error(`settings content is ${m.contentWidth}px inside a ${m.pageWidth}px page`);
    await run.shot("settings-ai-full-width");
  }, { needs: ["u1"] });
}
