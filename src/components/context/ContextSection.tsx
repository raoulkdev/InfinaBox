import { useState } from "react";
import { motion } from "motion/react";
import { BookOpen } from "lucide-react";
import { FileBrowser } from "@/components/cockpit/FileBrowser";
import { GraphsSection } from "@/components/cockpit/GraphsSection";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { fadeTransition } from "@/lib/motion";

// The Context section (product spec §7.2): the one model of the game —
// its concept, style guide, mechanics and tasks — as Markdown cards in
// `.ibproject/context/`, plus graph views over `.ibproject/graphs/`. Both
// the person and their AI read and write these cards. It's one of
// App.tsx's permanently mounted sections: either tab can hold an unsaved
// card or graph edit, so neither is ever unmounted by switching away.

export interface ContextSectionProps {
  projectPath: string | null;
}

// Same hidden-dotfolder convention the old Documents section used:
// `list_directory`'s dotfile filter (src-tauri/src/commands/fs.rs) keeps
// `.ibproject` out of general file browsers, while this section browses it
// directly by path, which that filter doesn't apply to.
const CONTEXT_DIR = ".ibproject/context";

type ContextTab = "cards" | "graphs";

const TABS: { id: ContextTab; label: string }[] = [
  { id: "cards", label: "Cards" },
  { id: "graphs", label: "Graphs" },
];

export function ContextSection({ projectPath }: ContextSectionProps) {
  const [tab, setTab] = useState<ContextTab>("cards");

  return (
    <Tabs
      value={tab}
      onValueChange={(value) => setTab(value as ContextTab)}
      className="flex h-full min-h-0 min-w-0 flex-1 flex-col gap-2"
    >
      <div
        data-tauri-drag-region
        className="flex shrink-0 items-center gap-3 rounded-xl border border-border bg-card px-3 py-2"
      >
        <div data-tauri-drag-region className="flex min-w-0 flex-1 flex-col">
          <span className="text-xs font-medium tracking-wide text-muted-foreground">Context</span>
          <p className="truncate text-sm">
            What InfinaBox and your AI know about your game: its idea, style, mechanics and plans.
          </p>
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
        // Both tabs stay mounted, stacked in one slot — the same
        // opacity + `inert` treatment App.tsx gives its persistent
        // sections (see the comment there), so a half-edited card survives
        // a look at the graphs and back. `forceMount` keeps Radix from
        // unmounting the inactive tab's content.
        <div className="relative min-h-0 flex-1">
          <TabsContent value="cards" forceMount asChild>
            <motion.div
              className="absolute inset-0 flex min-h-0 min-w-0 overflow-hidden"
              animate={{ opacity: tab === "cards" ? 1 : 0 }}
              style={{ pointerEvents: tab === "cards" ? "auto" : "none" }}
              transition={fadeTransition}
              inert={tab !== "cards"}
            >
              <FileBrowser
                label="Cards"
                rootPath={`${projectPath}/${CONTEXT_DIR}`}
                forceMdExtension
                variant="list"
              />
            </motion.div>
          </TabsContent>
          <TabsContent value="graphs" forceMount asChild>
            <motion.div
              className="absolute inset-0 flex min-h-0 min-w-0 overflow-hidden"
              animate={{ opacity: tab === "graphs" ? 1 : 0 }}
              style={{ pointerEvents: tab === "graphs" ? "auto" : "none" }}
              transition={fadeTransition}
              inert={tab !== "graphs"}
            >
              <GraphsSection projectPath={projectPath} />
            </motion.div>
          </TabsContent>
        </div>
      ) : (
        // Honest empty state, like Studio's: Context belongs to a project.
        <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-2 rounded-xl border border-border bg-card text-center">
          <div className="flex size-10 items-center justify-center rounded-lg border border-border">
            <BookOpen className="size-5 text-muted-foreground" />
          </div>
          <h2 className="text-lg font-medium tracking-tight">No project open</h2>
          <p className="max-w-xs text-sm text-muted-foreground">
            Create or open a project from Home to see and edit what your AI knows about your game.
          </p>
        </div>
      )}
    </Tabs>
  );
}
