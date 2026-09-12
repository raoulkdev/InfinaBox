import { useState, type KeyboardEvent } from "react";
// TODO: wire to ask_question once implemented on the Rust side.
// import { invoke } from "@tauri-apps/api/core";
import { Send } from "lucide-react";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";

interface LocalMessage {
  id: number;
  text: string;
}

export function AgentPanel() {
  const [messages, setMessages] = useState<LocalMessage[]>([]);
  const [draft, setDraft] = useState("");

  function submit() {
    const text = draft.trim();
    if (!text) return;
    setMessages((prev) => [...prev, { id: prev.length, text }]);
    setDraft("");

    // TODO: once ask_question is implemented, call it here instead of only
    // echoing locally, e.g.:
    // const answer = await invoke<string>("ask_question", {
    //   projectPath,
    //   question: text,
    // });
  }

  function onKeyDown(e: KeyboardEvent<HTMLInputElement>) {
    if (e.key === "Enter") {
      submit();
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
        Real agent answers, grounded in the local project graph, are coming in
        a later milestone. For now, messages you send are only echoed here.
      </p>
      <ScrollArea className="min-h-0 flex-1">
        <div className="flex flex-col gap-1.5 p-3">
          {messages.length === 0 && (
            <p className="text-sm text-muted-foreground">No messages yet.</p>
          )}
          {messages.map((m) => (
            <div
              key={m.id}
              className="self-end border border-border bg-accent px-2.5 py-1.5 text-sm"
            >
              {m.text}
            </div>
          ))}
        </div>
      </ScrollArea>
      <div className="flex shrink-0 items-center gap-2 border-t border-border p-2">
        <Input
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={onKeyDown}
          placeholder="Message the agent…"
        />
        <Button size="icon" onClick={submit} aria-label="Send message">
          <Send />
        </Button>
      </div>
    </div>
  );
}
