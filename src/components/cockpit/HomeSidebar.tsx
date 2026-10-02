import { useContextMenu } from "@/lib/context-menu";
import { Gamepad2, Settings as SettingsIcon, type LucideIcon } from "lucide-react";
import { motion } from "motion/react";
import { Button } from "@/components/ui/button";
import { springTransition } from "@/lib/motion";
import { cn } from "@/lib/utils";

// Home is its own place, apart from any open game, and has its own sidebar:
// your games, and InfinaBox's settings. Opening a game switches to the
// game's sidebar (`Sidebar`), whose first entry comes back here.

export type HomePage = "home" | "settings";

const ITEMS: { id: HomePage; label: string; icon: LucideIcon }[] = [
  { id: "home", label: "Your games", icon: Gamepad2 },
  { id: "settings", label: "Settings", icon: SettingsIcon },
];

export function HomeSidebar({ active, onSelect }: { active: HomePage; onSelect: (page: HomePage) => void }) {
  const menu = useContextMenu();
  return (
    <div
      className="relative flex h-full w-[212px] shrink-0 flex-col"
      data-testid="home-sidebar"
      onContextMenu={(e) => menu(e, ITEMS.map((i) => ({ label: i.label, icon: <i.icon />, disabled: i.id === active, onSelect: () => onSelect(i.id) })))}
    >
      {/* Same shell as the game sidebar: a full-height card whose empty
       * strip behind the macOS traffic lights is real drag space. */}
      <div data-tauri-drag-region className="absolute inset-0 rounded-xl border border-border bg-card" />
      <div
        data-tauri-drag-region
        className="relative z-10 flex h-full min-h-0 flex-col gap-1 px-2 pt-11 pb-2"
      >
        {ITEMS.map((item) => {
          const Icon = item.icon;
          const isActive = active === item.id;
          return (
            <Button
              key={item.id}
              type="button"
              variant="ghost"
              size="sm"
              aria-pressed={isActive}
              data-testid={`home-nav-${item.id}`}
              onClick={() => onSelect(item.id)}
              className={cn(
                "relative w-full justify-start gap-2 px-2 font-normal text-muted-foreground hover:text-foreground",
                isActive && "text-foreground",
              )}
            >
              {isActive && (
                <motion.div
                  layoutId="home-sidebar-active-indicator"
                  className="absolute inset-0 rounded-md bg-accent"
                  transition={springTransition}
                />
              )}
              <Icon className="relative z-10 size-4 shrink-0" />
              <span className="relative z-10 truncate">{item.label}</span>
            </Button>
          );
        })}
      </div>
    </div>
  );
}
