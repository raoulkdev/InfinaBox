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

export function AgentPanel() {
  const [graph, setGraph] = useState<GraphState>({ status: "loading" });
  const [messages, setMessages] = useState<Message[]>([]);
  const [draft, setDraft] = useState("");
  const [asking, setAsking] = useState(false);
  const nextId = useRef(0);

  useEffect(() => {
    let cancelled = false;

    async function prepare() {
      try {
        const projectPath = await invoke<string>("get_default_project_path");
        // Builds the graph if it doesn't exist yet, or is a no-op if it's
        // already current — same idempotent logic verified in Phase 0/1.
        await invoke("refresh_project_graph", { projectPath });
        if (!cancelled) {
          setGraph({ status: "ready", projectPath });
        }
      } catch (err) {
        if (!cancelled) {
          setGraph({
            status: "error",
            message: err instanceof Error ? err.message : String(err),
          });
        }
      }
    }

    void prepare();
    return () => {
      cancelled = true;
    };
  }, []);

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
