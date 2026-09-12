import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Send } from "lucide-react";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

interface Message {
  id: number;
  from: "user" | "agent";
  text: string;
}

type GraphState =
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "ready"; projectPath: string };

interface AgentPanelProps {
  projectPath: string | null;
}

/** Best-effort detection of "this folder isn't a Git repository" so we can
 * show an honest, specific message instead of the raw backend error text —
 * refresh_project_graph fails this way for any real, non-Git folder the
 * user might now pick via Open Project. The backend wraps git2::Repository
 * ::open's failure with anyhow context "failed to open Git repository at
 * '<path>'" (see crates/core/src/git_indexer.rs::walk_commits); note that
 * anyhow's `.to_string()` — used when converting to the String error type
 * these Tauri commands return — only surfaces that top-level context, not
 * the underlying libgit2 message, so we match on the context text itself.
 * Falls back to showing the raw message for every other failure mode. */
function isMissingGitRepoError(message: string): boolean {
  return message.toLowerCase().includes("failed to open git repository");
}

export function AgentPanel({ projectPath }: AgentPanelProps) {
  const [graph, setGraph] = useState<GraphState>({ status: "loading" });
  const [messages, setMessages] = useState<Message[]>([]);
  const [draft, setDraft] = useState("");
  const [asking, setAsking] = useState(false);
  const nextId = useRef(0);

  useEffect(() => {
    if (!projectPath) {
      return;
    }

    let cancelled = false;
    setGraph({ status: "loading" });
    setMessages([]);

    async function prepare() {
      try {
        // Builds the graph if it doesn't exist yet, or is a no-op if it's
        // already current — same idempotent logic verified in Phase 0/1.
        await invoke("refresh_project_graph", { projectPath });
        if (!cancelled) {
          setGraph({ status: "ready", projectPath: projectPath! });
        }
      } catch (err) {
        if (!cancelled) {
          const raw = err instanceof Error ? err.message : String(err);
          const message = isMissingGitRepoError(raw)
            ? "This folder isn't a Git repository yet — the project graph needs Git history to build from."
            : raw;
          setGraph({ status: "error", message });
        }
      }
    }

    void prepare();
    return () => {
      cancelled = true;
    };
  }, [projectPath]);

  function addMessage(from: Message["from"], text: string) {
    setMessages((prev) => [...prev, { id: nextId.current++, from, text }]);
  }

  async function submit() {
    const text = draft.trim();
    if (!text || graph.status !== "ready" || asking) return;

    addMessage("user", text);
    setDraft("");
    setAsking(true);

    try {
      const answer = await invoke<string>("ask_question", {
        projectPath: graph.projectPath,
        question: text,
      });
      addMessage("agent", answer);
    } catch (err) {
      addMessage(
        "agent",
        `Couldn't answer that: ${err instanceof Error ? err.message : String(err)}`,
      );
    } finally {
      setAsking(false);
    }
  }

  function onKeyDown(e: KeyboardEvent<HTMLInputElement>) {
    if (e.key === "Enter") {
      void submit();
    }
  }

  return (
    <div className="flex h-full w-[280px] shrink-0 flex-col border border-border bg-card">
      <div className="flex h-8 shrink-0 items-center border-b border-border px-3">
        <span className="text-xs font-medium tracking-wide text-muted-foreground uppercase">
          Agent
        </span>
      </div>
      <p className="border-b border-border px-3 py-2 text-xs text-muted-foreground">
        Answers are grounded in the local project graph — a real commit
        history, cited by sha. Only three specialist agents (Orchestrator,
        Engineering, QA) exist so far, and there's no real LLM behind this
        yet: it refuses rather than guesses when nothing in the graph
        matches your question.
      </p>
      <ScrollArea className="min-h-0 flex-1">
        <div className="flex flex-col gap-1.5 p-3">
          {graph.status === "loading" && (
            <p className="text-sm text-muted-foreground">
              Building the project graph…
            </p>
          )}
          {graph.status === "error" && (
            <p className="text-sm text-destructive">{graph.message}</p>
          )}
          {graph.status === "ready" && messages.length === 0 && (
            <p className="text-sm text-muted-foreground">
              Try: "what commit last touched ElevatorLogic.gd, and why"
            </p>
          )}
          {messages.map((m) => (
            <div
              key={m.id}
              className={cn(
                "max-w-[90%] border border-border px-2.5 py-1.5 text-sm whitespace-pre-wrap",
                m.from === "user"
                  ? "self-end bg-accent"
                  : "self-start bg-background",
              )}
            >
              {m.text}
            </div>
          ))}
          {asking && (
            <p className="self-start text-sm text-muted-foreground">
              thinking…
            </p>
          )}
        </div>
      </ScrollArea>
      <div className="flex shrink-0 items-center gap-2 border-t border-border p-2">
        <Input
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={onKeyDown}
          disabled={graph.status !== "ready"}
          placeholder="Message the agent…"
        />
        <Button
          size="icon"
          onClick={() => void submit()}
          disabled={graph.status !== "ready" || asking}
          aria-label="Send message"
        >
          <Send />
        </Button>
      </div>
    </div>
  );
}
