import { useEffect, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AnimatePresence, MotionConfig, motion } from "motion/react";
import { Palette, Music2, LayoutTemplate, ShieldCheck, Radio } from "lucide-react";
import { BuildRow } from "@/components/cockpit/BuildRow";
import { Sidebar, type Section } from "@/components/cockpit/Sidebar";
import { DashboardSection } from "@/components/cockpit/DashboardSection";
import { DesignSection } from "@/components/cockpit/DesignSection";
import { GraphsSection } from "@/components/cockpit/GraphsSection";
import { BusinessSection } from "@/components/cockpit/BusinessSection";
import { MarketingSection } from "@/components/cockpit/MarketingSection";
import { CommunitySection } from "@/components/cockpit/CommunitySection";
import { ReleaseSection } from "@/components/cockpit/ReleaseSection";
import { NotBuiltYetSection } from "@/components/cockpit/NotBuiltYetSection";
import { StudioSection } from "@/components/studio/StudioSection";
import { fadeRise, fadeTransition, springTransition } from "@/lib/motion";
import { recordProjectOpened } from "@/lib/recent-projects";
import { cn } from "@/lib/utils";

// Every section besides Home/Studio/Build/Design/Graphs mounts only when
// selected — none of them own state worth preserving across a tab switch.
// Studio's live AI turn stream and game tracking, Build's live terminal,
// Design's unsaved draft, and Graphs' unsaved canvas edits are the real
// exceptions, handled separately below by staying permanently mounted.
// Real docs-backed sections and "not built yet" placeholders share this
// one render map so adding another discipline later doesn't mean
// copy-pasting another `{section === "x" && (...)}` block into an
// ever-growing if-chain.
type SimpleSection = Exclude<Section, "home" | "studio" | "build" | "design" | "graphs">;

// The four panels that stay permanently mounted (see the comment further
// down) crossfade between each other via opacity instead of the instant
// `hidden` swap every other section uses — the one place in this file
// where a real DOM mount/unmount (what AnimatePresence needs) would lose
// state, so the animation has to be opacity-driven instead.
const PERSISTENT_SECTIONS = ["studio", "build", "design", "graphs"] as const;
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
  // The currently open project folder — shared by the Build tab's code
  // browser and terminal starting directory, and every docs-style section.
  // Starts `null` on every launch — no project is auto-restored, even if
  // one was open last time — so Home always lands with nothing selected;
  // it only becomes real, user-driven state once the user opens a project
  // via the Sidebar's picker or the Home dashboard.
  const [projectPath, setProjectPath] = useState<string | null>(null);

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
  }

  function handleOpenProjectFromDashboard(path: string) {
    handleOpenProject(path);
    setSection("studio");
  }

  const simpleSections: Record<SimpleSection, () => ReactNode> = {
    art: () => (
      <NotBuiltYetSection
        icon={Palette}
        description="Asset briefs and previews arrive once the file browser can render images, not just markdown."
      />
    ),
    audio: () => (
      <NotBuiltYetSection
        icon={Music2}
        description="Asset briefs and playback arrive once the file browser can render audio, not just markdown."
      />
    ),
    uiux: () => (
      <NotBuiltYetSection
        icon={LayoutTemplate}
        description="Flow docs and mockup previews arrive once the file browser can render images, not just markdown."
      />
    ),
    qa: () => (
      <NotBuiltYetSection
        icon={ShieldCheck}
        description="Bug tracking and grounded commit lookups arrive in a later phase, once the core agent loop is proven."
      />
    ),
    release: () => <ReleaseSection projectPath={projectPath} />,
    business: () => <BusinessSection projectPath={projectPath} />,
    marketing: () => <MarketingSection projectPath={projectPath} />,
    community: () => <CommunitySection projectPath={projectPath} />,
    liveops: () => (
      <NotBuiltYetSection
        icon={Radio}
        description="Analytics, crash triage, and live events connect once there's a shipped game generating real data."
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

        {/* Studio, Build, Design, and Graphs stay mounted even when
            hidden — Studio holds a live AI turn stream (events arrive only
            once; a remounted chat would miss the rest of a turn) and the
            Play panel's view of a running game, Build owns the live
            terminal session (never respawn/re-cwd it, see TerminalPanel's
            own comment), and Design/Graphs can each hold an unsaved draft.
            Because none of the four ever actually unmounts, AnimatePresence
            (which animates mount/unmount) can't crossfade between them —
            instead all four sit absolutely
            stacked in this one slot, permanently mounted, and only their
            opacity/pointer-events toggle with `section`. The slot itself
            still collapses via `hidden` exactly like every other section
            when none of the four is active, so it never steals flex
            space from Home or a simple section. (With no project open,
            Studio just renders its own "No project open" card, so there's
            nothing to defer mounting for — unlike Build's terminal.)

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
            <StudioSection projectPath={projectPath} />
          </motion.div>
          <motion.div
            className="absolute inset-0 flex min-h-0 min-w-0 overflow-hidden"
            animate={{ opacity: section === "build" ? 1 : 0 }}
            style={{ pointerEvents: section === "build" ? "auto" : "none" }}
            transition={fadeTransition}
            inert={section !== "build"}
          >
            {/* TerminalPanel spawns its shell once, on mount, using
                whatever projectPath it was given at that instant (see its
                own comment on why it never re-cwds later) — so it must
                not mount at all until a real project is chosen, or it'd
                spawn in the user's home directory instead. */}
            <BuildRow projectPath={projectPath} showTerminal={projectPath !== null} />
          </motion.div>
          <motion.div
            className="absolute inset-0 flex min-h-0 min-w-0 overflow-hidden"
            animate={{ opacity: section === "design" ? 1 : 0 }}
            style={{ pointerEvents: section === "design" ? "auto" : "none" }}
            transition={fadeTransition}
            inert={section !== "design"}
          >
            <DesignSection projectPath={projectPath} />
          </motion.div>
          <motion.div
            className="absolute inset-0 flex min-h-0 min-w-0 overflow-hidden"
            animate={{ opacity: section === "graphs" ? 1 : 0 }}
            style={{ pointerEvents: section === "graphs" ? "auto" : "none" }}
            transition={fadeTransition}
            inert={section !== "graphs"}
          >
            <GraphsSection projectPath={projectPath} />
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
