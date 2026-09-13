import { Plus, Box, FolderOpen } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";

interface TopBarProps {
  projectPath: string | null;
  onOpenProject: (path: string) => void;
}

function folderName(path: string): string {
  return path.split("/").filter(Boolean).pop() ?? path;
}

export function TopBar({ projectPath, onOpenProject }: TopBarProps) {
  async function handleOpenProject() {
    // `open` resolves to `null` when the user cancels the dialog — that's
    // a normal outcome, not an error, so there's nothing to catch/report.
    const folder = await open({ directory: true, multiple: false });
    if (typeof folder === "string") {
      onOpenProject(folder);
    }
  }

  return (
    <header className="flex h-12 shrink-0 items-center justify-between border border-border bg-card px-3">
      <div className="flex items-center gap-3">
        <div className="flex size-6 items-center justify-center border border-border bg-accent">
          <Box className="size-3.5" />
        </div>
        <span className="text-sm font-medium tracking-tight">InfinaBox</span>
        <Separator orientation="vertical" className="h-4" />
        <Button
          size="sm"
          variant="outline"
          onClick={() => void handleOpenProject()}
          title={projectPath ?? undefined}
        >
          <FolderOpen />
          {projectPath ? folderName(projectPath) : "Open Project"}
        </Button>
      </div>
      <div className="flex items-center gap-2">
        <Button size="sm">
          <Plus />
          New task
        </Button>
      </div>
    </header>
  );
}
