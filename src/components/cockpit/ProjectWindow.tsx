import { OverviewTab } from "@/components/cockpit/OverviewTab";
import { ChangesTab } from "@/components/cockpit/ChangesTab";

interface ProjectWindowProps {
  projectPath: string | null;
}

export function ProjectWindow({ projectPath }: ProjectWindowProps) {
  return (
    <div className="flex h-full min-w-0 flex-1 flex-col gap-2 overflow-hidden">
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-xl border border-border bg-card">
        <div data-tauri-drag-region className="flex h-9 shrink-0 items-center px-3">
          <span className="text-xs font-medium tracking-wide text-muted-foreground">
            Overview
          </span>
        </div>
        <OverviewTab projectPath={projectPath} />
      </div>
      <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-xl border border-border bg-card">
        <div data-tauri-drag-region className="flex h-9 shrink-0 items-center px-3">
          <span className="text-xs font-medium tracking-wide text-muted-foreground">Changes</span>
        </div>
        <ChangesTab projectPath={projectPath} />
      </div>
    </div>
  );
}
