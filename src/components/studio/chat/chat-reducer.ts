import type { AgentErrorKind, AgentEvent, ChatRecord } from "@/lib/studio-types";

// The chat's view model, folded from two sources that describe the same
// thing: `ChatRecord`s loaded from the thread's saved `.jsonl` file, and
// live `AgentEvent`s streamed while a turn runs. Both go through the same
// `applyEvent`, so a thread looks identical whether you watched it happen
// or reopened it later.
//
// Deliberately plain TypeScript with no React in it — every function here is
// pure (state in, new state out), so a test runner can exercise it directly
// once the frontend has one.

/** One tool the agent used, paired with its result once that arrives. */
export interface WorkStep {
  id: string;
  /** The runtime's raw tool name (`Edit`, `Bash`, `mcp__infinabox__run_game`, ...). */
  name: string;
  /** The runtime's own one-line description of the call. */
  summary: string;
  /** `running` until a result arrives; `unfinished` if the turn ended first. */
  status: "running" | "ok" | "failed" | "unfinished";
  resultSummary: string | null;
}

export type ChatItem =
  | { kind: "user"; key: string; text: string }
  | { kind: "assistant"; key: string; text: string }
  /** A run of consecutive tool uses, shown collapsed as one "working" row. */
  | { kind: "work"; key: string; steps: WorkStep[]; filesChanged: string[] }
  | { kind: "error"; key: string; errorKind: AgentErrorKind; message: string };

export interface TurnError {
  kind: AgentErrorKind;
  message: string;
}

export interface ChatView {
  items: ChatItem[];
  /** True from a user message until that turn's `turn_completed` (or the
   * backend's `agent-turn-finished`, via `endTurn`). */
  turnInProgress: boolean;
  /** The most recent error reported during the current/latest turn — what
   * the status banner reacts to (sign-in needed, rate limited). Cleared when
   * the user sends the next message. */
  lastTurnError: TurnError | null;
  /** Model name the runtime reported at session start, if it reported one. */
  model: string | null;
  /** Monotonic counter for React keys — deterministic, so re-folding the
   * same records always produces the same keys (no remount on reload). */
  nextKey: number;
}

export const emptyChatView: ChatView = {
  items: [],
  turnInProgress: false,
  lastTurnError: null,
  model: null,
  nextKey: 0,
};

function withItem(view: ChatView, make: (key: string) => ChatItem): ChatView {
  return {
    ...view,
    items: [...view.items, make(`i${view.nextKey}`)],
    nextKey: view.nextKey + 1,
  };
}

function replaceLast(view: ChatView, item: ChatItem): ChatView {
  return { ...view, items: [...view.items.slice(0, -1), item] };
}

/** A message the user sent (typed, or injected by "Ask AI to fix"). */
export function applyUserMessage(view: ChatView, text: string): ChatView {
  const next = withItem(view, (key) => ({ kind: "user", key, text }));
  return { ...next, turnInProgress: true, lastTurnError: null };
}

export function applyEvent(view: ChatView, event: AgentEvent): ChatView {
  switch (event.type) {
    case "session_started":
      return { ...view, model: event.model ?? view.model };

    case "assistant_text": {
      if (!event.text.trim()) return view;
      // The CLI emits one text block per assistant message; back-to-back
      // blocks with nothing in between read as one reply, not several.
      const last = view.items[view.items.length - 1];
      if (last?.kind === "assistant") {
        return replaceLast(view, { ...last, text: `${last.text}\n\n${event.text}` });
      }
      return withItem(view, (key) => ({ kind: "assistant", key, text: event.text }));
    }

    case "tool_use": {
      const step: WorkStep = {
        id: event.id,
        name: event.name,
        summary: event.summary,
        status: "running",
        resultSummary: null,
      };
      const last = view.items[view.items.length - 1];
      if (last?.kind === "work") {
        return replaceLast(view, { ...last, steps: [...last.steps, step] });
      }
      return withItem(view, (key) => ({ kind: "work", key, steps: [step], filesChanged: [] }));
    }

    case "tool_result": {
      // Results can arrive after later text in principle, so search back
      // through every work group rather than assuming the last one.
      const items = view.items.map((item) => {
        if (item.kind !== "work" || !item.steps.some((s) => s.id === event.id)) return item;
        return {
          ...item,
          steps: item.steps.map((s) =>
            s.id === event.id
              ? { ...s, status: event.ok ? ("ok" as const) : ("failed" as const), resultSummary: event.summary }
              : s,
          ),
        };
      });
      return { ...view, items };
    }

    case "files_changed": {
      if (event.paths.length === 0) return view;
      // Attach to the most recent work group of this turn; a files_changed
      // with no tool use before it still gets its own row rather than being
      // dropped, since the change on disk is real either way.
      const index = findLastIndex(view.items, (i) => i.kind === "work" || i.kind === "user");
      const target = index >= 0 ? view.items[index] : undefined;
      if (target?.kind === "work") {
        const merged = Array.from(new Set([...target.filesChanged, ...event.paths]));
        const items = [...view.items];
        items[index] = { ...target, filesChanged: merged };
        return { ...view, items };
      }
      return withItem(view, (key) => ({ kind: "work", key, steps: [], filesChanged: [...event.paths] }));
    }

    case "turn_completed": {
      let next = endTurn(view);
      // A turn that ended in error without ever saying why still gets a
      // card — the failure is real even though its reason wasn't reported,
      // and we say exactly that instead of inventing one.
      if (event.is_error && !turnHasError(view)) {
        next = withItem(next, (key) => ({
          kind: "error",
          key,
          errorKind: "other",
          message: "The AI stopped with an error but didn't report what went wrong.",
        }));
      }
      return next;
    }

    case "error":
      return {
        ...withItem(view, (key) => ({
          kind: "error",
          key,
          errorKind: event.kind,
          message: event.message,
        })),
        lastTurnError: { kind: event.kind, message: event.message },
      };
  }
}

/** Marks the current turn over: any tool still "running" never got a
 * result, so it's shown as unfinished rather than spinning forever. */
export function endTurn(view: ChatView): ChatView {
  if (!view.turnInProgress && !view.items.some(hasRunningStep)) return view;
  const items = view.items.map((item) =>
    hasRunningStep(item) && item.kind === "work"
      ? {
          ...item,
          steps: item.steps.map((s) => (s.status === "running" ? { ...s, status: "unfinished" as const } : s)),
        }
      : item,
  );
  return { ...view, items, turnInProgress: false };
}

/** A failure outside the agent's own event stream — e.g. the `agent_send`
 * command itself rejecting — shown the same way as an agent error. */
export function applyLocalError(view: ChatView, message: string): ChatView {
  return endTurn(applyEvent(view, { type: "error", kind: "other", message }));
}

export function applyRecord(view: ChatView, record: ChatRecord): ChatView {
  return record.kind === "user" ? applyUserMessage(view, record.text) : applyEvent(view, record.event);
}

/** Folds a whole saved thread. A saved thread never has a turn running
 * from the file's point of view (the caller knows better, if one is). */
export function buildChatView(records: ChatRecord[]): ChatView {
  return endTurn(records.reduce(applyRecord, emptyChatView));
}

// --- Plain-language summaries for the collapsed "working" row ---

/** "Edited 2 files, ran the game" — built only from what actually happened
 * (the real tool uses and the real changed-file list), never estimated. */
export function describeWork(item: Extract<ChatItem, { kind: "work" }>): string {
  const phrases: string[] = [];
  if (item.filesChanged.length > 0) {
    const n = item.filesChanged.length;
    phrases.push(`changed ${n} ${n === 1 ? "file" : "files"}`);
  }
  const seen = new Set<string>();
  for (const step of item.steps) {
    const phrase = describeTool(step.name);
    // File edits are already counted above from the real changed-file list.
    if (phrase === null || seen.has(phrase)) continue;
    seen.add(phrase);
    phrases.push(phrase);
  }
  if (phrases.length === 0) return item.steps.length > 0 ? "Worked on your project" : "Changed files";
  const text = phrases.join(", ");
  return text.charAt(0).toUpperCase() + text.slice(1);
}

// Claude Code's built-in tool names, plus the InfinaBox MCP tools matched by
// what their names say rather than an exact list, since those names belong
// to the MCP server and may grow. Anything unrecognised falls back to its
// real name instead of a guess at what it did.
function describeTool(name: string): string | null {
  const mcp = /^mcp__(.+?)__(.+)$/.exec(name);
  const tool = (mcp ? mcp[2] : name).toLowerCase();
  if (mcp) {
    if (tool.includes("run_game") || tool === "run") return "ran the game";
    if (tool.includes("stop_game")) return "stopped the game";
    if (tool.includes("error")) return "checked the game for errors";
    if (tool.includes("output") || tool.includes("log")) return "read the game's output";
    if (tool.includes("snapshot")) return "looked at the history";
    return `used ${mcp[2].replace(/_/g, " ")}`;
  }
  switch (tool) {
    case "edit":
    case "multiedit":
    case "write":
    case "notebookedit":
      return null;
    case "read":
      return "read files";
    case "glob":
    case "grep":
    case "ls":
      return "searched the project";
    case "bash":
      return "ran commands";
    case "webfetch":
    case "websearch":
      return "looked things up online";
    case "todowrite":
      return "made a to-do list";
    case "task":
    case "agent":
      return "asked a helper";
    default:
      return `used ${name}`;
  }
}

function hasRunningStep(item: ChatItem): boolean {
  return item.kind === "work" && item.steps.some((s) => s.status === "running");
}

function turnHasError(view: ChatView): boolean {
  for (let i = view.items.length - 1; i >= 0; i--) {
    const item = view.items[i];
    if (item.kind === "error") return true;
    if (item.kind === "user") return false;
  }
  return false;
}

function findLastIndex<T>(items: T[], predicate: (item: T) => boolean): number {
  for (let i = items.length - 1; i >= 0; i--) if (predicate(items[i])) return i;
  return -1;
}
