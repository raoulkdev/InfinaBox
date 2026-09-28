#!/usr/bin/env node
// A scripted, dependency-free stand-in for an OpenAI-compatible Responses
// API endpoint, so the real `codex` CLI can be recorded (and tested end to
// end) without an account or a network: Codex is pointed at it through a
// custom `model_providers` entry (`wire_api = "responses"`), and everything
// Codex prints is then genuinely the CLI's own output. Used by
// `scripts/record-codex-fixtures.mjs` and the Codex runtime's `#[ignore]`d
// end-to-end test.
//
//   node scripts/fixtures/mock-responses-server.mjs [--port N]
//     Serves POST .../responses on 127.0.0.1 (a free port unless given) and
//     prints `{"port":N}` on stdout once it's listening.
//
//   node scripts/fixtures/mock-responses-server.mjs --mcp
//     Runs instead as a minimal stdio MCP server with two tools: `echo`
//     (like scripts/fixtures/echo-mcp-server.mjs, plus whether the bridge
//     token reached it through the environment — never the token itself)
//     and `propose_plan` (answering like InfinaBox's own server).
//
// The reply is chosen by a `SCENARIO=<name>` word in the latest user message,
// and by how many tool results have come back since that message (the step):
//
//   text       reply "hello from the fixture"
//   edit       apply_patch adding "world" to notes.txt, then a short reply
//   mcp        call the MCP `echo` tool with "ping", then repeat its result
//   plan       call `propose_plan` with a title and two steps, then a reply
//   bad_plan   call `propose_plan` with malformed input (steps not a list)
//   shell      run a shell command (whether the bridge token is visible to
//              it), then repeat its output
//   resume     reply with the first assistant message found in the history
//              (proves a resumed turn carries the earlier conversation)
//   unauthorized  answer HTTP 401 with an error body, the way the API
//              rejects a request without valid credentials
//   hang       edit notes.txt, then start a reply and never finish it (for
//              cancelling a turn mid-stream)
//
// Every request body is appended to $MOCK_RESPONSES_LOG (if set) as one JSON
// line, so tests can check what the CLI sent (instructions, tools).
import { createServer } from "node:http";
import { appendFileSync } from "node:fs";
import { createInterface } from "node:readline";

if (process.argv.includes("--mcp")) {
  runMcpServer();
} else {
  const portArg = process.argv.indexOf("--port");
  runResponsesServer(portArg > 0 ? Number(process.argv[portArg + 1]) : 0);
}

function runResponsesServer(port) {
  let responseCount = 0;
  const server = createServer((req, res) => {
    let body = "";
    req.on("data", (chunk) => (body += chunk));
    req.on("end", () => {
      if (req.method !== "POST" || !req.url.endsWith("/responses")) {
        res.writeHead(404, { "content-type": "application/json" });
        res.end(JSON.stringify({ error: { message: `no route for ${req.method} ${req.url}` } }));
        return;
      }
      if (process.env.MOCK_RESPONSES_LOG) appendFileSync(process.env.MOCK_RESPONSES_LOG, body.replace(/\n/g, " ") + "\n");
      let request;
      try {
        request = JSON.parse(body);
      } catch {
        res.writeHead(400);
        res.end();
        return;
      }
      responseCount += 1;
      respond(res, request, `resp_${responseCount}`);
    });
  });
  server.listen(port, "127.0.0.1", () => {
    process.stdout.write(JSON.stringify({ port: server.address().port }) + "\n");
  });
}

/** The text of an input message item. */
function messageText(item) {
  if (typeof item.content === "string") return item.content;
  return (item.content ?? []).map((c) => c.text ?? "").join("");
}

function respond(res, request, responseId) {
  const input = Array.isArray(request.input) ? request.input : [];
  // The latest real user message (Codex also sends its environment context
  // and instructions as user/developer messages).
  let userIndex = -1;
  for (let i = input.length - 1; i >= 0; i--) {
    const item = input[i];
    if (item.type === "message" && item.role === "user" && /SCENARIO=\w+/.test(messageText(item))) {
      userIndex = i;
      break;
    }
  }
  const userText = userIndex >= 0 ? messageText(input[userIndex]) : "";
  const scenario = userText.match(/SCENARIO=(\w+)/)?.[1] ?? "text";
  const step = input.slice(userIndex + 1).filter((i) => /_call_output$/.test(i.type ?? "")).length;
  const tools = Array.isArray(request.tools) ? request.tools : [];

  if (scenario === "unauthorized") {
    res.writeHead(401, { "content-type": "application/json" });
    res.end(
      JSON.stringify({
        error: { message: "Mock server: no valid credentials were sent.", type: "invalid_request_error", code: "invalid_api_key" },
      }),
    );
    return;
  }

  const sse = startStream(res, responseId);
  const say = (text) => sse.message(text);
  switch (scenario) {
    case "edit":
      if (step === 0) sse.applyPatch("*** Begin Patch\n*** Update File: notes.txt\n@@\n hello\n+world\n*** End Patch\n");
      else say("I added the line \"world\" to notes.txt.");
      break;
    case "mcp":
      if (step === 0) sse.mcpCall(tools, "echo", { text: "ping" });
      else say(`The echo tool returned: ${lastToolOutput(input)}`);
      break;
    case "plan":
      if (step === 0)
        sse.mcpCall(tools, "propose_plan", {
          title: "Add a double jump",
          steps: ["Let the player jump a second time in mid-air", "Play a small puff effect on the second jump"],
        });
      else say("I've shown you the plan. Approve it and I'll build it.");
      break;
    case "bad_plan":
      if (step === 0) sse.mcpCall(tools, "propose_plan", { title: "Add a double jump", steps: "jump twice" });
      else say("That plan didn't go through.");
      break;
    case "shell":
      if (step === 0) sse.functionCall("exec_command", { cmd: "printenv INFINABOX_BRIDGE_TOKEN || echo token-not-visible" });
      else say(`The command printed: ${lastToolOutput(input)}`);
      break;
    case "resume": {
      const earlier = input.find((i) => i.type === "message" && i.role === "assistant");
      say(earlier ? `Last time I said: ${messageText(earlier)}` : "I don't remember saying anything before.");
      break;
    }
    case "hang":
      if (step === 0) {
        sse.applyPatch("*** Begin Patch\n*** Update File: notes.txt\n@@\n hello\n+world\n*** End Patch\n");
      } else {
        // Start a message and never finish it: the turn stays open until the
        // client goes away.
        sse.event("response.output_item.added", {
          output_index: 0,
          item: { type: "message", id: "msg_hang", role: "assistant", status: "in_progress", content: [] },
        });
        sse.event("response.output_text.delta", { item_id: "msg_hang", output_index: 0, content_index: 0, delta: "Working on" });
        const keepAlive = setInterval(() => res.write(": still thinking\n\n"), 1000);
        res.on("close", () => clearInterval(keepAlive));
        return;
      }
      break;
    default:
      say("hello from the fixture");
  }
  sse.complete();
}

/** The text of the most recent tool output in the input. */
function lastToolOutput(input) {
  const out = [...input].reverse().find((i) => /_call_output$/.test(i.type ?? ""));
  if (!out) return "(nothing)";
  const o = out.output;
  if (typeof o === "string") {
    try {
      const parsed = JSON.parse(o);
      if (Array.isArray(parsed)) return parsed.map((c) => c.text ?? "").join("");
      if (Array.isArray(parsed?.content)) return parsed.content.map((c) => c.text ?? "").join("");
    } catch {}
    return o;
  }
  if (Array.isArray(o)) return o.map((c) => c.text ?? "").join("");
  return JSON.stringify(o);
}

/** A Server-Sent Events stream in the Responses API's shape. */
function startStream(res, responseId) {
  res.writeHead(200, { "content-type": "text/event-stream", "cache-control": "no-cache" });
  const output = [];
  const event = (type, data) => res.write(`event: ${type}\ndata: ${JSON.stringify({ type, ...data })}\n\n`);
  const base = { id: responseId, object: "response", created_at: Math.floor(Date.now() / 1000), model: "mock-model" };
  event("response.created", { response: { ...base, status: "in_progress", output: [] } });
  const addItem = (item) => {
    const index = output.length;
    output.push(item);
    event("response.output_item.added", { output_index: index, item });
    event("response.output_item.done", { output_index: index, item });
  };
  let callCount = 0;
  const callId = () => `call_${responseId}_${++callCount}`;
  return {
    event,
    message(text) {
      const id = `msg_${responseId}`;
      const index = output.length;
      const item = { type: "message", id, role: "assistant", status: "completed", content: [{ type: "output_text", text, annotations: [] }] };
      event("response.output_item.added", { output_index: index, item: { ...item, status: "in_progress", content: [] } });
      event("response.content_part.added", { item_id: id, output_index: index, content_index: 0, part: { type: "output_text", text: "", annotations: [] } });
      event("response.output_text.delta", { item_id: id, output_index: index, content_index: 0, delta: text });
      event("response.output_text.done", { item_id: id, output_index: index, content_index: 0, text });
      event("response.content_part.done", { item_id: id, output_index: index, content_index: 0, part: item.content[0] });
      output.push(item);
      event("response.output_item.done", { output_index: index, item });
    },
    applyPatch(patch) {
      // Codex offers apply_patch either as a freeform (custom) tool or as a
      // function tool taking `input`, depending on the model's metadata.
      addItem({ type: "custom_tool_call", id: `ctc_${responseId}`, status: "completed", call_id: callId(), name: "apply_patch", input: patch });
    },
    functionCall(name, args) {
      addItem({ type: "function_call", id: `fc_${responseId}`, status: "completed", call_id: callId(), name, arguments: JSON.stringify(args) });
    },
    mcpCall(tools, tool, args) {
      const name = findToolName(tools, tool);
      const item = { type: "function_call", id: `fc_${responseId}`, status: "completed", call_id: callId(), name: name.name, arguments: JSON.stringify(args) };
      if (name.namespace) item.namespace = name.namespace;
      addItem(item);
    },
    complete() {
      event("response.completed", {
        response: {
          ...base,
          status: "completed",
          output,
          usage: {
            input_tokens: 120,
            input_tokens_details: { cached_tokens: 0 },
            output_tokens: 12,
            output_tokens_details: { reasoning_tokens: 0 },
            total_tokens: 132,
          },
        },
      });
      res.end();
    },
  };
}

/** The name Codex gave an MCP tool in the request's tool list (it may be
 * prefixed with the server name, or grouped in a namespace). */
function findToolName(tools, tool) {
  for (const t of tools) {
    if (t.type === "function" && (t.name === tool || t.name?.endsWith(`__${tool}`))) return { name: t.name };
    if (t.type === "namespace" && Array.isArray(t.tools)) {
      for (const inner of t.tools) {
        if (inner.name === tool || inner.name?.endsWith(`__${tool}`)) return { name: inner.name, namespace: t.name };
      }
    }
  }
  return { name: tool };
}

function runMcpServer() {
  const rl = createInterface({ input: process.stdin });
  const send = (msg) => process.stdout.write(JSON.stringify(msg) + "\n");
  const tokenSeen = process.env.INFINABOX_BRIDGE_TOKEN ? "yes" : "no";
  rl.on("line", (line) => {
    let req;
    try {
      req = JSON.parse(line);
    } catch {
      return;
    }
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
          result: {
            tools: [
              {
                name: "echo",
                description: "Echoes the given text back.",
                inputSchema: { type: "object", properties: { text: { type: "string" } }, required: ["text"] },
              },
              {
                name: "propose_plan",
                description: "Shows the person a plan to approve before any change is made.",
                inputSchema: {
                  type: "object",
                  properties: { title: { type: "string" }, steps: { type: "array", items: { type: "string" } } },
                  required: ["title", "steps"],
                },
              },
            ],
          },
        });
        break;
      case "tools/call": {
        const args = req.params?.arguments ?? {};
        const text =
          req.params?.name === "propose_plan"
            ? "The plan is shown to the person with Approve and Change buttons. End your turn now."
            : `echo: ${args.text ?? ""} (bridge token received: ${tokenSeen})`;
        send({ jsonrpc: "2.0", id: req.id, result: { content: [{ type: "text", text }] } });
        break;
      }
      default:
        send({ jsonrpc: "2.0", id: req.id, error: { code: -32601, message: "Method not found" } });
    }
  });
}
