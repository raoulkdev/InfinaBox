import { useCallback, useRef, useState } from "react";
import { Sparkles } from "lucide-react";
import { ResizablePanelGroup } from "@/components/cockpit/ResizablePanelGroup";
import { ChatPanel, type ChatSendOutcome } from "@/components/studio/chat/ChatPanel";
import { HistoryPanel } from "@/components/studio/history/HistoryPanel";
import { PlayPanel } from "@/components/studio/play/PlayPanel";

// The Studio section (product spec §7.1): the conversation with the user's
// own AI on one side, the running game and its history on the other. It's
// one of App.tsx's permanently mounted sections — a turn streaming into
// the chat, or a game the Play panel is tracking, must survive switching
// to another tab and back.

export interface StudioSectionProps {
  projectPath: string | null;
}

/** Chat / (Play over History), laid out via the shared
 * `ResizablePanelGroup` system — resizable against each other and dragged
 * into either order, persisted under the "studio" storage key, the same
 * way Build's Agent/Code row is. */
export function StudioSection({ projectPath }: StudioSectionProps) {
  // ChatPanel hands over its `send` once on mount; the Play panel's "Ask AI
  // to fix" calls through this ref, so neither panel owns the other's state
  // and a re-registered `send` never re-renders Play.
  // `null` back from `askAiToFix` means the chat hasn't registered yet, so
  // nothing happened — the caller must not claim otherwise.
  const sendRef = useRef<((text: string) => ChatSendOutcome) | null>(null);
  const registerSend = useCallback((send: (text: string) => ChatSendOutcome) => {
    sendRef.current = send;
  }, []);
  const askAiToFix = useCallback(
    (message: string): ChatSendOutcome | null => sendRef.current?.(message) ?? null,
    [],
  );

  // Whether an AI turn is running (reported by the chat): History holds off
  // undo/go back meanwhile, since the AI may still be writing files.
  const [aiWorking, setAiWorking] = useState(false);

  // Honest empty state rather than panels querying a project that isn't
  // there: every Studio panel reads from (and writes to) a real project.
  if (!projectPath) {
    return (
      <div className="flex h-full min-w-0 flex-1 flex-col overflow-hidden rounded-xl border border-border bg-card">
        <div data-tauri-drag-region className="flex h-9 shrink-0 items-center px-3">
          <span className="text-xs font-medium tracking-wide text-muted-foreground">Studio</span>
        </div>
        <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 pb-9 text-center">
          <div className="flex size-10 items-center justify-center rounded-lg border border-border">
            <Sparkles className="size-5 text-muted-foreground" />
          </div>
          <h2 className="text-lg font-medium tracking-tight">No project open</h2>
          <p className="max-w-xs text-sm text-muted-foreground">
            Create or open a project from Home to chat with your AI and play your game here.
          </p>
        </div>
      </div>
    );
  }

  return (
    <ResizablePanelGroup
      storageKey="studio"
      panels={[
        {
          id: "chat",
          defaultPercent: 50,
          minPercent: 25,
          // ChatPanel is a bare column (Play/History draw their own card),
          // so it gets the same card shell here.
          content: (
            <div className="flex h-full min-h-0 flex-col overflow-hidden rounded-xl border border-border bg-card">
              <ChatPanel
                projectPath={projectPath}
                onRegisterSend={registerSend}
                onTurnRunningChange={setAiWorking}
              />
            </div>
          ),
        },
        {
          id: "game",
          defaultPercent: 50,
          minPercent: 25,
          // Play gets the larger share: it carries the run controls, the
          // error list, and the live output log; History is a plain list.
          // `gap-2` matches the row gap `ResizablePanelGroup` puts between
          // blocks.
          content: (
            <div className="flex h-full min-h-0 flex-col gap-2">
              <div className="min-h-0 flex-[3]">
                <PlayPanel projectPath={projectPath} onAskAiToFix={askAiToFix} />
              </div>
              <div className="min-h-0 flex-[2]">
                <HistoryPanel projectPath={projectPath} aiWorking={aiWorking} />
              </div>
            </div>
          ),
        },
      ]}
    />
  );
}
