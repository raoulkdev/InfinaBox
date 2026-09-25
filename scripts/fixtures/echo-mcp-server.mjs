#!/usr/bin/env node
// A minimal, dependency-free MCP server over stdio with one tool, `echo`.
// Used only by record-claude-fixtures.mjs to capture how the real Claude
// Code CLI reports an MCP tool call in its stream (fixture scenario d).
import { createInterface } from "node:readline";

const rl = createInterface({ input: process.stdin });
const send = (msg) => process.stdout.write(JSON.stringify(msg) + "\n");

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
        result: {
          tools: [
            {
              name: "echo",
              description: "Echoes the given text back.",
              inputSchema: {
                type: "object",
                properties: { text: { type: "string" } },
                required: ["text"],
              },
            },
          ],
        },
      });
      break;
    case "tools/call":
      send({
        jsonrpc: "2.0",
        id: req.id,
        result: { content: [{ type: "text", text: `echo: ${req.params?.arguments?.text ?? ""}` }] },
      });
      break;
    default:
      send({ jsonrpc: "2.0", id: req.id, error: { code: -32601, message: "Method not found" } });
  }
});
