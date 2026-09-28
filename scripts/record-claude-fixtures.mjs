#!/usr/bin/env node
// Records what the user's real, locally installed Claude Code CLI prints in
// headless streaming mode, for the agent runtime's parser tests (Phase A
// plan, Task 0.2; Phase B plan, Task PL). Run on a machine with `claude`
// installed and logged in, from the repo root:
//
//   node scripts/record-claude-fixtures.mjs            # every scenario
//   node scripts/record-claude-fixtures.mjs f_propose_plan   # just some
//
// Writes crates/core/tests/fixtures/claude/*. Home directory, username, git
// email, the temp project path and the model identifier are scrubbed to
// placeholders. Review the output before committing it.
//
// The CLI runs in a clean environment: only PATH, a fresh empty HOME (so no
// user config, plugins, settings or memory), and the proxy/certificate
// variables a sandboxed machine may need to reach the API. The CLI finds its
// own sign-in; this script never reads or passes credentials. Set
// RECORD_INHERIT_ENV=1 to run with your whole environment instead.
import { spawnSync } from "node:child_process";
import { cpSync, mkdtempSync, mkdirSync, writeFileSync, readFileSync, realpathSync } from "node:fs";
import { tmpdir, homedir, userInfo } from "node:os";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const outDir = process.env.FIXTURE_OUT ?? join(repoRoot, "crates/core/tests/fixtures/claude");
mkdirSync(outDir, { recursive: true });

// Scenario names given on the command line; none means all of them.
const only = process.argv.slice(2);
const wanted = (name) => only.length === 0 || only.includes(name);

const project = realpathSync(mkdtempSync(join(tmpdir(), "ib-claude-fixture-")));
writeFileSync(join(project, "notes.txt"), "hello\n");

const cleanHome = realpathSync(mkdtempSync(join(tmpdir(), "ib-claude-home-")));
const PASS_THROUGH = [
  "HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy", "NO_PROXY", "no_proxy",
  "NODE_EXTRA_CA_CERTS", "SSL_CERT_FILE", "SSL_CERT_DIR",
];
const env =
  process.env.RECORD_INHERIT_ENV === "1"
    ? process.env
    : Object.fromEntries(
        [["PATH", process.env.PATH], ["HOME", cleanHome], ...PASS_THROUGH.map((k) => [k, process.env[k]])].filter(
          ([, v]) => v !== undefined,
        ),
      );

const gitEmail = spawnSync("git", ["config", "user.email"], { encoding: "utf8" }).stdout.trim();
const replacements = [
  [project, "<PROJECT>"],
  [repoRoot, "<REPO>"],
  [cleanHome, "<HOME>"],
  [homedir(), "<HOME>"],
  [userInfo().username, "<USER>"],
  ...(gitEmail ? [[gitEmail, "<EMAIL>"]] : []),
];
const scrubText = (text, extra = []) =>
  [...replacements, ...extra].reduce((acc, [from, to]) => (from ? acc.split(from).join(to) : acc), text);

// The init message lists everything installed on this machine (tools, slash
// commands, skills, plugins, agents) plus local socket/memory paths. None of
// that is needed by the parser and it differs per machine, so those fields
// are emptied/removed, and rate-limit events are reduced to their status. Every other line is kept byte-for-byte (apart from
// the text replacements above).
const INIT_LIST_FIELDS = ["tools", "slash_commands", "terminal_slash_commands", "skills", "plugins", "agents", "capabilities"];
const INIT_DROP_FIELDS = ["memory_paths", "messaging_socket_path"];
function scrubLine(line) {
  let msg;
  try {
    msg = JSON.parse(line);
  } catch {
    return line;
  }
  // Rate-limit events describe the account's plan; keep only whether the
  // request was allowed.
  if (msg?.type === "rate_limit_event" && msg.rate_limit_info) {
    msg.rate_limit_info = { status: msg.rate_limit_info.status };
    return JSON.stringify(msg);
  }
  if (msg?.type !== "system" || msg?.subtype !== "init") return line;
  for (const f of INIT_LIST_FIELDS) if (Array.isArray(msg[f])) msg[f] = [];
  for (const f of INIT_DROP_FIELDS) delete msg[f];
  return JSON.stringify(msg);
}
// The model identifier the init message names (it also appears on every
// assistant message and in the result's per-model usage) becomes <MODEL>.
function initModel(text) {
  for (const line of text.split("\n")) {
    try {
      const msg = JSON.parse(line);
      if (msg?.type === "system" && msg?.subtype === "init" && typeof msg.model === "string") return msg.model;
    } catch {}
  }
  return null;
}
const scrub = (text, model) =>
  scrubText(text.split("\n").map(scrubLine).join("\n"), model ? [[model, "<MODEL>"]] : []);

function claude(args, cwd = project) {
  const res = spawnSync("claude", args, { cwd, env, encoding: "utf8", timeout: 300_000 });
  if (res.error) throw res.error;
  return { stdout: res.stdout ?? "", stderr: res.stderr ?? "", status: res.status };
}

function record(name, args, cwd = project) {
  if (!wanted(name)) return null;
  console.log(`recording ${name}: claude ${args.join(" ")}`);
  const res = claude(args, cwd);
  const model = initModel(res.stdout);
  writeFileSync(join(outDir, `${name}.jsonl`), scrub(res.stdout, model));
  writeFileSync(join(outDir, `${name}.stderr.txt`), scrub(res.stderr, model));
  writeFileSync(join(outDir, `${name}.exit.txt`), `${res.status}\n`);
  return res;
}

const STREAM = ["--output-format", "stream-json", "--verbose"];

// (a) plain text answer (also needed for c)
const a = record("a_plain_text", ["-p", "Reply with exactly: hello from the fixture", ...STREAM]);

// (b) a turn that edits a file
record("b_edit_file", [
  "-p",
  "Append the line 'world' to notes.txt. Do nothing else.",
  ...STREAM,
  "--allowedTools",
  "Read,Edit,Write",
]);

// (c) a resumed second turn, continuing (a)'s session
const init = (a?.stdout ?? "")
  .split("\n")
  .filter(Boolean)
  .map((l) => {
    try {
      return JSON.parse(l);
    } catch {
      return null;
    }
  })
  .find((m) => m && m.session_id);
if (init) {
  record("c_resumed_turn", ["-p", "What did you say last time? Answer in one sentence.", ...STREAM, "--resume", init.session_id]);
} else if (wanted("c_resumed_turn")) {
  console.warn("could not find a session_id in (a); skipping (c)");
}

// (d) a turn that calls a tool from an MCP server passed with --mcp-config
const echoServer = join(repoRoot, "scripts/fixtures/echo-mcp-server.mjs");
const mcpConfig = join(project, "mcp.json");
writeFileSync(mcpConfig, JSON.stringify({ mcpServers: { fixture: { command: "node", args: [echoServer] } } }));
record("d_mcp_tool", [
  "-p",
  "Call the echo tool with the text 'ping', then tell me what it returned.",
  ...STREAM,
  "--mcp-config",
  mcpConfig,
  "--allowedTools",
  "mcp__fixture__echo",
]);

// (e) an error: resuming a session that doesn't exist
record("e_bad_resume", ["-p", "hi", ...STREAM, "--resume", "00000000-0000-0000-0000-000000000000"]);

// (f) a plan turn, run the way ClaudeCodeRuntime runs one (its flags, and
// the system prompt `prompt::system_prompt` builds for the default options:
// the Director prompt, the "plans come first" section and "how to finish"),
// against the echo server offering `propose_plan` under the name
// `infinabox`, so the tool is `mcp__infinabox__propose_plan` as in the app.
// It runs in a copy of the blank-2d template (so there's a real game to
// plan for), with its AGENTS.md appended the way the runtime does, and the
// MCP config kept outside the project.
const gameDir = realpathSync(mkdtempSync(join(tmpdir(), "ib-claude-fixture-game-")));
replacements.unshift([gameDir, "<PROJECT>"]);
cpSync(join(repoRoot, "templates/blank-2d"), gameDir, { recursive: true });
const agentsMd = readFileSync(join(gameDir, "AGENTS.md"), "utf8").split("{{PROJECT_NAME}}").join("Sky Hopper");
writeFileSync(join(gameDir, "AGENTS.md"), agentsMd);
const concept = join(gameDir, ".ibproject/context/concept.md");
writeFileSync(concept, readFileSync(concept, "utf8").split("{{PROJECT_NAME}}").join("Sky Hopper"));
const prompts = join(repoRoot, "crates/core/src/agent/prompts");
const planPrompt =
  ["director.md", "plan-always.md", "explain.md"].map((f) => readFileSync(join(prompts, f), "utf8").trimEnd()).join("\n\n") +
  `\n\n---\n\n# This project's AGENTS.md\n\n${agentsMd.trim()}\n`;
const configDir = realpathSync(mkdtempSync(join(tmpdir(), "ib-claude-fixture-config-")));
replacements.unshift([configDir, "<CONFIG>"]);
const planConfig = join(configDir, "mcp-plan.json");
writeFileSync(
  planConfig,
  JSON.stringify({ mcpServers: { infinabox: { command: "node", args: [echoServer, "--propose-plan"] } } }),
);
record("f_propose_plan", [
  "-p",
  ...STREAM,
  "--setting-sources",
  "user",
  "--mcp-config",
  planConfig,
  "--strict-mcp-config",
  "--tools",
  "Read,Edit,Write,Glob,Grep",
  "--allowedTools",
  "Read,Edit,Write,Glob,Grep,mcp__infinabox__*",
  "--permission-mode",
  "acceptEdits",
  "--append-system-prompt",
  planPrompt,
  "--",
  "Add a double jump to my game, so the player can jump once more in the air.",
], gameDir);

// Version and help, so the flags the runtime relies on are on record.
if (only.length === 0) {
  writeFileSync(join(outDir, "version.txt"), scrubText(claude(["--version"]).stdout));
  writeFileSync(join(outDir, "help.txt"), scrubText(claude(["--help"]).stdout));
}

console.log(`\nWrote fixtures to ${outDir}. Review them for anything personal before committing.`);
