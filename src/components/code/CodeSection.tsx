import { BuildRow } from "@/components/cockpit/BuildRow";

// The Code page: the project's code and a real terminal on one page (the
// pre-Studio Build workspace, unchanged), for people who want to look under
// the hood. App settings (which AI, which Godot, accounts) live on the
// Settings page. It's one of App.tsx's
// permanently mounted sections: the terminal's shell session must survive
// navigating away and back.

export interface CodeSectionProps {
  projectPath: string | null;
}

export function CodeSection({ projectPath }: CodeSectionProps) {
  return (
    <div className="flex h-full min-h-0 min-w-0 flex-1 flex-col gap-2">
      <div
        data-tauri-drag-region
        className="flex shrink-0 items-center gap-3 rounded-xl border border-border bg-card px-3 py-2"
      >
        <div data-tauri-drag-region className="flex min-w-0 flex-1 flex-col">
          <span className="text-xs font-medium tracking-wide text-muted-foreground">Code</span>
        </div>
      </div>
      <div className="flex min-h-0 min-w-0 flex-1 overflow-hidden">
        {/* TerminalPanel spawns its shell once, on mount, using whatever
            projectPath it was given at that instant (see its own comment
            on why it never re-cwds later) — so it must not mount at all
            until a real project is chosen, or it'd spawn in the user's home
            directory instead. */}
        <BuildRow projectPath={projectPath} showTerminal={projectPath !== null} />
      </div>
    </div>
  );
}
