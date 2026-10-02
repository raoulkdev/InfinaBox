#!/usr/bin/env node
// A minimal, dependency-free MCP server over stdio with one tool, `echo`.
// Used by the fixture recorders to capture how a real agent CLI reports an
// MCP tool call in its stream (Claude fixture scenario d).
//
// With `--propose-plan` it also offers `propose_plan`, with the same name,
// description, input schema, limits and result text as the real InfinaBox
// MCP server's tool (`crates/mcp-server/src/server.rs`; copied from its real
// `tools/list` output — keep the two in sync). Register it under the server
// name `infinabox` so the CLI calls it `mcp__infinabox__propose_plan`, the
// name the stream parsers look for (Claude fixture scenario f). Without the
// flag the server behaves exactly as before.
import { createInterface } from "node:readline";

const offerPlan = process.argv.includes("--propose-plan");

const ECHO_TOOL = {
  name: "echo",
  description: "Echoes the given text back.",
  inputSchema: {
    type: "object",
    properties: { text: { type: "string" } },
    required: ["text"],
  },
};

const PLAN_TOOL = {
  name: "propose_plan",
  description:
    "Show the person a plan for a change to their game, as a card with Approve and Change buttons. Use it before changing the game (unless your instructions say this turn doesn't need a plan): read the relevant Context cards first, then give a short title and 2-6 plain-language steps (no code, no file paths unless they help). After calling it, end your turn with one short sentence and make no changes until the person approves.",
  inputSchema: {
    $schema: "https://json-schema.org/draft/2020-12/schema",
    properties: {
      steps: {
        description:
          "The steps in order, as plain sentences about what will change in the game (no code). 2-6 steps is best; at most 8, each at most 200 characters.",
        items: { type: "string" },
        type: "array",
      },
      title: {
        description: 'A short title in plain words, at most 80 characters, e.g. "Add a double jump".',
        type: "string",
      },
    },
    required: ["title", "steps"],
    type: "object",
  },
};

const PLAN_SHOWN =
  "The plan is now shown to the person with Approve and Change buttons. End your turn now with one short sentence, and don't change anything: you'll get a new message when they approve it or ask for changes.";

// The real server's limits (`prompt::validate_plan`), in brief.
function planError(args) {
  const title = typeof args?.title === "string" ? args.title.trim() : "";
  const steps = Array.isArray(args?.steps) ? args.steps : [];
  if (!title) return "The plan needs a short title.";
  if ([...title].length > 80) return "The plan's title is too long: keep it to 80 characters.";
  if (steps.length === 0) return "The plan needs at least one step.";
  if (steps.length > 8) return `The plan has ${steps.length} steps: keep it to 8 at most (2–6 is best).`;
  for (const [i, step] of steps.entries()) {
    if (typeof step !== "string" || !step.trim()) return `Step ${i + 1} of the plan is empty.`;
    if ([...step.trim()].length > 200)
      return `Step ${i + 1} of the plan is too long: keep each step to 200 characters.`;
  }
  return null;
}

const rl = createInterface({ input: process.stdin });
const send = (msg) => process.stdout.write(JSON.stringify(msg) + "\n");
const text = (id, t, isError = false) =>
  send({ jsonrpc: "2.0", id, result: { content: [{ type: "text", text: t }], ...(isError ? { isError: true } : {}) } });

rl.on("line", (line) => {
  let req;
  try {
    req = JSON.parse(line);
  } catch {
    return;
  }
  // Notifications (no id) need no reply.
  if (req.id === undefined) return;

  switch (req.method) {
    case "initialize":
      send({
        jsonrpc: "2.0",
        id: req.id,
        result: {
          protocolVersion: req.params?.protocolVersion ?? "2025-06-18",
          capabilities: { tools: {} },
          serverInfo: { name: "fixture", version: "1.0.0" },
        },
      });
      break;
    case "tools/list":
      send({
        jsonrpc: "2.0",
        id: req.id,
        result: { tools: offerPlan ? [ECHO_TOOL, PLAN_TOOL] : [ECHO_TOOL] },
      });
      break;
    case "tools/call":
      if (offerPlan && req.params?.name === "propose_plan") {
        const error = planError(req.params?.arguments);
        text(req.id, error ?? PLAN_SHOWN, error !== null);
      } else {
        text(req.id, `echo: ${req.params?.arguments?.text ?? ""}`);
      }
      break;
    default:
      send({ jsonrpc: "2.0", id: req.id, error: { code: -32601, message: "Method not found" } });
  }
});
