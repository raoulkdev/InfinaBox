// Launching the real InfinaBox binary under tauri-driver and opening a
// WebDriver session on its webview.
//
// tauri-driver is started fresh for every app launch, with the environment
// that launch needs: tauri-driver starts WebKitWebDriver, which starts the
// app, and each hands its environment down. That is how a scenario gives
// the app its own HOME/XDG dirs (so app data and webview localStorage start
// empty and nothing persists) and chooses whether INFINABOX_GODOT is set.

import { spawn } from "node:child_process";
import { createWriteStream } from "node:fs";
import net from "node:net";
import path from "node:path";
import { Builder, Capabilities } from "selenium-webdriver";

function freePort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.unref();
    server.on("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const { port } = server.address();
      server.close(() => resolve(port));
    });
  });
}

async function waitForPort(port, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const ok = await new Promise((resolve) => {
      const socket = net.connect(port, "127.0.0.1");
      socket.once("connect", () => {
        socket.destroy();
        resolve(true);
      });
      socket.once("error", () => resolve(false));
    });
    if (ok) return;
    await new Promise((r) => setTimeout(r, 100));
  }
  throw new Error(`tauri-driver didn't start listening on port ${port} within ${timeoutMs}ms`);
}

/**
 * Starts the app and returns `{ driver, stop }`.
 *
 * @param {object} opts
 * @param {string} opts.binary        Path to the built app (`target/debug/tauri-app`).
 * @param {string} opts.tauriDriver   Path to `tauri-driver`.
 * @param {string} [opts.nativeDriver] Path to `WebKitWebDriver`.
 * @param {Record<string,string|undefined>} opts.env Full environment for the app
 *   (keys set to `undefined` are removed).
 * @param {string} opts.logDir        Where tauri-driver's (and so the app's) output goes.
 * @param {string} opts.label         Used in the log file name.
 */
export async function startApp({ binary, tauriDriver, nativeDriver, env, logDir, label }) {
  const port = await freePort();
  const nativePort = await freePort();
  const cleanEnv = {};
  for (const [k, v] of Object.entries(env)) if (v !== undefined) cleanEnv[k] = v;

  const args = ["--port", String(port), "--native-port", String(nativePort)];
  if (nativeDriver) args.push("--native-driver", nativeDriver);
  const child = spawn(tauriDriver, args, { env: cleanEnv, stdio: ["ignore", "pipe", "pipe"] });
  const logFile = path.join(logDir, `${label}.app.log`);
  const log = createWriteStream(logFile);
  child.stdout.pipe(log);
  child.stderr.pipe(log);
  let exited = false;
  child.on("exit", () => {
    exited = true;
  });

  await waitForPort(port, 15_000);

  const caps = new Capabilities();
  caps.setBrowserName("wry");
  caps.set("tauri:options", { application: binary });
  let driver;
  try {
    driver = await new Builder()
      .withCapabilities(caps)
      .usingServer(`http://127.0.0.1:${port}/`)
      .build();
  } catch (err) {
    child.kill("SIGTERM");
    throw new Error(`Couldn't open a WebDriver session on the app (see ${logFile}): ${err.message}`);
  }

  async function stop() {
    try {
      await driver.quit();
    } catch {
      // The session may already be gone (e.g. the app crashed); the
      // tauri-driver kill below still cleans up.
    }
    if (!exited) {
      child.kill("SIGTERM");
      await new Promise((resolve) => {
        const timer = setTimeout(() => {
          child.kill("SIGKILL");
          resolve();
        }, 5000);
        child.once("exit", () => {
          clearTimeout(timer);
          resolve();
        });
      });
    }
    log.end();
  }

  return { driver, stop, logFile };
}
