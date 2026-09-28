#!/usr/bin/env node
// Records what the real Codex CLI prints for `codex exec --json`, for the
// Codex runtime's parser tests (Phase B plan, Task CX). Run from the repo
// root on a machine with `codex` installed (it doesn't need to be signed in):
//
//   node scripts/record-codex-fixtures.mjs            # uses `codex` on PATH
//   CODEX_BIN=/path/to/codex node scripts/record-codex-fixtures.mjs
//
// Most scenarios point the real CLI at scripts/fixtures/mock-responses-server.mjs
// (a scripted local Responses API endpoint, configured as a custom
// `model_providers` entry with a fake key), so every line is genuinely the
// CLI's own output without an account or a network; the model's side of the
// conversation is the only thing scripted. The error scenarios use the real
// default provider with a fresh, signed-out CODEX_HOME.
//
// Each run uses the same restriction flags as `crates/core/src/agent/codex.rs`
// (`RESTRICTIONS` below mirrors its `build_args`). Writes
// crates/core/tests/fixtures/codex/*: `<name>.jsonl` (stdout),
// `<name>.stderr.txt`, `<name>.exit.txt`, plus `version.txt`,
// `help.txt` (`codex exec --help`) and `resume-help.txt`. The project path,
// CODEX_HOME, repo path, home directory and username are scrubbed to
// placeholders. Review the output before committing it.
import { spawn, spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, realpathSync, rmSync } from "node:fs";
import { tmpdir, homedir, userInfo } from "node:os";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { createInterface } from "node:readline";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const outDir = process.env.FIXTURE_OUT ?? join(repoRoot, "crates/core/tests/fixtures/codex");
const codexBin = process.env.CODEX_BIN ?? "codex";
const mockServer = join(repoRoot, "scripts/fixtures/mock-responses-server.mjs");
mkdirSync(outDir, { recursive: true });

const scratch = realpathSync(mkdtempSync(join(tmpdir(), "ibx-codex-fixture-")));
const project = join(scratch, "project");
const codexHome = join(scratch, "codex-home");
const freshHome = join(scratch, "codex-home-signed-out");
for (const d of [project, codexHome, freshHome]) mkdirSync(d, { recursive: true });

// A real git project like the ones InfinaBox makes, with an AGENTS.md.
const resetProject = () => {
  writeFileSync(join(project, "notes.txt"), "hello\n");
};
resetProject();
writeFileSync(join(project, "AGENTS.md"), "# Fixture project\n\nKeep replies short.\n");
const git = (...args) => spawnSync("git", args, { cwd: project, encoding: "utf8" });
git("init", "-q", ".");
git("add", "-A");
git("-c", "user.email=fixture@example.com", "-c", "user.name=fixture", "commit", "-qm", "Fixture project");

const replacements = [
  [project, "<PROJECT>"],
  [freshHome, "<CODEX_HOME>"],
  [codexHome, "<CODEX_HOME>"],
  [scratch, "<TMP>"],
  [repoRoot, "<REPO>"],
  [homedir(), "<HOME>"],
  [userInfo().username, "<USER>"],
];
const scrub = (text) => replacements.reduce((acc, [from, to]) => (from ? acc.split(from).join(to) : acc), text);

// The bridge token the app would pass; it must reach the MCP server through
// the environment and never appear on the command line or in the output.
const MCP_ENV = { INFINABOX_PROJECT: project, INFINABOX_BRIDGE_TOKEN: "fixture-bridge-token" };
const envNames = Object.keys(MCP_ENV);
const tomlList = (items) => `[${items.map((s) => JSON.stringify(s)).join(",")}]`;

// Mirrors `build_args` in crates/core/src/agent/codex.rs (see its docs for
// why each is there). The instructions are a short stand-in for the Director
// prompt, which is long and not what these recordings are about.
const RESTRICTIONS = [
  "--json",
  "--skip-git-repo-check",
  "--ignore-user-config",
  "--ignore-rules",
  "--disable", "apps",
  "--disable", "plugins",
  "--disable", "hooks",
  "--sandbox", "workspace-write",
  "-C", project,
  "-c", `mcp_servers.infinabox.command=${JSON.stringify(process.execPath)}`,
  "-c", `mcp_servers.infinabox.args=${tomlList([mockServer, "--mcp"])}`,
  "-c", `mcp_servers.infinabox.env_vars=${tomlList(envNames)}`,
  "-c", `mcp_servers.infinabox.default_tools_approval_mode="approve"`,
  "-c", `shell_environment_policy.exclude=${tomlList(envNames)}`,
  "-c", `developer_instructions=${JSON.stringify("You are the InfinaBox Director (fixture stand-in).")}`,
];

// The mock provider: a custom model provider on the local mock server, and a
// model catalog entry for the scripted model (so Codex offers its own
// apply_patch tool the way it does for its real models).
const catalog = join(scratch, "catalog.json");
writeFileSync(
  catalog,
  JSON.stringify({
    models: [
      {
        slug: "mock-model",
        display_name: "Mock",
        description: "Scripted model served by mock-responses-server.mjs",
        apply_patch_tool_type: "freeform",
        shell_type: "shell_command",
        supported_in_api: true,
        visibility: "list",
        priority: 1,
        supported_reasoning_levels: [],
        default_reasoning_level: null,
        base_instructions: "You are a scripted test model.",
        support_verbosity: false,
        truncation_policy: { mode: "tokens", limit: 10000 },
        experimental_supported_tools: [],
      },
    ],
  }),
);
const mockProvider = (port) => [
  "-c", "model_provider=mock",
  "-c", `model_providers.mock={name="mock",base_url="http://127.0.0.1:${port}/v1",env_key="MOCK_API_KEY",wire_api="responses"}`,
  "-c", `model_catalog_json=${JSON.stringify(catalog)}`,
  "-m", "mock-model",
];

const baseEnv = {
  ...process.env,
  ...MCP_ENV,
  MOCK_API_KEY: "sk-mock-not-a-real-key",
  NO_PROXY: [process.env.NO_PROXY, "127.0.0.1", "localhost"].filter(Boolean).join(","),
};

function startMock() {
  return new Promise((resolvePort, reject) => {
    const child = spawn(process.execPath, [mockServer], { stdio: ["ignore", "pipe", "inherit"] });
    child.on("error", reject);
    createInterface({ input: child.stdout }).once("line", (line) => resolvePort({ child, port: JSON.parse(line).port }));
  });
}

function save(name, args, res) {
  writeFileSync(join(outDir, `${name}.jsonl`), scrub(res.stdout));
  writeFileSync(join(outDir, `${name}.stderr.txt`), scrub(res.stderr));
  writeFileSync(join(outDir, `${name}.exit.txt`), `${res.status ?? `signal ${res.signal}`}\n`);
  if (res.stdout.includes("fixture-bridge-token") || args.join(" ").includes("fixture-bridge-token")) {
    throw new Error(`${name}: the bridge token leaked into the output or the arguments`);
  }
}

/** Runs `codex exec <args>` to completion. */
function run(name, args, home) {
  console.log(`recording ${name}`);
  const res = spawnSync(codexBin, ["exec", ...args], {
    cwd: project,
    env: { ...baseEnv, CODEX_HOME: home },
    encoding: "utf8",
    timeout: 120_000,
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (res.error) throw res.error;
  save(name, args, res);
  return res;
}

/** Runs `codex exec <args>` in its own process group and stops the group
 * with SIGTERM once `stopWhen(line)` matches a stdout line (as the app's
 * Stop does). */
function runAndStop(name, args, home, stopWhen) {
  console.log(`recording ${name}`);
  return new Promise((done, fail) => {
    const child = spawn(codexBin, ["exec", ...args], {
      cwd: project,
      env: { ...baseEnv, CODEX_HOME: home },
      stdio: ["ignore", "pipe", "pipe"],
      detached: true,
    });
    let stdout = "";
    let stderr = "";
    let stopped = false;
    const timer = setTimeout(() => {
      try {
        process.kill(-child.pid, "SIGKILL");
      } catch {}
      fail(new Error(`${name}: never reached the stop point`));
    }, 90_000);
    child.stdout.on("data", (d) => {
      stdout += d;
      if (!stopped && stdout.split("\n").some(stopWhen)) {
        stopped = true;
        // Give the CLI a moment to be mid-stream, then stop it.
        setTimeout(() => process.kill(-child.pid, "SIGTERM"), 500);
        // Like the runtime: SIGKILL if it's still there after the grace period.
        setTimeout(() => {
          try {
            process.kill(-child.pid, "SIGKILL");
          } catch {}
        }, 3500);
      }
    });
    child.stderr.on("data", (d) => (stderr += d));
    child.on("close", (status, signal) => {
      clearTimeout(timer);
      const res = { stdout, stderr, status, signal };
      save(name, args, res);
      done(res);
    });
  });
}

const threadId = (stdout) =>
  stdout
    .split("\n")
    .map((l) => {
      try {
        return JSON.parse(l);
      } catch {
        return null;
      }
    })
    .find((m) => m?.type === "thread.started")?.thread_id;

const { child: mock, port } = await startMock();
// The mock's port is picked fresh each run; errors quote its URL.
replacements.unshift([`127.0.0.1:${port}`, "127.0.0.1:<PORT>"]);
try {
  const withMock = (message, resumeId) => [
    ...RESTRICTIONS,
    ...mockProvider(port),
    ...(resumeId ? ["resume", resumeId] : []),
    "--",
    message,
  ];

  // (a) a plain text reply
  const a = run("a_plain_text", withMock("SCENARIO=text Reply with exactly: hello from the fixture"), codexHome);
  // (b) an edit with Codex's own apply_patch tool (a real change to notes.txt)
  run("b_edit_file", withMock("SCENARIO=edit Append the line 'world' to notes.txt."), codexHome);
  const edited = readFileSync(join(project, "notes.txt"), "utf8");
  if (edited !== "hello\nworld\n") throw new Error(`b_edit_file: notes.txt is ${JSON.stringify(edited)}`);
  resetProject();
  // (c) a resumed turn, continuing (a)'s thread
  const aThread = threadId(a.stdout);
  if (!aThread) throw new Error("no thread id in (a)");
  run("c_resumed_turn", withMock("SCENARIO=resume What did you say last time?", aThread), codexHome);
  // (d) an MCP tool call (the echo tool; the server is registered as `infinabox`)
  run("d_mcp_tool", withMock("SCENARIO=mcp Call the echo tool with 'ping'."), codexHome);
  // (e) a propose_plan call, and (f) one with malformed input
  run("e_propose_plan", withMock("SCENARIO=plan Add a double jump."), codexHome);
  run("f_bad_plan", withMock("SCENARIO=bad_plan Add a double jump."), codexHome);
  // (g) a shell command (Codex's own shell tool)
  run("g_shell_command", withMock("SCENARIO=shell Is the bridge token visible?"), codexHome);
  // (h) cancelled mid-stream: an edit lands, then the reply hangs and the
  // process group gets SIGTERM
  await runAndStop("h_cancelled", withMock("SCENARIO=hang Append 'world' to notes.txt, then explain."), codexHome, (l) =>
    l.includes('"type":"item.completed"') && l.includes('"file_change"'),
  );
  resetProject();
  // (i) the provider rejects the credentials (HTTP 401 from the mock).
  run("i_unauthorized", withMock("SCENARIO=unauthorized hi"), codexHome);
} finally {
  mock.kill();
}

// (j) a resume id the CLI doesn't know (real default provider, signed out).
run("j_bad_resume", [...RESTRICTIONS, "resume", "00000000-0000-0000-0000-000000000000", "--", "hi"], freshHome);
// (k) not signed in, real default provider, fresh CODEX_HOME. Codex doesn't
// check sign-in before sending, so what it prints next depends on the
// network: where the API can't be reached it retries without end, so this
// one is stopped (SIGTERM) once it says it's waiting for the network, or
// after it finishes on its own.
await runAndStop("k_not_logged_in", [...RESTRICTIONS, "--", "Reply with exactly: hello"], freshHome, (l) =>
  l.includes("waiting for network"),
).catch((e) => console.warn(`k_not_logged_in: ${e.message}`));

// Version, help, and sign-in status, so what the runtime relies on is on record.
const plain = (args, home = freshHome) =>
  spawnSync(codexBin, args, { env: { ...process.env, CODEX_HOME: home }, encoding: "utf8" });
writeFileSync(join(outDir, "version.txt"), scrub(plain(["--version"]).stdout));
writeFileSync(join(outDir, "help.txt"), scrub(plain(["exec", "--help"]).stdout));
writeFileSync(join(outDir, "resume-help.txt"), scrub(plain(["exec", "resume", "--help"]).stdout));
const status = plain(["login", "status"]);
writeFileSync(
  join(outDir, "login_status_signed_out.txt"),
  scrub(`exit: ${status.status}\nstdout: ${status.stdout}stderr: ${status.stderr}`),
);
// Signed in (with a fake API key in a throwaway CODEX_HOME — `login status`
// only reads the stored credentials, it doesn't check them online).
const signedInHome = join(scratch, "codex-home-signed-in");
mkdirSync(signedInHome, { recursive: true });
replacements.unshift([signedInHome, "<CODEX_HOME>"]);
spawnSync(codexBin, ["login", "--with-api-key"], {
  env: { ...process.env, CODEX_HOME: signedInHome },
  input: "sk-mock-not-a-real-key-0000\n",
  encoding: "utf8",
});
const signedIn = plain(["login", "status"], signedInHome);
writeFileSync(
  join(outDir, "login_status_signed_in.txt"),
  scrub(`exit: ${signedIn.status}\nstdout: ${signedIn.stdout}stderr: ${signedIn.stderr}`),
);

rmSync(scratch, { recursive: true, force: true });
console.log(`\nWrote fixtures to ${outDir}. Review them before committing.`);
