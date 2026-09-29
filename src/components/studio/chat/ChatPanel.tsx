import { useCallback, useEffect, useRef, useState } from "react";
import { AlertCircle, Loader2, MessageSquare, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  agentCancel,
  agentSend,
  chatCreateThread,
  chatListThreads,
  chatLoadThread,
  onAgentEvent,
  onAgentTurnFinished,
  onAutofixState,
} from "@/lib/studio-api";
import type {
  AgentEvent,
  AutoFixStatePayload,
  MessageOrigin,
  Role,
  PendingTurn,
  ThreadSummary,
} from "@/lib/studio-types";
import { AgentStatusBanner } from "./AgentStatusBanner";
import { AutoFixBanner } from "./AutoFixBanner";
import { ChatComposer } from "./ChatComposer";
import { ChatTranscript } from "./ChatTranscript";
import { StudioSettingsPopover } from "./StudioSettingsPopover";
import { ThreadPicker } from "./ThreadPicker";
import {
  applyEvent,
  applyLocalError,
  applyUserMessage,
  buildChatView,
  emptyChatView,
  endTurn,
  unsavedLiveEvents,
  type ChatView,
} from "./chat-reducer";

// Studio's chat with the user's own AI (spec §7.1). Everything shown comes
// from the thread's saved records (`chat_load_thread`) or the live
// `agent-event` stream — see chat-reducer.ts for how both fold into the same
// view. Backend failures are shown with their real error text, never
// papered over.
//
// Turns don't only start here: the backend starts automatic error fixes
// itself, and a turn can already be running in a thread when it's opened.
// So "is a turn running" comes from the backend's own signals — any
// `agent-event` for a thread means it's running, its `agent-turn-finished`
// means it's done — and a turn this panel didn't start is picked up by
// re-reading the thread (which has its opening message) while buffering the
// events that stream in meanwhile (see `showThread`).

/** What a registered send actually did with the text: sent it to the AI
 * as a message, or (when the chat couldn't send right then — still
 * loading, failed to load, or the AI is mid-turn) put it in the chat box
 * as a draft for the user to send. Callers report this truthfully. */
export type ChatSendOutcome = "sent" | "drafted";

export interface ChatPanelProps {
  projectPath: string;
  /** Called once on mount with a function that sends `text` as a user
   * message in the active thread — how the Play panel's "Ask AI to fix"
   * reaches the chat without the two panels sharing state. It returns
   * what happened to the text. */
  onRegisterSend: (send: (text: string) => ChatSendOutcome) => void;
  /** Told whether any AI turn is running in this chat (any thread, until
   * its `agent-turn-finished`) — so History can hold off undo/go back
   * while the AI is still editing files. */
  onTurnRunningChange?: (running: boolean) => void;
  /** A turn to start as soon as this project's chat is open (the
   * onboarding's first build): its thread is selected, the message sent
   * once, then `onPendingTurnTaken` is called. */
  pendingTurn?: PendingTurn | null;
  onPendingTurnTaken?: () => void;
}

/** How long after a confirmed Stop the composer waits for the backend's
 * `agent-turn-finished` before re-enabling itself anyway. The backend holds
 * a stopped turn for a few seconds (the CLI's SIGTERM grace, then the
 * post-turn snapshot), so this is comfortably longer than that. */
const STOP_FALLBACK_MS = 15_000;

/** What a brand-new project's first conversation is called. */
const DEFAULT_THREAD_TITLE = "Main";

/** The message a plan's Approve button sends. The runtime is told by the
 * `plan_approval` origin, not these words; they're what the AI reads. */
const PLAN_APPROVAL_MESSAGE = "Approved — go ahead with the plan.";

/** The chat box's placeholder after a plan's "Change something". */
const CHANGE_PLAN_HINT = "What should be different?";

// How close to the bottom (px) still counts as "following along", so new
// output keeps scrolling into view — but not if the user scrolled up to
// reread something.
const STICK_THRESHOLD = 48;

type Problem = { message: string; retry: () => void };

function latestThread(threads: ThreadSummary[]): ThreadSummary {
  return threads.reduce((a, b) => (b.created_at > a.created_at ? b : a));
}

/** Project paths compared loosely: the backend may echo the path with a
 * trailing separator the frontend didn't use. */
function samePath(a: string, b: string): boolean {
  const trim = (p: string) => p.replace(/[\\/]+$/, "");
  return trim(a) === trim(b);
}

export function ChatPanel({
  projectPath,
  onRegisterSend,
  onTurnRunningChange,
  pendingTurn = null,
  onPendingTurnTaken,
}: ChatPanelProps) {
  const [threads, setThreads] = useState<ThreadSummary[]>([]);
  const [activeThreadId, setActiveThreadId] = useState<string | null>(null);
  const [view, setView] = useState<ChatView>(emptyChatView);
  const [loading, setLoading] = useState(true);
  const [problem, setProblem] = useState<Problem | null>(null);
  const [draft, setDraft] = useState("");
  const [composerHint, setComposerHint] = useState<string | null>(null);
  const [role, setRole] = useState<Role>("director");
  const roleRef = useRef<Role>("director");
  roleRef.current = role;
  const [stopping, setStopping] = useState(false);
  // Threads with a turn in flight, including ones in the background after
  // switching away. Kept separately from `view.turnInProgress` because the
  // backend's turn only really ends at `agent-turn-finished` (after its
  // post-turn snapshot), a moment after the agent's own `turn_completed`.
  const [running, setRunning] = useState<ReadonlySet<string>>(new Set());
  const [initAttempt, setInitAttempt] = useState(0);
  // Which project the thread list was last opened for — the pending turn
  // waits for this, so it never races the "open the latest thread" step.
  const [initDoneFor, setInitDoneFor] = useState<string | null>(null);
  // The auto-fix loop's latest state for this project (`autofix-state`).
  const [autofix, setAutofix] = useState<AutoFixStatePayload | null>(null);

  // Mirrors of the latest state for the event subscriptions and the
  // registered `send`, which are created once and must not go stale.
  const projectRef = useRef(projectPath);
  const threadsRef = useRef<ThreadSummary[]>(threads);
  const activeRef = useRef<string | null>(null);
  const loadingRef = useRef(true);
  const runningRef = useRef<ReadonlySet<string>>(running);
  const busyRef = useRef(false);
  const loadSeq = useRef(0);
  // The in-flight "find or create the first thread" request, reused if this
  // effect runs twice for the same project (StrictMode's mount → cleanup →
  // mount), so an empty project never ends up with two "Main" threads.
  const initRequest = useRef<{ key: string; promise: Promise<ThreadSummary[]> } | null>(null);
  // Live events for the thread being (re)read, collected until the read
  // lands and then merged in — so nothing streamed meanwhile is lost.
  const liveBuffer = useRef<{ threadId: string; events: AgentEvent[] } | null>(null);
  // Threads whose current turn the view is known to show from its start:
  // ones this panel sent, or re-read after the turn began. An event for any
  // other running thread means a turn started elsewhere (an auto-fix).
  const syncedTurns = useRef<Set<string>>(new Set());
  // Threads of the previously open project that were still running when
  // the project changed. Their events (which carry no project path) are
  // ignored until their `agent-turn-finished`, so they can't mark this
  // project's chat busy or hold off its History.
  const foreignTurns = useRef<Set<string>>(new Set());
  // The project `running` describes.
  const runningFor = useRef<string | null>(null);
  // The pending turn already being handled (content key), so StrictMode's
  // double effects and re-renders send it exactly once.
  const pendingTaken = useRef<string | null>(null);

  // The fallback timer armed by a confirmed Stop (see `handleStop`).
  const stopFallback = useRef<number | null>(null);

  const scrollRef = useRef<HTMLDivElement | null>(null);
  const stickToBottom = useRef(true);
  const composerRef = useRef<HTMLTextAreaElement | null>(null);

  const busy = view.turnInProgress || (activeThreadId !== null && running.has(activeThreadId));

  // `activeRef`, `loadingRef` and `runningRef` are not synced here: each
  // is written together with its state wherever that changes. Copying the
  // state in after a render could put back a value from a render that
  // committed before the latest change (a turn's `agent-turn-finished`
  // landing between an event's render and its effects), and a thread
  // re-read then would be folded as still running — the chat stuck on
  // "Working…" after its turn had ended.
  useEffect(() => {
    projectRef.current = projectPath;
    threadsRef.current = threads;
    busyRef.current = busy;
  });

  function clearStopFallback() {
    if (stopFallback.current !== null) {
      window.clearTimeout(stopFallback.current);
      stopFallback.current = null;
    }
  }
  useEffect(() => clearStopFallback, []);

  function markRunning(threadId: string, isRunning: boolean) {
    // Computed from the ref (updated right away) rather than a state
    // updater, so two events in the same tick both see each other's change.
    const prev = runningRef.current;
    if (prev.has(threadId) === isRunning) return;
    const next = new Set(prev);
    if (isRunning) next.add(threadId);
    else next.delete(threadId);
    runningRef.current = next;
    setRunning(next);
  }

  /** Loads a thread's saved records and makes it the displayed one,
   * merging in any live events that arrive while it reads. `quiet` keeps
   * the current view on screen while reloading (used to resync during and
   * after a turn), and keeps it if the reload fails. Resolves to whether
   * the thread is now showing its saved records. */
  const showThread = useCallback(
    async (threadId: string, { quiet = false } = {}): Promise<boolean> => {
      const seq = ++loadSeq.current;
      activeRef.current = threadId;
      setActiveThreadId(threadId);
      liveBuffer.current = { threadId, events: [] };
      if (!quiet) {
        loadingRef.current = true;
        setLoading(true);
        setProblem(null);
        stickToBottom.current = true;
      }
      try {
        const loaded = await chatLoadThread(projectPath, threadId);
        if (seq !== loadSeq.current) return false;
        const buffered = liveBuffer.current?.threadId === threadId ? liveBuffer.current.events : [];
        liveBuffer.current = null;
        const isRunning = runningRef.current.has(threadId);
        let next = buildChatView(loaded.records, { running: isRunning });
        for (const event of unsavedLiveEvents(loaded.records, buffered)) next = applyEvent(next, event);
        // A thread is only known to be running from its events (or a send
        // from here), and the backend saves a turn's opening message before
        // any of them — so the read has the whole turn, with the buffered
        // events on top. No need to re-read it on its next event.
        if (isRunning) syncedTurns.current.add(threadId);
        // A quiet resync right after a live turn keeps that turn's error for
        // the status banner; the saved file alone never sets it (see
        // `buildChatView`), so reopening an old failed thread shows no banner.
        setView((v) => (quiet ? { ...next, lastTurnError: v.lastTurnError } : next));
        return true;
      } catch (err) {
        if (seq !== loadSeq.current) return false;
        liveBuffer.current = null;
        if (quiet) return false;
        setView(emptyChatView);
        setProblem({ message: String(err), retry: () => void showThread(threadId) });
        return false;
      } finally {
        if (seq === loadSeq.current && !quiet) {
          loadingRef.current = false;
          setLoading(false);
        }
      }
    },
    [projectPath],
  );

  // Open the project's latest conversation, creating the first one if the
  // project has none yet.
  useEffect(() => {
    let cancelled = false;
    const key = `${projectPath}\n${initAttempt}`;
    if (initRequest.current?.key !== key) {
      initRequest.current = {
        key,
        promise: chatListThreads(projectPath).then(async (list) =>
          list.length > 0 ? list : [await chatCreateThread(projectPath, DEFAULT_THREAD_TITLE)],
        ),
      };
    }

    loadSeq.current++;
    activeRef.current = null;
    loadingRef.current = true;
    liveBuffer.current = null;
    setActiveThreadId(null);
    setThreads([]);
    setView(emptyChatView);
    setLoading(true);
    setProblem(null);
    setDraft("");
    setComposerHint(null);
    setInitDoneFor(null);
    setAutofix(null);
    // Running turns belong to the project they started in and can't finish
    // in another one, so a different project starts idle; the old turns'
    // later events are ignored (see `foreignTurns`). A retry for the same
    // project keeps them.
    if (runningFor.current !== projectPath) {
      runningFor.current = projectPath;
      for (const threadId of runningRef.current) foreignTurns.current.add(threadId);
      const idle: ReadonlySet<string> = new Set();
      runningRef.current = idle;
      setRunning(idle);
      syncedTurns.current.clear();
    }

    initRequest.current.promise.then(
      (list) => {
        if (cancelled) return;
        // Back in a project whose turn was set aside on switching away: its
        // events count again (its next one marks the thread running).
        for (const thread of list) foreignTurns.current.delete(thread.id);
        setThreads(list);
        setInitDoneFor(projectPath);
        void showThread(latestThread(list).id);
      },
      (err) => {
        if (cancelled) return;
        loadingRef.current = false;
        setLoading(false);
        setProblem({ message: String(err), retry: () => setInitAttempt((n) => n + 1) });
      },
    );
    return () => {
      cancelled = true;
    };
  }, [projectPath, initAttempt, showThread]);

  // Live events. Ones for other threads only mark that thread busy; the
  // full record is on disk and loads when the user switches to it. Events
  // for the active thread that arrive mid-read go into the read's buffer.
  useEffect(() => {
    const stopEvents = onAgentEvent(({ threadId, event }) => {
      if (foreignTurns.current.has(threadId)) return;
      markRunning(threadId, true);
      if (threadId !== activeRef.current) return;
      if (!loadingRef.current && !syncedTurns.current.has(threadId)) {
        // A turn this view didn't see start (the backend began it): re-read
        // the thread for its opening message. This event is on disk
        // already; the merge drops it if the read has it.
        syncedTurns.current.add(threadId);
        void showThread(threadId, { quiet: true });
      }
      const buffer = liveBuffer.current;
      if (buffer?.threadId === threadId) buffer.events.push(event);
      if (!loadingRef.current) setView((v) => applyEvent(v, event));
    });
    const stopFinished = onAgentTurnFinished(({ threadId }) => {
      // The previous project's turn ending: nothing of it is on screen.
      if (foreignTurns.current.delete(threadId)) return;
      markRunning(threadId, false);
      syncedTurns.current.delete(threadId);
      if (threadId !== activeRef.current) return;
      clearStopFallback();
      setStopping(false);
      setView((v) => endTurn(v));
      void showThread(threadId, { quiet: true });
    });
    return () => {
      stopEvents();
      stopFinished();
    };
  }, [showThread]);

  /** Makes `threadId` the open thread, re-reading the thread list first if
   * it's one this panel hasn't seen (just created by the backend). */
  const openThread = useCallback(
    async (threadId: string, { refreshList = false } = {}): Promise<boolean> => {
      const project = projectPath;
      if (refreshList || !threadsRef.current.some((t) => t.id === threadId)) {
        try {
          const list = await chatListThreads(project);
          if (projectRef.current !== project) return false;
          threadsRef.current = list;
          setThreads(list);
        } catch {
          // The thread may still load on its own; if not, `showThread`
          // shows the real error.
        }
      }
      if (projectRef.current !== project) return false;
      clearStopFallback();
      setStopping(false);
      setComposerHint(null);
      return showThread(threadId);
    },
    [projectPath, showThread],
  );

  // The auto-fix loop, for this project only. While it fixes, show the
  // thread it's fixing in (its turn streams there); the backend starts that
  // turn, so it shows up through the events above like any other.
  useEffect(() => {
    return onAutofixState((payload) => {
      if (!samePath(payload.projectPath, projectRef.current)) return;
      if (payload.state === "idle") {
        // Idle only ends a fix in progress; "couldn't fix it" stays until
        // the person dismisses it or sends a message.
        setAutofix((prev) => (prev?.state === "gave_up" ? prev : null));
        return;
      }
      setAutofix(payload);
      if (payload.state === "fixing" && payload.threadId !== activeRef.current) {
        void openThread(payload.threadId);
      }
    });
  }, [openThread]);

  // Decides synchronously whether the text goes to the AI or stays as a
  // draft (so the caller can say which), then sends in the background.
  const send = useCallback(
    (text: string, origin: MessageOrigin = "user", roleOverride?: Role): ChatSendOutcome => {
      const message = text.trim();
      const threadId = activeRef.current;
      // A second Approve (a double click) while the first is being sent:
      // drop it rather than leave the approval text in the chat box.
      if (origin === "plan_approval" && busyRef.current) return "drafted";
      // Not ready to send (still loading, failed to load, or the AI is
      // mid-turn): keep the text as a draft rather than dropping it — this
      // is also where an "Ask AI to fix" lands while the AI is busy.
      if (!message || !threadId || loadingRef.current || busyRef.current) {
        if (message) setDraft((d) => (d.trim() ? `${d}\n\n${message}` : message));
        return "drafted";
      }
      busyRef.current = true;
      stickToBottom.current = true;
      // Invalidate any in-flight quiet reload (from the previous turn's
      // `agent-turn-finished`), which would otherwise land after this and
      // replace the view without the message just sent.
      loadSeq.current++;
      liveBuffer.current = null;
      syncedTurns.current.add(threadId);
      setComposerHint(null);
      // The person has spoken since the AI gave up, so that notice is stale.
      setAutofix((prev) => (prev?.state === "gave_up" ? null : prev));
      setView((v) => applyUserMessage(v, message, origin));
      markRunning(threadId, true);
      // A send that fails from here on still went to the chat: its error
      // shows in the transcript, right under the message.
      agentSend(projectPath, threadId, message, origin, roleOverride ?? roleRef.current).catch((err) => {
        markRunning(threadId, false);
        syncedTurns.current.delete(threadId);
        if (activeRef.current === threadId) setView((v) => applyLocalError(v, String(err)));
      });
      return "sent";
    },
    [projectPath],
  );

  // One stable function handed to the parent, always calling the latest
  // `send` — so it stays valid across re-renders and project switches.
  const sendRef = useRef(send);
  useEffect(() => {
    sendRef.current = send;
  }, [send]);
  const stableSend = useCallback((text: string) => sendRef.current(text), []);
  useEffect(() => {
    onRegisterSend(stableSend);
  }, [onRegisterSend, stableSend]);

  const anyRunning = running.size > 0;
  useEffect(() => {
    onTurnRunningChange?.(anyRunning);
  }, [onTurnRunningChange, anyRunning]);

  // The onboarding's first build: once this project's chat has opened,
  // switch to the thread it was prepared in (re-reading the list — the
  // thread was just created on disk) and send its message, exactly once.
  const takenCallback = useRef(onPendingTurnTaken);
  useEffect(() => {
    takenCallback.current = onPendingTurnTaken;
  });
  const chatOpen = initDoneFor === projectPath && !loading && problem === null;
  useEffect(() => {
    if (!pendingTurn || !chatOpen) return;
    const key = `${projectPath}\n${pendingTurn.threadId}\n${pendingTurn.message}`;
    if (pendingTaken.current === key) return;
    pendingTaken.current = key;
    const project = projectPath;
    void (async () => {
      const opened = await openThread(pendingTurn.threadId, { refreshList: true });
      if (projectRef.current !== project) return;
      if (!opened || activeRef.current !== pendingTurn.threadId) {
        // Not sent: the thread didn't open (its error is on screen). Allow
        // another go once the chat is open again (e.g. after "Try again").
        pendingTaken.current = null;
        return;
      }
      if (pendingTurn.role) setRole(pendingTurn.role);
      sendRef.current(pendingTurn.message, pendingTurn.origin, pendingTurn.role);
      takenCallback.current?.();
    })();
  }, [pendingTurn, chatOpen, projectPath, openThread]);

  function handleComposerSend() {
    const text = draft;
    setDraft("");
    send(text);
  }

  const handleApprovePlan = useCallback(() => {
    sendRef.current(PLAN_APPROVAL_MESSAGE, "plan_approval");
  }, []);

  const handleChangePlan = useCallback(() => {
    setComposerHint(CHANGE_PLAN_HINT);
    composerRef.current?.focus();
  }, []);

  async function handleStop() {
    const threadId = activeRef.current;
    if (!threadId) return;
    setStopping(true);
    try {
      await agentCancel(threadId);
      // The cancel only asks the AI to stop: the backend still holds the
      // turn while the CLI exits (a few seconds' grace) and the post-turn
      // snapshot is taken, and refuses a new message until then. So the
      // composer stays on "Stopping…" until `agent-turn-finished` for this
      // thread (handled above). That should always follow, but don't rely
      // on it: after STOP_FALLBACK_MS the composer frees itself so it can
      // never stay stuck (a send that's still too early then gets the
      // backend's own "still working" error in the transcript).
      clearStopFallback();
      stopFallback.current = window.setTimeout(() => {
        stopFallback.current = null;
        markRunning(threadId, false);
        syncedTurns.current.delete(threadId);
        if (activeRef.current === threadId) {
          busyRef.current = false;
          setStopping(false);
          setView((v) => endTurn(v));
        }
      }, STOP_FALLBACK_MS);
    } catch (err) {
      setStopping(false);
      setView((v) => applyEvent(v, { type: "error", kind: "other", message: `Couldn't stop the AI: ${String(err)}` }));
    }
  }

  async function handleCreateThread(title: string) {
    const thread = await chatCreateThread(projectPath, title);
    setThreads((prev) => [...prev, thread]);
    clearStopFallback();
    setStopping(false);
    setComposerHint(null);
    await showThread(thread.id);
  }

  function handleSelectThread(threadId: string) {
    if (threadId === activeRef.current) return;
    clearStopFallback();
    setStopping(false);
    setComposerHint(null);
    void showThread(threadId);
  }

  // Keep following new output while the user is at (or near) the bottom.
  useEffect(() => {
    const el = scrollRef.current;
    if (el && stickToBottom.current) el.scrollTop = el.scrollHeight;
  }, [view, loading]);

  const ready = !loading && problem === null && activeThreadId !== null;
  const provider = threads.find((t) => t.id === activeThreadId)?.provider ?? null;

  return (
    <div
      data-testid="chat-panel"
      data-busy={busy ? "true" : "false"}
      data-ready={ready ? "true" : "false"}
      className="flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden"
    >
      <div data-tauri-drag-region className="flex h-9 shrink-0 items-center gap-2 px-3">
        <span className="text-xs font-medium tracking-wide text-muted-foreground">Chat</span>
        <ThreadPicker
          threads={threads}
          activeThreadId={activeThreadId}
          runningThreadIds={running}
          onSelect={handleSelectThread}
          onCreate={handleCreateThread}
          disabled={threads.length === 0}
        />
        <div className="ml-auto flex min-w-0 items-center gap-1">
          {view.model && (
            <span className="truncate text-[11px] text-muted-foreground/70" title="Model reported by the AI">
              {view.model}
            </span>
          )}
          <StudioSettingsPopover projectPath={projectPath} />
        </div>
      </div>

      <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-xl border border-border bg-card">
        <div className="flex shrink-0 flex-col gap-2 px-3 pt-3 empty:hidden">
          <AutoFixBanner state={autofix} onDismiss={() => setAutofix(null)} />
          <AgentStatusBanner lastTurnError={view.lastTurnError} />
        </div>

        <div
          ref={scrollRef}
          onScroll={(e) => {
            const el = e.currentTarget;
            stickToBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < STICK_THRESHOLD;
          }}
          role="log"
          aria-live="polite"
          aria-label="Conversation"
          className="min-h-0 flex-1 overflow-y-auto px-3 py-3"
        >
          {loading ? (
            <div className="flex h-full items-center justify-center gap-2 text-xs text-muted-foreground">
              <Loader2 className="size-3.5 animate-spin" />
              Opening conversation…
            </div>
          ) : problem ? (
            <div className="flex h-full flex-col items-center justify-center gap-3 px-4 text-center">
              <AlertCircle className="size-5 text-destructive" />
              <div className="flex flex-col gap-1">
                <span className="text-sm font-medium">Couldn't open the conversation</span>
                <span className="font-mono text-xs break-words text-muted-foreground">{problem.message}</span>
              </div>
              <Button type="button" size="sm" variant="outline" onClick={problem.retry}>
                <RefreshCw />
                Try again
              </Button>
            </div>
          ) : view.items.length === 0 ? (
            <div className="flex h-full flex-col items-center justify-center gap-2 px-4 text-center">
              <MessageSquare className="size-5 text-muted-foreground" />
              <span className="max-w-xs text-sm text-muted-foreground">
                Tell the AI what you'd like to make or change in your game.
              </span>
            </div>
          ) : (
            <ChatTranscript
              items={view.items}
              turnInProgress={busy}
              provider={provider}
              onApprovePlan={handleApprovePlan}
              onChangePlan={handleChangePlan}
            />
          )}
        </div>

        <div className="shrink-0 p-3 pt-0">
          <ChatComposer
            value={draft}
            onChange={setDraft}
            onSend={handleComposerSend}
            onStop={() => void handleStop()}
            busy={busy}
            stopping={stopping}
            disabled={!ready}
            hint={composerHint}
            inputRef={composerRef}
            role={role}
            onRoleChange={setRole}
          />
        </div>
      </div>
    </div>
  );
}
