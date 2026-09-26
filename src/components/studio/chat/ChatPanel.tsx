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
} from "@/lib/studio-api";
import type { ThreadSummary } from "@/lib/studio-types";
import { AgentStatusBanner } from "./AgentStatusBanner";
import { ChatComposer } from "./ChatComposer";
import { ChatTranscript } from "./ChatTranscript";
import { ThreadPicker } from "./ThreadPicker";
import {
  applyEvent,
  applyLocalError,
  applyUserMessage,
  buildChatView,
  emptyChatView,
  endTurn,
  type ChatView,
} from "./chat-reducer";

// Studio's chat with the user's own AI (spec §7.1). Everything shown comes
// from the thread's saved records (`chat_load_thread`) or the live
// `agent-event` stream — see chat-reducer.ts for how both fold into the same
// view. Backend failures are shown with their real error text, never
// papered over.

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
}

/** What a brand-new project's first conversation is called. */
const DEFAULT_THREAD_TITLE = "Main";

// How close to the bottom (px) still counts as "following along", so new
// output keeps scrolling into view — but not if the user scrolled up to
// reread something.
const STICK_THRESHOLD = 48;

type Problem = { message: string; retry: () => void };

function latestThread(threads: ThreadSummary[]): ThreadSummary {
  return threads.reduce((a, b) => (b.created_at > a.created_at ? b : a));
}

export function ChatPanel({ projectPath, onRegisterSend }: ChatPanelProps) {
  const [threads, setThreads] = useState<ThreadSummary[]>([]);
  const [activeThreadId, setActiveThreadId] = useState<string | null>(null);
  const [view, setView] = useState<ChatView>(emptyChatView);
  const [loading, setLoading] = useState(true);
  const [problem, setProblem] = useState<Problem | null>(null);
  const [draft, setDraft] = useState("");
  const [stopping, setStopping] = useState(false);
  // Threads with a turn in flight, including ones in the background after
  // switching away. Kept separately from `view.turnInProgress` because the
  // backend's turn only really ends at `agent-turn-finished` (after its
  // post-turn snapshot), a moment after the agent's own `turn_completed`.
  const [running, setRunning] = useState<ReadonlySet<string>>(new Set());
  const [initAttempt, setInitAttempt] = useState(0);

  // Mirrors of the latest state for the event subscriptions and the
  // registered `send`, which are created once and must not go stale.
  const activeRef = useRef<string | null>(null);
  const loadingRef = useRef(true);
  const runningRef = useRef<ReadonlySet<string>>(running);
  const busyRef = useRef(false);
  const loadSeq = useRef(0);
  // The in-flight "find or create the first thread" request, reused if this
  // effect runs twice for the same project (StrictMode's mount → cleanup →
  // mount), so an empty project never ends up with two "Main" threads.
  const initRequest = useRef<{ key: string; promise: Promise<ThreadSummary[]> } | null>(null);

  const scrollRef = useRef<HTMLDivElement | null>(null);
  const stickToBottom = useRef(true);

  const busy = view.turnInProgress || (activeThreadId !== null && running.has(activeThreadId));

  useEffect(() => {
    activeRef.current = activeThreadId;
    loadingRef.current = loading;
    runningRef.current = running;
    busyRef.current = busy;
  });

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

  /** Loads a thread's saved records and makes it the displayed one.
   * `quiet` keeps the current view on screen while reloading (used to
   * resync after a turn), and keeps it if the reload fails. */
  const showThread = useCallback(
    async (threadId: string, { quiet = false } = {}) => {
      const seq = ++loadSeq.current;
      activeRef.current = threadId;
      setActiveThreadId(threadId);
      if (!quiet) {
        loadingRef.current = true;
        setLoading(true);
        setProblem(null);
        stickToBottom.current = true;
      }
      try {
        const loaded = await chatLoadThread(projectPath, threadId);
        if (seq !== loadSeq.current) return;
        const next = buildChatView(loaded.records, { running: runningRef.current.has(threadId) });
        // A quiet resync right after a live turn keeps that turn's error for
        // the status banner; the saved file alone never sets it (see
        // `buildChatView`), so reopening an old failed thread shows no banner.
        setView((v) => (quiet ? { ...next, lastTurnError: v.lastTurnError } : next));
      } catch (err) {
        if (seq !== loadSeq.current || quiet) return;
        setView(emptyChatView);
        setProblem({ message: String(err), retry: () => void showThread(threadId) });
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
    setActiveThreadId(null);
    setThreads([]);
    setView(emptyChatView);
    setLoading(true);
    setProblem(null);
    setDraft("");

    initRequest.current.promise.then(
      (list) => {
        if (cancelled) return;
        setThreads(list);
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
  // for the active thread that arrive mid-load are dropped here — the
  // resync on `agent-turn-finished` below picks them up from disk.
  useEffect(() => {
    const stopEvents = onAgentEvent(({ threadId, event }) => {
      markRunning(threadId, true);
      if (threadId !== activeRef.current || loadingRef.current) return;
      setView((v) => applyEvent(v, event));
    });
    const stopFinished = onAgentTurnFinished(({ threadId }) => {
      markRunning(threadId, false);
      if (threadId !== activeRef.current) return;
      setStopping(false);
      setView((v) => endTurn(v));
      void showThread(threadId, { quiet: true });
    });
    return () => {
      stopEvents();
      stopFinished();
    };
  }, [showThread]);

  // Decides synchronously whether the text goes to the AI or stays as a
  // draft (so the caller can say which), then sends in the background.
  const send = useCallback(
    (text: string): ChatSendOutcome => {
      const message = text.trim();
      const threadId = activeRef.current;
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
      setView((v) => applyUserMessage(v, message));
      markRunning(threadId, true);
      // A send that fails from here on still went to the chat: its error
      // shows in the transcript, right under the message.
      agentSend(projectPath, threadId, message).catch((err) => {
        markRunning(threadId, false);
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

  function handleComposerSend() {
    const text = draft;
    setDraft("");
    send(text);
  }

  async function handleStop() {
    const threadId = activeRef.current;
    if (!threadId) return;
    setStopping(true);
    try {
      await agentCancel(threadId);
      // `agent-turn-finished` should follow a cancel, but don't rely on it:
      // once the backend has confirmed the stop, the composer must never
      // stay stuck on "Stop".
      markRunning(threadId, false);
      if (activeRef.current === threadId) {
        busyRef.current = false;
        setStopping(false);
        setView((v) => endTurn(v));
      }
    } catch (err) {
      setStopping(false);
      setView((v) => applyEvent(v, { type: "error", kind: "other", message: `Couldn't stop the AI: ${String(err)}` }));
    }
  }

  async function handleCreateThread(title: string) {
    const thread = await chatCreateThread(projectPath, title);
    setThreads((prev) => [...prev, thread]);
    setStopping(false);
    await showThread(thread.id);
  }

  function handleSelectThread(threadId: string) {
    if (threadId === activeRef.current) return;
    setStopping(false);
    void showThread(threadId);
  }

  // Keep following new output while the user is at (or near) the bottom.
  useEffect(() => {
    const el = scrollRef.current;
    if (el && stickToBottom.current) el.scrollTop = el.scrollHeight;
  }, [view, loading]);

  const ready = !loading && problem === null && activeThreadId !== null;

  return (
    <div className="flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
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
        {view.model && (
          <span className="ml-auto truncate text-[11px] text-muted-foreground/70" title="Model reported by the AI">
            {view.model}
          </span>
        )}
      </div>

      <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-xl border border-border bg-card">
        <div className="shrink-0 px-3 pt-3 empty:hidden">
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
            <ChatTranscript items={view.items} turnInProgress={busy} />
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
          />
        </div>
      </div>
    </div>
  );
}
