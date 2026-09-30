import { memo, useEffect, useMemo, useState, type ReactNode } from "react";
import { motion } from "motion/react";
import {
  AlertCircle,
  Check,
  ChevronRight,
  CircleDashed,
  FileText,
  Hammer,
  ListChecks,
  Loader2,
  PencilLine,
  Sparkles,
  Square,
  Wrench,
  X,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { attachmentLabel, isImagePath, splitAttachments } from "@/lib/attachments";
import { fadeTransition, springTransition } from "@/lib/motion";
import { assetsReadBase64 } from "@/lib/studio-api";
import type { AgentErrorKind } from "@/lib/studio-types";
import { aiDisplayName } from "./ai-name";
import { ConnectHint } from "./ConnectHint";
import {
  changeExplanations,
  describeWork,
  planStatuses,
  type ChatItem,
  type PlanStatus,
  type WorkStep,
} from "./chat-reducer";
import { MessageText } from "./MessageText";

// The conversation itself: one component per view-model item kind from
// chat-reducer.ts. Purely presentational — everything shown here came from
// the thread's saved records or the live agent event stream.

interface ChatTranscriptProps {
  /** For showing the pictures attached to messages. */
  projectPath?: string;
  items: ChatItem[];
  turnInProgress: boolean;
  /** The thread's provider (`ThreadSummary.provider`), for error copy. */
  provider: string | null;
  /** Approves the waiting plan (sends the approval message). */
  onApprovePlan: () => void;
  /** Asks for a different plan: focuses the chat box with a hint. */
  onChangePlan: () => void;
}

export function ChatTranscript({ projectPath, items, turnInProgress, provider, onApprovePlan, onChangePlan }: ChatTranscriptProps) {
  const last = items[items.length - 1];
  // While a turn runs but nothing new has streamed in since the user's
  // message (or the last reply), show that the AI is on it rather than
  // leaving a silent gap. A running work row already shows its own spinner.
  const showThinking = turnInProgress && last?.kind !== "work";

  // Derived from the whole list (not stored per item) so they stay right as
  // the conversation moves on: a plan stops waiting once anything is sent
  // after it, and a reply becomes "What changed" once its turn's changed
  // files are reported.
  const plans = useMemo(() => planStatuses(items), [items]);
  const explained = useMemo(() => changeExplanations(items), [items]);

  return (
    <div className="flex flex-col gap-3">
      {items.map((item) => (
        <motion.div
          key={item.key}
          data-testid="chat-item"
          data-kind={item.kind}
          initial={{ opacity: 0, y: 6 }}
          animate={{ opacity: 1, y: 0 }}
          transition={fadeTransition}
        >
          <ChatItemView
            item={item}
            projectPath={projectPath}
            planStatus={plans.get(item.key) ?? null}
            // The plan's buttons wait for the turn to end: the backend
            // refuses a new message until then.
            planActionable={!turnInProgress}
            changedFiles={explained.get(item.key) ?? null}
            provider={provider}
            onApprovePlan={onApprovePlan}
            onChangePlan={onChangePlan}
          />
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

interface ChatItemViewProps {
  item: ChatItem;
  projectPath?: string;
  planStatus: PlanStatus | null;
  planActionable: boolean;
  changedFiles: string[] | null;
  provider: string | null;
  onApprovePlan: () => void;
  onChangePlan: () => void;
}

// Memoised: the reducer returns the same object for every item a new event
// didn't touch, so while a turn streams only the changed row re-renders —
// not every earlier reply's markdown on each event.
const ChatItemView = memo(function ChatItemView({
  item,
  projectPath,
  planStatus,
  planActionable,
  changedFiles,
  provider,
  onApprovePlan,
  onChangePlan,
}: ChatItemViewProps) {
  switch (item.kind) {
    case "user":
      return <UserMessage text={item.text} origin={item.origin} projectPath={projectPath} />;
    case "assistant":
      return changedFiles ? (
        <WhatChangedCard text={item.text} files={changedFiles} />
      ) : (
        <div className="max-w-[95%] px-1 text-foreground">
          <MessageText text={item.text} />
        </div>
      );
    case "plan":
      return (
        <PlanCard
          title={item.title}
          steps={item.steps}
          status={planStatus ?? "waiting"}
          actionable={planActionable}
          onApprove={onApprovePlan}
          onChange={onChangePlan}
        />
      );
    case "work":
      return <WorkRow item={item} />;
    case "stopped":
      return (
        <div className="flex items-center gap-2 px-1 text-xs text-muted-foreground" data-testid="stopped-note">
          <Square className="size-3 fill-current" />
          <span>Stopped</span>
          <span className="h-px flex-1 bg-border" />
        </div>
      );
    case "error":
      return <ErrorCard kind={item.errorKind} message={item.message} provider={provider} />;
  }
});

// --- Messages sent to the AI ---

function UserMessage({
  text,
  origin,
  projectPath,
}: {
  text: string;
  origin: Extract<ChatItem, { kind: "user" }>["origin"];
  projectPath?: string;
}) {
  const sent = splitAttachments(text);
  switch (origin) {
    case "plan_approval":
      // The approval text itself is boilerplate the app sent for them; what
      // matters is that they said yes.
      return (
        <div className="flex justify-end">
          <span className="inline-flex items-center gap-1.5 rounded-full border border-foreground/10 bg-foreground/[0.06] px-2.5 py-1 text-xs text-foreground/90">
            <Check className="size-3.5" />
            You approved the plan
          </span>
        </div>
      );
    case "auto_fix":
      return (
        <SystemNote
          origin={origin}
          icon={<Wrench className="size-3.5" />}
          title="Something broke — fixing it"
          detailsLabel="Show the errors"
        >
          {text}
        </SystemNote>
      );
    case "first_build":
      return (
        <SystemNote
          origin={origin}
          icon={<Hammer className="size-3.5" />}
          title="Building your game for the first time"
          detailsLabel="Show what I asked the AI"
        >
          {text}
        </SystemNote>
      );
    default:
      // The user's bubble is a tint of the foreground colour rather than
      // `bg-secondary`: the transcript sits on `bg-card`, which the dark
      // theme gives the same value as `--secondary`, so that bubble was
      // invisible. A foreground tint stays a step lighter (dark) or darker
      // (light) than whatever surface it's on, in either theme.
      return (
        <div className="flex justify-end">
          <div className="max-w-[85%] rounded-2xl rounded-br-md border border-foreground/10 bg-foreground/[0.08] px-3 py-2 text-sm leading-relaxed break-words whitespace-pre-wrap text-foreground">
            {sent.text}
            {sent.paths.length > 0 && (
              <div className="mt-2 flex flex-wrap gap-2" data-testid="message-attachments">
                {sent.paths.map((path) => (
                  <AttachedFileView key={path} path={path} projectPath={projectPath} />
                ))}
              </div>
            )}
          </div>
        </div>
      );
  }
}

/** One file attached to a sent message: a picture shows as a thumbnail,
 * anything else as its name. */
function AttachedFileView({ path, projectPath }: { path: string; projectPath?: string }) {
  const [src, setSrc] = useState<string | null>(null);
  const image = isImagePath(path);
  useEffect(() => {
    if (!image || !projectPath) return;
    let cancelled = false;
    assetsReadBase64(projectPath, path).then(
      (file) => {
        if (!cancelled && !file.truncated) setSrc(`data:${file.mime};base64,${file.base64}`);
      },
      () => {},
    );
    return () => {
      cancelled = true;
    };
  }, [image, projectPath, path]);
  if (image && src) {
    return <img src={src} alt={attachmentLabel(path)} title={attachmentLabel(path)} className="max-h-40 max-w-full rounded-lg border border-foreground/10" />;
  }
  return (
    <span className="inline-flex items-center gap-1.5 rounded-lg border border-foreground/10 bg-background/40 px-2 py-1 text-xs">
      <FileText className="size-3.5" />
      {attachmentLabel(path)}
    </span>
  );
}

/** A message InfinaBox sent on the person's behalf: a neutral one-line note,
 * with the real message it sent one click away. */
function SystemNote({
  origin,
  icon,
  title,
  detailsLabel,
  children,
}: {
  /** Which kind of message it stands for (read by the E2E tests). */
  origin: "auto_fix" | "first_build";
  icon: ReactNode;
  title: string;
  detailsLabel: string;
  children: string;
}) {
  const [open, setOpen] = useState(false);
  return (
    <Collapsible
      open={open}
      onOpenChange={setOpen}
      data-testid="system-note"
      data-origin={origin}
      className="rounded-lg border border-dashed border-border bg-muted/30"
    >
      <div className="flex items-center gap-2 px-2.5 py-1.5 text-xs">
        <span className="flex shrink-0 text-muted-foreground">{icon}</span>
        <span className="min-w-0 flex-1 truncate font-medium text-foreground/90">{title}</span>
        <CollapsibleTrigger className="flex shrink-0 items-center gap-1 rounded text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/50">
          {open ? "Hide" : detailsLabel}
          <motion.span animate={{ rotate: open ? 90 : 0 }} transition={springTransition} className="flex">
            <ChevronRight className="size-3.5" />
          </motion.span>
        </CollapsibleTrigger>
      </div>
      <CollapsibleContent>
        <div className="max-h-60 overflow-y-auto border-t border-dashed border-border px-2.5 py-2 font-mono text-[11px] leading-relaxed break-words whitespace-pre-wrap text-muted-foreground">
          {children}
        </div>
      </CollapsibleContent>
    </Collapsible>
  );
}

// --- Plans ---

const PLAN_STATUS_LABEL: Record<Exclude<PlanStatus, "waiting">, string> = {
  approved: "Approved",
  changed: "Changed",
  replaced: "Replaced by a newer plan",
};

function PlanCard({
  title,
  steps,
  status,
  actionable,
  onApprove,
  onChange,
}: {
  title: string;
  steps: string[];
  status: PlanStatus;
  actionable: boolean;
  onApprove: () => void;
  onChange: () => void;
}) {
  const waiting = status === "waiting";
  return (
    <div
      data-testid="plan-card"
      data-status={status}
      className={
        waiting
          ? "overflow-hidden rounded-xl border border-foreground/20 bg-background shadow-sm"
          : "overflow-hidden rounded-xl border border-border bg-background/40"
      }
    >
      <div className="flex items-start gap-2 px-3 pt-3">
        <ListChecks className="mt-0.5 size-4 shrink-0 text-muted-foreground" />
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <span className="text-[11px] font-medium tracking-wide text-muted-foreground uppercase">Plan</span>
          <span data-testid="plan-title" className="text-sm font-medium break-words text-foreground">
            {title}
          </span>
        </div>
        {!waiting && (
          <span className="flex shrink-0 items-center gap-1 rounded-full border border-border px-2 py-0.5 text-[11px] text-muted-foreground">
            {status === "approved" && <Check className="size-3" />}
            {status === "changed" && <PencilLine className="size-3" />}
            {PLAN_STATUS_LABEL[status]}
          </span>
        )}
      </div>
      <ol className="flex flex-col gap-1.5 px-3 pt-2.5 pb-3">
        {steps.map((step, i) => (
          <li key={i} className="flex items-start gap-2 text-sm leading-relaxed">
            <span
              className={
                "mt-0.5 flex size-5 shrink-0 items-center justify-center rounded-full border text-[11px] tabular-nums " +
                (waiting ? "border-foreground/20 text-foreground/80" : "border-border text-muted-foreground")
              }
            >
              {i + 1}
            </span>
            <span className={"min-w-0 break-words " + (waiting ? "text-foreground" : "text-muted-foreground")}>
              {step}
            </span>
          </li>
        ))}
      </ol>
      {waiting && (
        <div className="flex flex-wrap items-center gap-2 border-t border-border bg-muted/30 px-3 py-2">
          <span className="mr-auto flex items-center gap-1.5 text-xs text-muted-foreground">
            {actionable ? (
              <span className="size-1.5 rounded-full bg-foreground/60" aria-hidden />
            ) : (
              <Loader2 className="size-3 animate-spin" />
            )}
            Waiting for your OK
          </span>
          <Button
            type="button"
            size="sm"
            variant="outline"
            data-testid="plan-change"
            onClick={onChange}
            disabled={!actionable}
          >
            <PencilLine />
            Change something
          </Button>
          <Button type="button" size="sm" data-testid="plan-approve" onClick={onApprove} disabled={!actionable}>
            <Check />
            Approve
          </Button>
        </div>
      )}
    </div>
  );
}

// --- "What changed": the explanation that ends a turn which edited files ---

function WhatChangedCard({ text, files }: { text: string; files: string[] }) {
  const [open, setOpen] = useState(false);
  return (
    <div data-testid="what-changed" className="overflow-hidden rounded-xl border border-border bg-background/60">
      <div className="flex items-center gap-1.5 px-3 pt-2.5 text-[11px] font-medium tracking-wide text-muted-foreground uppercase">
        <Sparkles className="size-3.5" />
        What changed
      </div>
      <div className="px-3 pt-1 pb-2.5 text-foreground">
        <MessageText text={text} />
      </div>
      <Collapsible open={open} onOpenChange={setOpen} className="border-t border-border">
        <CollapsibleTrigger className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/50">
          <FileText className="size-3.5 shrink-0" />
          <span className="flex-1">
            {files.length} {files.length === 1 ? "file" : "files"} changed
          </span>
          <motion.span animate={{ rotate: open ? 90 : 0 }} transition={springTransition} className="flex">
            <ChevronRight className="size-3.5" />
          </motion.span>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <FileList files={files} className="px-3 pb-2" />
        </CollapsibleContent>
      </Collapsible>
    </div>
  );
}

function FileList({ files, className }: { files: string[]; className?: string }) {
  return (
    <div className={"flex flex-col gap-1 " + (className ?? "")}>
      {files.map((path) => (
        <span key={path} className="flex items-center gap-1.5 font-mono text-[11px] text-foreground/90">
          <FileText className="size-3 shrink-0 text-muted-foreground" />
          <span className="truncate" title={path}>
            {path}
          </span>
        </span>
      ))}
    </div>
  );
}

// --- Tool use ---

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
              <FileList files={item.filesChanged} />
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

// --- Errors ---

// Plain-language headings for each error kind, naming the thread's own AI
// (Claude Code or Codex) when known; the real message from the runtime (or
// the failed command) is always shown underneath, unedited. (`cancelled`
// never reaches here — Stop is the neutral "Stopped" note — but the map
// covers it so a stray one still reads sensibly.)
function errorTitle(kind: AgentErrorKind, provider: string | null): string {
  const name = aiDisplayName(provider);
  switch (kind) {
    case "not_installed":
      return `${name} isn't installed`;
    case "not_authenticated":
      return `${name} needs you to sign in`;
    case "rate_limited":
      return "You've reached your plan's limit for now";
    case "process_failed":
      return "The AI stopped unexpectedly";
    case "other":
      return "Something went wrong";
    case "cancelled":
      return "Stopped";
  }
}

function ErrorCard({ kind, message, provider }: { kind: AgentErrorKind; message: string; provider: string | null }) {
  const needsSetup = kind === "not_installed" || kind === "not_authenticated";
  return (
    <div className="flex items-start gap-2 rounded-lg border border-destructive/30 bg-destructive/5 px-3 py-2">
      <AlertCircle className="mt-0.5 size-4 shrink-0 text-destructive" />
      <div className="flex min-w-0 flex-col gap-0.5">
        <span className="text-sm font-medium text-foreground">{errorTitle(kind, provider)}</span>
        <span className="font-mono text-xs break-words whitespace-pre-wrap text-muted-foreground">{message}</span>
        {needsSetup && (
          <span className="pt-1 text-xs">
            <ConnectHint what={kind === "not_installed" ? "set up" : "sign in to"} provider={provider} />
          </span>
        )}
      </div>
    </div>
  );
}
