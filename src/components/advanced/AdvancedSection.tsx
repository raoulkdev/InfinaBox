import { useState } from "react";
import { motion } from "motion/react";
import { Bot } from "lucide-react";
import { BuildRow } from "@/components/cockpit/BuildRow";
import { ConnectAiPanel } from "@/components/connect/ConnectAiPanel";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { fadeTransition } from "@/lib/motion";
import { GodotSettings } from "./GodotSettings";

// The Advanced section (product spec §7.8): everything under the hood, for
// people who grow into it — the project's code and a real terminal (the
// pre-Studio Build workspace, unchanged), and app settings (which Godot,
// which AI). It's one of App.tsx's permanently mounted sections: the
// terminal's shell session must survive navigating away and back.

export interface AdvancedSectionProps {
  projectPath: string | null;
}

type AdvancedTab = "code" | "settings";

const TABS: { id: AdvancedTab; label: string }[] = [
  { id: "code", label: "Code & terminal" },
  { id: "settings", label: "Settings" },
];

export function AdvancedSection({ projectPath }: AdvancedSectionProps) {
  const [tab, setTab] = useState<AdvancedTab>("code");
  // Bumped each time Settings is shown, so the Godot status is re-read
  // then rather than only once at startup.
  const [settingsShown, setSettingsShown] = useState(0);

  function selectTab(next: AdvancedTab) {
    setTab(next);
    if (next === "settings") setSettingsShown((n) => n + 1);
  }

  return (
    <Tabs
      value={tab}
      onValueChange={(value) => selectTab(value as AdvancedTab)}
      className="flex h-full min-h-0 min-w-0 flex-1 flex-col gap-2"
    >
      <div
        data-tauri-drag-region
        className="flex shrink-0 items-center gap-3 rounded-xl border border-border bg-card px-3 py-2"
      >
        <div data-tauri-drag-region className="flex min-w-0 flex-1 flex-col">
          <span className="text-xs font-medium tracking-wide text-muted-foreground">Advanced</span>
          <p className="truncate text-sm">Your game's code, a terminal, and settings, for when you want to look under the hood.</p>
        </div>
        <TabsList>
          {TABS.map((t) => (
            <TabsTrigger key={t.id} value={t.id} className="px-3">
              {t.label}
            </TabsTrigger>
          ))}
        </TabsList>
      </div>

      {/* Both tabs stay mounted, stacked in one slot with the same
          opacity + `inert` treatment App.tsx gives its persistent sections
          (see the comment there): the terminal's shell and an unsaved code
          edit must survive a trip to Settings. `forceMount` keeps Radix
          from unmounting the inactive tab's content. */}
      <div className="relative min-h-0 flex-1">
        <TabsContent value="code" forceMount asChild>
          <motion.div
            className="absolute inset-0 flex min-h-0 min-w-0 overflow-hidden"
            animate={{ opacity: tab === "code" ? 1 : 0 }}
            style={{ pointerEvents: tab === "code" ? "auto" : "none" }}
            transition={fadeTransition}
            inert={tab !== "code"}
          >
            {/* TerminalPanel spawns its shell once, on mount, using
                whatever projectPath it was given at that instant (see its
                own comment on why it never re-cwds later) — so it must
                not mount at all until a real project is chosen, or it'd
                spawn in the user's home directory instead. */}
            <BuildRow projectPath={projectPath} showTerminal={projectPath !== null} />
          </motion.div>
        </TabsContent>
        <TabsContent value="settings" forceMount asChild>
          <motion.div
            className="absolute inset-0 flex min-h-0 min-w-0 overflow-hidden"
            animate={{ opacity: tab === "settings" ? 1 : 0 }}
            style={{ pointerEvents: tab === "settings" ? "auto" : "none" }}
            transition={fadeTransition}
            inert={tab !== "settings"}
          >
            <ScrollArea className="min-h-0 flex-1">
              <div className="mx-auto flex w-full max-w-2xl flex-col gap-2 pb-2">
                <GodotSettings projectPath={projectPath} refreshToken={settingsShown} />
                <section className="flex flex-col gap-4 rounded-xl border border-border bg-card p-4">
                  <div className="flex items-start gap-3">
                    <div className="flex size-9 shrink-0 items-center justify-center rounded-lg border border-border">
                      <Bot className="size-4 text-muted-foreground" />
                    </div>
                    <div className="flex min-w-0 flex-1 flex-col gap-0.5">
                      <h2 className="text-sm font-medium">Your AI</h2>
                      <p className="text-sm text-muted-foreground">
                        The AI that builds your game. It runs on your computer with your own account;
                        InfinaBox never stores your sign-in.
                      </p>
                    </div>
                  </div>
                  <ConnectAiPanel />
                </section>
              </div>
            </ScrollArea>
          </motion.div>
        </TabsContent>
      </div>
    </Tabs>
  );
}
