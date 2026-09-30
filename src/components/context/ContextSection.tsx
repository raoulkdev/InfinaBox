import { useCallback, useEffect, useState, type ReactNode } from "react";
import { motion } from "motion/react";
import { BookOpen } from "lucide-react";
import { GraphsSection } from "@/components/cockpit/GraphsSection";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { onProjectFilesChanged } from "@/lib/fs-watch";
import { fadeTransition } from "@/lib/motion";
import { contextList } from "@/lib/studio-api";
import type { CardSummary } from "@/lib/studio-types";
import { BoardTab } from "./BoardTab";
import { NotesTab, type OpenRequest } from "./NotesTab";
import type { AskAi } from "./aiActions";
import { MapTab } from "./MapTab";
import { errorText } from "./cardTypes";

// The Context section (product spec §7.2): the one model of the game —
// its concept, style guide, mechanics and tasks — as typed Markdown cards in
// `.ibproject/context/`, seen as a list (Cards), a task Board, a Map of the
// links between cards, and graph documents over `.ibproject/graphs/`. Both
// the person and their AI read and write these cards. It's one of
// App.tsx's permanently mounted sections: any tab can hold an unsaved card
// or graph edit, so no tab is ever unmounted by switching away.

export interface ContextSectionProps {
  projectPath: string | null;
  /** Sends a request to the AI in Studio's chat (with a role). */
  onAskAi: AskAi;
}

type ContextTab = "notes" | "board" | "map" | "graphs";

const TABS: { id: ContextTab; label: string }[] = [
  { id: "notes", label: "Notes" },
  { id: "board", label: "Board" },
  { id: "map", label: "Map" },
  { id: "graphs", label: "Graphs" },
];

export function ContextSection({ projectPath, onAskAi }: ContextSectionProps) {
  const [tab, setTab] = useState<ContextTab>("notes");

  return (
    <Tabs
      value={tab}
      onValueChange={(value) => setTab(value as ContextTab)}
      data-testid="context-section"
      className="flex h-full min-h-0 min-w-0 flex-1 flex-col gap-2"
    >
      <div
        data-tauri-drag-region
        className="flex shrink-0 items-center gap-3 rounded-xl border border-border bg-card px-3 py-2"
      >
        <div data-tauri-drag-region className="flex min-w-0 flex-1 flex-col">
          <span className="text-xs font-medium tracking-wide text-muted-foreground">Notes</span>
        </div>
        {projectPath && (
          <TabsList>
            {TABS.map((t) => (
              <TabsTrigger key={t.id} value={t.id} className="px-3">
                {t.label}
              </TabsTrigger>
            ))}
          </TabsList>
        )}
      </div>

      {projectPath ? (
        <ContextViews projectPath={projectPath} tab={tab} setTab={setTab} onAskAi={onAskAi} />
      ) : (
        // Honest empty state, like Studio's: Context belongs to a project.
        <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 rounded-xl border border-border bg-card text-center">
          <div className="flex size-10 items-center justify-center rounded-lg border border-border">
            <BookOpen className="size-5 text-muted-foreground" />
          </div>
          <h2 className="text-lg font-medium tracking-tight">No project open</h2>
        </div>
      )}
    </Tabs>
  );
}

/** The four tabs' content, for an open project. Owns the card list every
 * view shares and the "something changed on disk" tick that makes each view
 * re-read. */
function ContextViews({
  projectPath,
  tab,
  setTab,
  onAskAi,
}: {
  projectPath: string;
  tab: ContextTab;
  setTab: (tab: ContextTab) => void;
  onAskAi: AskAi;
}) {
  const [cards, setCards] = useState<CardSummary[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [tick, setTick] = useState(0);
  const [openRequest, setOpenRequest] = useState<OpenRequest | null>(null);

  const bump = useCallback(() => setTick((n) => n + 1), []);

  // Files changing on disk (the AI writing a card, git, our own Save)
  // refreshes every view.
  useEffect(() => onProjectFilesChanged(bump), [bump]);

  useEffect(() => {
    let cancelled = false;
    contextList(projectPath).then(
      (list) => {
        if (cancelled) return;
        setCards(list);
        setError(null);
      },
      (err) => {
        if (!cancelled) setError(errorText(err));
      },
    );
    return () => {
      cancelled = true;
    };
  }, [projectPath, tick]);

  const openCard = useCallback(
    (path: string) => {
      setOpenRequest((prev) => ({ path, nonce: (prev?.nonce ?? 0) + 1 }));
      setTab("notes");
    },
    [setTab],
  );

  // Every tab stays mounted, stacked in one slot — the same opacity +
  // `inert` treatment App.tsx gives its persistent sections (see the
  // comment there), so a half-edited card survives a look at the board and
  // back. `forceMount` keeps Radix from unmounting the inactive content.
  const pane = (id: ContextTab, children: ReactNode) => (
    <TabsContent value={id} forceMount asChild>
      <motion.div
        className="absolute inset-0 flex min-h-0 min-w-0 overflow-hidden"
        animate={{ opacity: tab === id ? 1 : 0 }}
        style={{ pointerEvents: tab === id ? "auto" : "none" }}
        transition={fadeTransition}
        inert={tab !== id}
      >
        {children}
      </motion.div>
    </TabsContent>
  );

  return (
    <div className="relative min-h-0 flex-1">
      {pane(
        "notes",
        <NotesTab
          projectPath={projectPath}
          cards={cards}
          cardsError={error}
          refreshTick={tick}
          openRequest={openRequest}
          onReload={bump}
          onChanged={bump}
          onAskAi={onAskAi}
        />,
      )}
      {pane(
        "board",
        <BoardTab
          projectPath={projectPath}
          refreshTick={tick}
          onOpen={openCard}
          onChanged={bump}
          existingPaths={(cards ?? []).map((c) => c.path)}
        />,
      )}
      {pane("map", <MapTab projectPath={projectPath} refreshTick={tick} onOpen={openCard} />)}
      {pane("graphs", <GraphsSection projectPath={projectPath} />)}
    </div>
  );
}
