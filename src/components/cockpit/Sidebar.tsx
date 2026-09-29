import { useEffect, useState, type ReactNode } from "react";
import { AnimatePresence, motion } from "motion/react";
import {
  Home,
  Sparkles,
  BookOpen,
  Images,
  Rocket,
  Wrench,
  Settings as SettingsIcon,
  FolderOpen,
  PanelLeftClose,
  PanelLeftOpen,
  type LucideIcon,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Separator } from "@/components/ui/separator";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { fadeTransition, springTransition, widthTransition } from "@/lib/motion";
import { pickAndOpenExistingProject, projectFolderName } from "@/lib/project-picker";
import { cn } from "@/lib/utils";

// Widths in px, matching the collapsed/expanded classes this replaced
// (`w-24` / `w-[212px]`) — Motion needs real numbers to animate `width`,
// not Tailwind classes.
const EXPANDED_WIDTH = 212;
const COLLAPSED_WIDTH = 89;

// Fades a label in/out instead of the abrupt appear/disappear a plain
// `{show && <span>}` gives — used everywhere a row's text needs to react
// to the sidebar collapsing, so that one visual behavior doesn't drift
// between the half-dozen labels in this file.
function FadeLabel({ show, children }: { show: boolean; children: ReactNode }) {
  return (
    <AnimatePresence initial={false}>
      {show && (
        <motion.span
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={fadeTransition}
          className="relative z-10 truncate"
        >
          {children}
        </motion.span>
      )}
    </AnimatePresence>
  );
}

// The six entries of the product spec's navigation (§11). The old
// per-discipline sections (Documents, Business, Marketing, ...) are gone:
// their docs now live as Context cards, and their tools arrive as parts of
// Assets and Playtest & Launch.
export type Section = "home" | "studio" | "context" | "assets" | "launch" | "advanced" | "settings";

interface SectionItem {
  id: Section;
  label: string;
  icon: LucideIcon;
}

// Home is pinned above the project button: it's the landing page (your
// games, your AI, Godot) and the one entry that isn't about the open
// project.
const PINNED: SectionItem[] = [{ id: "home", label: "Home", icon: Home }];

// Everything under the project button is a screen of the open project, in
// the order a game gets made: Studio (chat + the running game) is where
// it happens, Context is what InfinaBox and the AI know about the game,
// then Assets and Playtest & Launch, and Advanced (terminal, code,
// settings) last, for people who grow into it. A flat list — six entries
// don't need the discipline groups the old 13-entry sidebar did.
const PROJECT_SECTIONS: SectionItem[] = [
  { id: "studio", label: "Studio", icon: Sparkles },
  { id: "context", label: "Context", icon: BookOpen },
  { id: "assets", label: "Assets", icon: Images },
  { id: "launch", label: "Playtest & Launch", icon: Rocket },
  { id: "advanced", label: "Advanced", icon: Wrench },
];

// App settings sit at the foot of the sidebar, apart from the project's
// screens: they're about InfinaBox, not the open game.
const SETTINGS_ITEM: SectionItem = { id: "settings", label: "Settings", icon: SettingsIcon };

interface SidebarProps {
  active: Section;
  onSelect: (section: Section) => void;
  projectPath: string | null;
  onOpenProject: (path: string) => void;
}

// Persisted the same lightweight way recent-projects.ts persists its list —
// per-machine UI state with no reason to round-trip through Tauri, and no
// reason to lose it every time the app restarts.
const COLLAPSED_STORAGE_KEY = "infinabox.sidebarCollapsed";

function loadCollapsed(): boolean {
  try {
    return localStorage.getItem(COLLAPSED_STORAGE_KEY) === "true";
  } catch {
    return false;
  }
}

function SectionRow({
  item,
  active,
  collapsed,
  onSelect,
}: {
  item: SectionItem;
  active: boolean;
  collapsed: boolean;
  onSelect: () => void;
}) {
  const Icon = item.icon;
  const button = (
    <Button
      type="button"
      variant="ghost"
      size="sm"
      aria-pressed={active}
      data-testid={`nav-${item.id}`}
      onClick={onSelect}
      className={cn(
        "relative w-full gap-2 px-2 font-normal text-muted-foreground hover:text-foreground",
        collapsed ? "justify-center" : "justify-start",
        active && "text-foreground",
      )}
    >
      {/* A single shared element (via `layoutId`) that slides between
       * whichever row is active, rather than each row's own background
       * snapping on/off — the same "sliding tab indicator" pattern
       * motion.dev's own docs use as the canonical layout-animation
       * example. */}
      {active && (
        <motion.div
          layoutId="sidebar-active-indicator"
          className="absolute inset-0 rounded-md bg-accent"
          transition={springTransition}
        />
      )}
      <Icon className="relative z-10 size-4 shrink-0" />
      <FadeLabel show={!collapsed}>{item.label}</FadeLabel>
    </Button>
  );

  if (!collapsed) return button;

  return (
    <Tooltip>
      <TooltipTrigger asChild>{button}</TooltipTrigger>
      <TooltipContent side="right">{item.label}</TooltipContent>
    </Tooltip>
  );
}

// Replaces the old top bar's "Open Project" button — now that there's no
// dedicated header, it lives right under the pinned Home row, heading the
// open project's own screens, in every section
// that shows the sidebar (Home shows it too).
function ProjectButtonRow({
  projectPath,
  collapsed,
  onOpenProject,
}: {
  projectPath: string | null;
  collapsed: boolean;
  onOpenProject: (path: string) => void;
}) {
  const [error, setError] = useState<string | null>(null);

  async function handleClick() {
    const result = await pickAndOpenExistingProject();
    if (result.status === "cancelled") return;
    if (result.status === "invalid") {
      setError(`"${projectFolderName(result.path)}" isn't an InfinaBox project.`);
      return;
    }
    setError(null);
    onOpenProject(result.path);
  }

  const button = (
    <Button
      type="button"
      variant="ghost"
      size="sm"
      onClick={() => void handleClick()}
      className={cn(
        "w-full gap-2 px-2 font-normal text-muted-foreground hover:text-foreground",
        collapsed ? "justify-center" : "justify-start",
      )}
    >
      <FolderOpen className="size-4 shrink-0" />
      <FadeLabel show={!collapsed}>
        {projectPath ? projectFolderName(projectPath) : "Open Project"}
      </FadeLabel>
    </Button>
  );

  // Expanded: a tooltip only earns its keep once the label is already
  // truncated (the full path). Collapsed: there's no label at all, so the
  // tooltip is the only way to tell what this button does or which project
  // is open.
  const showTooltip = collapsed || Boolean(projectPath);

  return (
    <div className="flex flex-col gap-0.5">
      {showTooltip ? (
        <Tooltip>
          <TooltipTrigger asChild>{button}</TooltipTrigger>
          <TooltipContent side="right">{projectPath ?? "Open Project"}</TooltipContent>
        </Tooltip>
      ) : (
        button
      )}
      {error && !collapsed && <p className="px-2 text-[11px] text-destructive">{error}</p>}
    </div>
  );
}

export function Sidebar({ active, onSelect, projectPath, onOpenProject }: SidebarProps) {
  const [collapsed, setCollapsed] = useState(loadCollapsed);

  useEffect(() => {
    try {
      localStorage.setItem(COLLAPSED_STORAGE_KEY, String(collapsed));
    } catch {
      // Losing the preference just means it defaults back to expanded next
      // launch — not worth surfacing as an error.
    }
  }, [collapsed]);

  return (
    <motion.div
      // Collapsed width has a floor: the macOS traffic lights are drawn
      // by the OS at a fixed offset (see `trafficLightPosition` in
      // tauri.conf.json — x: 22px inset, and the cluster itself is
      // ~54px wide), independent of anything in the DOM. Narrower than
      // that and the lights spill onto whatever block sits to the right
      // instead of reading as "on top of the sidebar."
      animate={{ width: collapsed ? COLLAPSED_WIDTH : EXPANDED_WIDTH }}
      transition={widthTransition}
      className="relative flex h-full shrink-0 flex-col"
    >
      {/* The visual shell — full block height, same as every other panel
       * in the app, unchanged by the traffic lights. `data-tauri-drag-region`
       * lives directly on this element (matching the old top bar's own
       * pattern — the attribute has to be on the actual element you want
       * draggable, not just an ancestor) so the empty strip behind the
       * lights is real, working drag space, while the block still LOOKS
       * like an ordinary full-height card. */}
      <div
        data-tauri-drag-region
        className="absolute inset-0 rounded-xl border border-border bg-card"
      />

      {/* The real content — offset down past the traffic lights with
       * `pt-11` (their vertical clearance; matches the old top bar's
       * height). No `pointer-events-auto` needed here: `data-tauri-drag-region`
       * is purely a data attribute Tauri's JS layer reads on `pointerdown` —
       * it never touches the CSS pointer-events cascade, so there's nothing
       * to opt back into (see App.tsx's top comment). */}
      <div className="relative z-10 flex h-full min-h-0 flex-col gap-1 px-2 pt-11 pb-2">
        {PINNED.map((item) => (
          <SectionRow
            key={item.id}
            item={item}
            active={active === item.id}
            collapsed={collapsed}
            onSelect={() => onSelect(item.id)}
          />
        ))}
        <ProjectButtonRow projectPath={projectPath} collapsed={collapsed} onOpenProject={onOpenProject} />

        <Separator className="my-1" />

        <ScrollArea className="min-h-0 flex-1">
          <div className={cn("flex flex-col gap-0.5", !collapsed && "pr-2")}>
            {PROJECT_SECTIONS.map((item) => (
              <SectionRow
                key={item.id}
                item={item}
                active={active === item.id}
                collapsed={collapsed}
                onSelect={() => onSelect(item.id)}
              />
            ))}
          </div>
        </ScrollArea>

        <Separator className="my-1" />

        <SectionRow
          item={SETTINGS_ITEM}
          active={active === "settings"}
          collapsed={collapsed}
          onSelect={() => onSelect("settings")}
        />

        <AnimatePresence mode="wait" initial={false}>
          {collapsed ? (
            <motion.div
              key="expand"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={fadeTransition}
            >
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    onClick={() => setCollapsed(false)}
                    className="w-full justify-center px-2 font-normal text-muted-foreground hover:text-foreground"
                  >
                    <PanelLeftOpen className="size-4 shrink-0" />
                  </Button>
                </TooltipTrigger>
                <TooltipContent side="right">Expand sidebar</TooltipContent>
              </Tooltip>
            </motion.div>
          ) : (
            <motion.div
              key="collapse"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={fadeTransition}
            >
              <Button
                type="button"
                variant="ghost"
                size="sm"
                onClick={() => setCollapsed(true)}
                className="w-full justify-start gap-2 px-2 font-normal text-muted-foreground hover:text-foreground"
              >
                <PanelLeftClose className="size-4 shrink-0" />
                <span className="truncate">Collapse</span>
              </Button>
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    </motion.div>
  );
}
