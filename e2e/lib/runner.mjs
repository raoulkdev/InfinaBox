// A minimal step runner. The scenarios are one long story (create a project,
// play it, break it, undo), so steps share state and run in order; a step
// can name the steps it `needs`, and is skipped (not failed) when one of
// them didn't pass. Every step ends with a screenshot, pass or fail.

import fs from "node:fs";
import path from "node:path";
import { screenshot } from "./x11.mjs";

export class Run {
  constructor(artifactsDir) {
    this.dir = artifactsDir;
    fs.mkdirSync(this.dir, { recursive: true });
    this.results = [];
    this.counter = 0;
  }

  /** Records a note (evidence) against the step currently running. */
  note(text) {
    console.log(`      · ${text}`);
    this.current?.notes.push(text);
  }

  /** Saves `text` as an artifact of the current step (e.g. the game log). */
  attach(label, text) {
    const file = path.join(this.dir, `${String(++this.counter).padStart(2, "0")}-${label}.txt`);
    fs.writeFileSync(file, text);
    this.current?.attachments.push(file);
    return file;
  }

  /**
   * Two captures: the X screen (or one native window, e.g. the game's), and
   * — when a WebDriver session is attached as `run.driver` and no specific
   * window was asked for — the webview's own rendering (`*.webview.png`).
   * The X picture can lag the DOM under Xvfb (WebKitGTK composites through
   * software GL, and sections cross-fade), so there's a short settle first,
   * and the webview capture is the reliable record of what the UI showed.
   */
  async shot(label, windowId) {
    await new Promise((r) => setTimeout(r, 750));
    const base = path.join(this.dir, `${String(++this.counter).padStart(2, "0")}-${label}`);
    try {
      screenshot(`${base}.png`, windowId);
      this.current?.screenshots.push(`${base}.png`);
    } catch (err) {
      this.note(`screenshot ${label} failed: ${err.message}`);
    }
    if (this.driver && !windowId) {
      try {
        fs.writeFileSync(`${base}.webview.png`, await this.driver.takeScreenshot(), "base64");
        this.current?.screenshots.push(`${base}.webview.png`);
      } catch (err) {
        this.note(`webview screenshot ${label} failed: ${err.message}`);
      }
    }
    return `${base}.png`;
  }

  /** `after` runs once the step has finished, pass or fail (e.g. to save
   * logs); its own failure is only noted. */
  async step(id, title, fn, { needs = [], after } = {}) {
    const result = { id, title, status: "pending", notes: [], screenshots: [], attachments: [], error: null };
    this.results.push(result);
    const blocked = needs.filter((n) => this.results.find((r) => r.id === n)?.status !== "pass");
    if (blocked.length > 0) {
      result.status = "skip";
      result.error = `skipped: needs ${blocked.join(", ")}`;
      console.log(`SKIP ${id} ${title} (${result.error})`);
      return result;
    }
    console.log(`---- ${id} ${title}`);
    this.current = result;
    const started = Date.now();
    try {
      await fn(this);
      result.status = "pass";
    } catch (err) {
      result.status = "fail";
      result.error = err.stack || String(err);
    }
    if (after) {
      try {
        await after(this);
      } catch (err) {
        this.note(`after-step hook failed: ${err.message}`);
      }
    }
    await this.shot(`${id}-${result.status}`);
    result.ms = Date.now() - started;
    this.current = null;
    console.log(`${result.status.toUpperCase()} ${id} ${title} (${(result.ms / 1000).toFixed(1)}s)`);
    if (result.status === "fail") console.log(`      ${result.error.split("\n").slice(0, 4).join("\n      ")}`);
    return result;
  }

  writeReport() {
    const file = path.join(this.dir, "report.json");
    fs.writeFileSync(file, JSON.stringify(this.results, null, 2));
    const lines = ["", "Summary:"];
    for (const r of this.results) lines.push(`  ${r.status.toUpperCase().padEnd(4)} ${r.id} ${r.title}`);
    lines.push(`Report: ${file}`);
    console.log(lines.join("\n"));
    return this.results.every((r) => r.status === "pass");
  }
}
