#!/usr/bin/env node
// Records what the user's real, locally installed Claude Code CLI prints in
// headless streaming mode, for the Phase A agent runtime's parser tests
// (Phase A plan, Task 0.2). Run on a machine with `claude` installed and
// logged in, from the repo root:
//
//   node scripts/record-claude-fixtures.mjs
//
// Writes crates/core/tests/fixtures/claude/*. Home directory, username, git
// email, and the temp project path are scrubbed to placeholders. Review the
// output before committing it.
import { spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync, realpathSync } from "node:fs";
import { tmpdir, homedir, userInfo } from "node:os";
import { join, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const outDir = process.env.FIXTURE_OUT ?? join(repoRoot, "crates/core/tests/fixtures/claude");
mkdirSync(outDir, { recursive: true });

const project = realpathSync(mkdtempSync(join(tmpdir(), "ib-claude-fixture-")));
writeFileSync(join(project, "notes.txt"), "hello\n");

const gitEmail = spawnSync("git", ["config", "user.email"], { encoding: "utf8" }).stdout.trim();
const replacements = [
  [project, "<PROJECT>"],
  [homedir(), "<HOME>"],
  [userInfo().username, "<USER>"],
  ...(gitEmail ? [[gitEmail, "<EMAIL>"]] : []),
];
const scrub = (text) =>
  replacements.reduce((acc, [from, to]) => (from ? acc.split(from).join(to) : acc), text);

function claude(args) {
  const res = spawnSync("claude", args, { cwd: project, encoding: "utf8", timeout: 300_000 });
  if (res.error) throw res.error;
  return { stdout: res.stdout ?? "", stderr: res.stderr ?? "", status: res.status };
}

function record(name, args) {
  console.log(`recording ${name}: claude ${args.join(" ")}`);
  const res = claude(args);
  writeFileSync(join(outDir, `${name}.jsonl`), scrub(res.stdout));
  writeFileSync(join(outDir, `${name}.stderr.txt`), scrub(res.stderr));
  writeFileSync(join(outDir, `${name}.exit.txt`), `${res.status}\n`);
  return res;
}

const STREAM = ["--output-format", "stream-json", "--verbose"];

// (a) plain text answer
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
const init = a.stdout
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
} else {
  console.warn("could not find a session_id in (a); skipping (c)");
}

// (d) a turn that calls a tool from an MCP server passed with --mcp-config
const mcpConfig = join(project, "mcp.json");
writeFileSync(
  mcpConfig,
  JSON.stringify({
    mcpServers: { fixture: { command: "node", args: [join(repoRoot, "scripts/fixtures/echo-mcp-server.mjs")] } },
  }),
);
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

// Version and help, so the flags the runtime relies on are on record.
writeFileSync(join(outDir, "version.txt"), scrub(claude(["--version"]).stdout));
writeFileSync(join(outDir, "help.txt"), scrub(claude(["--help"]).stdout));

console.log(`\nWrote fixtures to ${outDir}. Review them for anything personal before committing.`);
