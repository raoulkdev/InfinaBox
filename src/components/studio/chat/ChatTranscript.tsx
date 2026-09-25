import { useState } from "react";
import { motion } from "motion/react";
import {
  AlertCircle,
  Check,
  ChevronRight,
  CircleDashed,
  FileText,
  Loader2,
  Wrench,
  X,
} from "lucide-react";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { fadeTransition, springTransition } from "@/lib/motion";
import type { AgentErrorKind } from "@/lib/studio-types";
import { describeWork, type ChatItem, type WorkStep } from "./chat-reducer";
import { MessageText } from "./MessageText";

// The conversation itself: one component per view-model item kind from
// chat-reducer.ts. Purely presentational — everything shown here came from
// the thread's saved records or the live agent event stream.

export function ChatTranscript({ items, turnInProgress }: { items: ChatItem[]; turnInProgress: boolean }) {
  const last = items[items.length - 1];
  // While a turn runs but nothing new has streamed in since the user's
  // message (or the last reply), show that the AI is on it rather than
  // leaving a silent gap. A running work row already shows its own spinner.
  const showThinking = turnInProgress && last?.kind !== "work";

  return (
    <div className="flex flex-col gap-3">
      {items.map((item) => (
        <motion.div
          key={item.key}
          initial={{ opacity: 0, y: 6 }}
          animate={{ opacity: 1, y: 0 }}
          transition={fadeTransition}
        >
          <ChatItemView item={item} />
        </motion.div>
      ))}
      {showThinking && (
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          transition={fadeTransition}
          className="flex items-center gap-2 px-1 text-xs text-muted-foreground"
        >
          <Loader2 className="size-3.5 animate-spin" />
          Working…
        </motion.div>
      )}
    </div>
  );
}

function ChatItemView({ item }: { item: ChatItem }) {
  switch (item.kind) {
    case "user":
      return (
        <div className="flex justify-end">
          <div className="max-w-[85%] rounded-2xl rounded-br-md bg-secondary px-3 py-2 text-sm leading-relaxed break-words whitespace-pre-wrap text-secondary-foreground">
            {item.text}
          </div>
        </div>
      );
    case "assistant":
      return (
        <div className="max-w-[95%] px-1 text-foreground">
          <MessageText text={item.text} />
        </div>
      );
    case "work":
      return <WorkRow item={item} />;
    case "error":
      return <ErrorCard kind={item.errorKind} message={item.message} />;
  }
}

function WorkRow({ item }: { item: Extract<ChatItem, { kind: "work" }> }) {
  const [open, setOpen] = useState(false);
  const running = item.steps.some((s) => s.status === "running");
  const failed = item.steps.filter((s) => s.status === "failed").length;

  return (
    <Collapsible open={open} onOpenChange={setOpen} className="rounded-lg border border-border bg-card">
      <CollapsibleTrigger className="flex w-full items-center gap-2 rounded-lg px-2.5 py-1.5 text-left text-xs text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/50">
        {running ? (
          <Loader2 className="size-3.5 shrink-0 animate-spin" />
        ) : (
          <Wrench className="size-3.5 shrink-0" />
        )}
        <span className="min-w-0 flex-1 truncate">
          {describeWork(item)}
          {failed > 0 && (
            <span className="text-destructive">
              {" "}
              · {failed} {failed === 1 ? "step" : "steps"} failed
            </span>
          )}
        </span>
        <motion.span animate={{ rotate: open ? 90 : 0 }} transition={springTransition} className="flex">
          <ChevronRight className="size-3.5" />
        </motion.span>
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div className="flex flex-col gap-1.5 border-t border-border px-2.5 py-2 text-xs">
          {item.steps.map((step) => (
            <StepLine key={step.id} step={step} />
          ))}
          {item.filesChanged.length > 0 && (
            <div className="flex flex-col gap-1 pt-1">
              <span className="text-muted-foreground">Files changed</span>
              {item.filesChanged.map((path) => (
                <span key={path} className="flex items-center gap-1.5 font-mono text-[11px] text-foreground/90">
                  <FileText className="size-3 shrink-0 text-muted-foreground" />
                  <span className="truncate" title={path}>
                    {path}
                  </span>
                </span>
              ))}
            </div>
          )}
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}

function StepLine({ step }: { step: WorkStep }) {
  const icon = {
    running: <Loader2 className="size-3 animate-spin text-muted-foreground" />,
    ok: <Check className="size-3 text-muted-foreground" />,
    failed: <X className="size-3 text-destructive" />,
    unfinished: <CircleDashed className="size-3 text-muted-foreground" />,
  }[step.status];

  return (
    <div className="flex items-start gap-1.5">
      <span className="mt-0.5 flex shrink-0">{icon}</span>
      <div className="flex min-w-0 flex-col">
        <span className="break-words text-foreground/90">{step.summary || step.name}</span>
        {step.status === "failed" && step.resultSummary && (
          <span className="break-words text-destructive/90">{step.resultSummary}</span>
        )}
        {step.status === "unfinished" && <span className="text-muted-foreground">Didn't finish</span>}
      </div>
    </div>
  );
}

// Plain-language headings for each error kind; the real message from the
// runtime (or the failed command) is always shown underneath, unedited.
const ERROR_TITLES: Record<AgentErrorKind, string> = {
  not_installed: "Claude Code isn't installed",
  not_authenticated: "Claude Code needs you to sign in",
  rate_limited: "You've reached your plan's limit for now",
  process_failed: "The AI stopped unexpectedly",
  other: "Something went wrong",
};

function ErrorCard({ kind, message }: { kind: AgentErrorKind; message: string }) {
  return (
    <div className="flex items-start gap-2 rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2">
      <AlertCircle className="mt-0.5 size-4 shrink-0 text-destructive" />
      <div className="flex min-w-0 flex-col gap-0.5">
        <span className="text-sm font-medium text-foreground">{ERROR_TITLES[kind]}</span>
        <span className="font-mono text-xs break-words whitespace-pre-wrap text-muted-foreground">{message}</span>
      </div>
    </div>
  );
}
