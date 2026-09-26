// Small WebDriver helpers over the app's `data-testid` attributes. Every
// wait polls the real DOM until a condition holds; nothing sleeps for a
// fixed time hoping the UI caught up.

import { By } from "selenium-webdriver";

export const tid = (id) => By.css(`[data-testid="${id}"]`);

/** Polls `fn` until it returns a truthy value, and returns that value. */
export async function waitUntil(fn, { timeoutMs = 15_000, intervalMs = 150, what = "condition" } = {}) {
  const deadline = Date.now() + timeoutMs;
  let lastError;
  for (;;) {
    try {
      const value = await fn();
      if (value) return value;
    } catch (err) {
      // `fatal` errors mean "this will never come true" — stop waiting.
      if (err.fatal) throw err;
      lastError = err;
    }
    if (Date.now() > deadline) {
      const detail = lastError ? ` (last error: ${lastError.message})` : "";
      throw new Error(`Timed out after ${timeoutMs}ms waiting for ${what}${detail}`);
    }
    await new Promise((r) => setTimeout(r, intervalMs));
  }
}

/** The first element matching `locator` that is displayed. */
export async function waitVisible(driver, locator, opts = {}) {
  return waitUntil(
    async () => {
      for (const el of await driver.findElements(locator)) {
        if (await el.isDisplayed()) return el;
      }
      return null;
    },
    { what: `a visible ${locator}`, ...opts },
  );
}

export async function waitAttr(driver, locator, attr, expected, opts = {}) {
  const want = Array.isArray(expected) ? expected : [expected];
  let last;
  try {
    return await waitUntil(
      async () => {
        const el = await driver.findElement(locator);
        last = await el.getAttribute(attr);
        return want.includes(last) ? last : null;
      },
      { what: `${locator} ${attr} in [${want.join(", ")}]`, ...opts },
    );
  } catch (err) {
    throw new Error(`${err.message}; last value: ${JSON.stringify(last)}`);
  }
}

/** Clicks through a real WebDriver click once the element is enabled. */
/** Sections and dialogs fade in, so a first click can land while the
 * element isn't interactable yet; the click is retried until it goes
 * through (or the wait times out). */
export async function clickWhenEnabled(driver, locator, opts = {}) {
  return waitUntil(
    async () => {
      for (const e of await driver.findElements(locator)) {
        if ((await e.isDisplayed()) && (await e.isEnabled())) {
          await e.click();
          return e;
        }
      }
      return null;
    },
    { what: `a clickable ${locator}`, ...opts },
  );
}

/** Text of the element (innerText, so hidden overflow still counts). */
export async function textOf(driver, locator) {
  const el = await driver.findElement(locator);
  return driver.executeScript("const e = arguments[0]; return e.innerText || e.textContent || ''", el);
}

/** True when the element sits inside an `inert` subtree — how App.tsx
 * parks the permanently mounted sections that aren't the active one. */
export async function isInert(driver, el) {
  return driver.executeScript("return arguments[0].closest('[inert]') !== null", el);
}

/** Calls a real Tauri command through the webview's own IPC bridge — the
 * exact path the frontend's `invoke()` uses. For steps that stand in for
 * something the app itself would do (e.g. the snapshot an AI turn makes). */
export async function invokeCommand(driver, command, args) {
  const result = await driver.executeAsyncScript(
    `const [cmd, args, done] = arguments;
     window.__TAURI_INTERNALS__.invoke(cmd, args)
       .then((value) => done({ ok: true, value }))
       .catch((error) => done({ ok: false, error: String(error) }));`,
    command,
    args,
  );
  if (!result.ok) throw new Error(`${command} failed: ${result.error}`);
  return result.value;
}
