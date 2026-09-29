import { useEffect, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AnimatePresence, MotionConfig, motion } from "motion/react";
import { Rocket } from "lucide-react";
import { Sidebar, type Section } from "@/components/cockpit/Sidebar";
import { DashboardSection } from "@/components/cockpit/DashboardSection";
import { NotBuiltYetSection } from "@/components/cockpit/NotBuiltYetSection";
import { AdvancedSection } from "@/components/advanced/AdvancedSection";
import { ContextSection } from "@/components/context/ContextSection";
import { AssetsSection } from "@/components/assets/AssetsSection";
import { StudioSection } from "@/components/studio/StudioSection";
import { fadeRise, fadeTransition, springTransition } from "@/lib/motion";
import { recordProjectOpened } from "@/lib/recent-projects";
import type { PendingTurn } from "@/lib/studio-types";
import { cn } from "@/lib/utils";

// Every section besides Home/Studio/Context/Advanced mounts only when
// selected — none of them own state worth preserving across a tab switch.
// Studio's live AI turn stream and game tracking, Context's unsaved card
// and graph edits, and Advanced's live terminal are the real exceptions,
// handled separately below by staying permanently mounted. The rest (today
// Assets, and Playtest & Launch, an honest "not built yet" placeholder)
// share this one render map so adding a real one later doesn't mean
// copy-pasting another `{section === "x" && (...)}` block into an
// ever-growing if-chain.
type SimpleSection = Exclude<Section, "home" | "studio" | "context" | "advanced">;

// The three panels that stay permanently mounted (see the comment further
// down) crossfade between each other via opacity instead of the instant
// `hidden` swap every other section uses — the one place in this file
// where a real DOM mount/unmount (what AnimatePresence needs) would lose
// state, so the animation has to be opacity-driven instead.
const PERSISTENT_SECTIONS = ["studio", "context", "advanced"] as const;
type PersistentSection = (typeof PERSISTENT_SECTIONS)[number];

function isPersistentSection(section: Section): section is PersistentSection {
  return (PERSISTENT_SECTIONS as readonly Section[]).includes(section);
}

function App() {
  // Home is the landing section — a project (and its terminal session)
  // only comes to life once the user actually opens one from there or via
  // the Sidebar's project button, rather than dropping straight into
  // Studio.
  const [section, setSection] = useState<Section>("home");
  // The currently open project folder — shared by Studio, Context, and
  // Advanced's code browser and terminal starting directory.
  // Starts `null` on every launch — no project is auto-restored, even if
  // one was open last time — so Home always lands with nothing selected;
  // it only becomes real, user-driven state once the user opens a project
  // via the Sidebar's picker or the Home dashboard.
  const [projectPath, setProjectPath] = useState<string | null>(null);
  // A turn Studio should start as soon as it shows this project — the
  // first build a brand-new game from the onboarding interview hands over
  // (see DashboardSection). Kept with the project it belongs to, and only
  // passed to Studio while that project is the open one, so opening some
  // other project first can never send it to the wrong game.
  const [pendingTurn, setPendingTurn] = useState<{ projectPath: string; turn: PendingTurn } | null>(null);

  // Every panel that reads from disk (file trees, open file content, git
  // Overview/Changes) listens for `project-fs-changed` — this is what
  // actually makes that event fire, by pointing the Rust-side watcher at
  // whichever project is currently open. Re-running on every `projectPath`
  // change retargets the watch onto the new root; the backend itself drops
  // the old watch when a new one is started (see `watch_project_path`).
  useEffect(() => {
    if (!projectPath) return;
    void invoke("watch_project_path", { path: projectPath });
  }, [projectPath]);

  // The Sidebar's project button keeps the user wherever they already
  // are; opening (or creating) a project from the Home dashboard also
  // jumps to Studio — an open project's default screen (product spec §11)
  // — since that's the whole point of picking one from there.
  function handleOpenProject(path: string) {
    recordProjectOpened(path);
    setProjectPath(path);
    // Opening any project drops a first build Studio hasn't started yet —
    // it belongs to the moment its game was created, not to a later visit.
    setPendingTurn(null);
  }

  function handleOpenProjectFromDashboard(path: string, turn?: PendingTurn) {
    handleOpenProject(path);
    setPendingTurn(turn ? { projectPath: path, turn } : null);
    setSection("studio");
  }

  const simpleSections: Record<SimpleSection, () => ReactNode> = {
    assets: () => <AssetsSection projectPath={projectPath} />,
    launch: () => (
      <NotBuiltYetSection
        icon={Rocket}
        description="Sharing test builds, collecting feedback, release builds and publishing your game are coming in a later update."
      />
    ),
  };

  return (
    // MotionConfig with reducedMotion="user" makes every animation in this
    // tree respect the OS-level "reduce motion" accessibility setting
    // automatically — Motion shortens/skips transitions for users who have
    // it on, with no per-component opt-in needed.
    <MotionConfig reducedMotion="user">
      {/* No dedicated top bar — every block is the same full height it
          always was. macOS still draws the traffic lights at a fixed
          offset from the window's real top-left corner (see
          trafficLightPosition in tauri.conf.json), independent of any DOM
          element. Only the Sidebar sits under them (it's the leftmost
          block, the only one whose top-left corner the lights actually
          land on) — see its own comment for how it stays full height
          while still clearing them.

          The root here carries the window's drag region. `data-tauri-drag-region`
          is purely a data attribute Tauri's own JS layer reads on
          `pointerdown` to start an OS window-drag — despite an earlier,
          disproven theory in this codebase's history, it never sets any
          CSS `pointer-events` value, and nothing here needs to "opt back
          into" pointer-events just because an ancestor carries it. (Every
          element's own `pointer-events` stays the plain CSS default,
          `auto`, unless something *else* — like the persistent-tab
          crossfade just below — deliberately sets it to `none`.) That's
          what makes a block's empty header strip (the "Files" label, the
          "Agent" label, ...) still draggable — you can grab the top of any
          block, not just the gaps between them — while the buttons and
          text inside it stay clickable for free, with no extra class
          needed. */}
      <div
        data-tauri-drag-region
        className="flex h-screen w-screen gap-2 overflow-hidden bg-background p-2 text-foreground"
      >
        <AnimatePresence initial={false}>
          {section !== "home" && (
            <motion.div
              key="sidebar"
              initial={{ opacity: 0, x: -8 }}
              animate={{ opacity: 1, x: 0 }}
              exit={{ opacity: 0, x: -8 }}
              transition={springTransition}
              className="flex h-full"
            >
              <Sidebar
                active={section}
                onSelect={setSection}
                projectPath={projectPath}
                onOpenProject={handleOpenProject}
              />
            </motion.div>
          )}
        </AnimatePresence>

        <AnimatePresence mode="wait" initial={false}>
          {section === "home" && (
            <motion.div
              key="home"
              {...fadeRise}
              transition={fadeTransition}
              className="flex min-h-0 min-w-0 flex-1 overflow-hidden"
            >
              <DashboardSection onOpenProject={handleOpenProjectFromDashboard} />
            </motion.div>
          )}
        </AnimatePresence>

        {/* Studio, Context, and Advanced stay mounted even when hidden —
            Studio holds a live AI turn stream (events arrive only once; a
            remounted chat would miss the rest of a turn) and the Play
            panel's view of a running game, Context can hold an unsaved
            card or graph edit, and Advanced owns the live terminal session
            (never respawn/re-cwd it, see TerminalPanel's own comment).
            Because none of the three ever actually unmounts, AnimatePresence
            (which animates mount/unmount) can't crossfade between them —
            instead all three sit absolutely
            stacked in this one slot, permanently mounted, and only their
            opacity/pointer-events toggle with `section`. The slot itself
            still collapses via `hidden` exactly like every other section
            when none of the three is active, so it never steals flex
            space from Home or a simple section. (With no project open,
            Studio and Context just render their own "No project open"
            card, so there's nothing to defer mounting for — unlike
            Advanced's terminal, which AdvancedSection holds back itself.)
            Context and Advanced each stack their own two tabs the same way
            inside, for the same reasons.

            Each inactive wrapper also carries the `inert` HTML attribute,
            not just `pointer-events: none` — belt and suspenders. `inert`
            is a browser-native "this subtree doesn't exist for interaction
            purposes" switch: it blocks clicks/hover the same way, but also
            pulls every descendant out of the tab order and out of the
            accessibility tree, and can't be quietly defeated by some
            future descendant adding its own `pointer-events-auto` (the
            exact mistake that caused this bug the first time — see the
            comment above). React 19's JSX types accept it natively on
            plain DOM elements; verified here it also passes through
            `motion.div` (whose props extend the same `HTMLAttributes`) and
            behaves correctly in the Tauri WKWebView (modern WebKit has
            supported `inert` since Safari 15.5). */}
        <div
          className={cn(
            "relative flex min-h-0 min-w-0 flex-1 overflow-hidden",
            !isPersistentSection(section) && "hidden",
          )}
        >
          <motion.div
            className="absolute inset-0 flex min-h-0 min-w-0 overflow-hidden"
            animate={{ opacity: section === "studio" ? 1 : 0 }}
            style={{ pointerEvents: section === "studio" ? "auto" : "none" }}
            transition={fadeTransition}
            inert={section !== "studio"}
          >
            <StudioSection
              projectPath={projectPath}
              pendingTurn={pendingTurn && pendingTurn.projectPath === projectPath ? pendingTurn.turn : null}
              onPendingTurnTaken={() => setPendingTurn(null)}
            />
          </motion.div>
          <motion.div
            className="absolute inset-0 flex min-h-0 min-w-0 overflow-hidden"
            animate={{ opacity: section === "context" ? 1 : 0 }}
            style={{ pointerEvents: section === "context" ? "auto" : "none" }}
            transition={fadeTransition}
            inert={section !== "context"}
          >
            <ContextSection projectPath={projectPath} />
          </motion.div>
          <motion.div
            className="absolute inset-0 flex min-h-0 min-w-0 overflow-hidden"
            animate={{ opacity: section === "advanced" ? 1 : 0 }}
            style={{ pointerEvents: section === "advanced" ? "auto" : "none" }}
            transition={fadeTransition}
            inert={section !== "advanced"}
          >
            <AdvancedSection projectPath={projectPath} />
          </motion.div>
        </div>

        <AnimatePresence mode="wait" initial={false}>
          {section !== "home" && !isPersistentSection(section) && (
            <motion.div
              key={section}
              {...fadeRise}
              transition={fadeTransition}
              className="flex min-h-0 min-w-0 flex-1 overflow-hidden"
            >
              {simpleSections[section]()}
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    </MotionConfig>
  );
}

export default App;
