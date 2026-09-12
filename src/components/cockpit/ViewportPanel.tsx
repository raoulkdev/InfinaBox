import { Box } from "lucide-react";

/** The persistent "Build" surface — always underneath whatever Project-menu
 * section is active. Honest placeholder: no fabricated activity feed. Real
 * build/engine activity is wired in a later milestone. */
export function ViewportPanel() {
  return (
    <div className="flex h-full min-w-0 flex-1 flex-col border border-border bg-card">
      <div className="flex h-8 shrink-0 items-center border-b border-border px-3">
        <span className="truncate text-sm text-foreground/90">
          project.godot
        </span>
      </div>
      <div className="flex flex-1 flex-col items-center justify-center gap-2 text-center">
        <div className="flex size-10 items-center justify-center border border-border">
          <Box className="size-5 text-muted-foreground" />
        </div>
        <h2 className="text-lg font-medium tracking-tight">Project</h2>
        <p className="max-w-xs text-sm text-muted-foreground">
          Build activity, previews, and diagnostics for the open project will
          appear here once the Godot handoff is wired up.
        </p>
      </div>
    </div>
  );
}
