import {
  Plus,
  Box,
  FolderOpen,
  PanelsTopLeft,
  FileText,
  ShieldCheck,
  Megaphone,
  Radio,
  ChevronDown,
} from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Separator } from "@/components/ui/separator";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

export type Section = "design" | "qa" | "business" | "liveops";

const SECTIONS: { id: Section; label: string; icon: typeof FileText }[] = [
  { id: "design", label: "Design", icon: FileText },
  { id: "qa", label: "QA", icon: ShieldCheck },
  { id: "business", label: "Business", icon: Megaphone },
  { id: "liveops", label: "Live Ops", icon: Radio },
];

interface TopBarProps {
  projectPath: string | null;
  onOpenProject: (path: string) => void;
  activeSection: Section | null;
  onSelectSection: (section: Section | null) => void;
}

function folderName(path: string): string {
  return path.split("/").filter(Boolean).pop() ?? path;
}

export function TopBar({
  projectPath,
  onOpenProject,
  activeSection,
  onSelectSection,
}: TopBarProps) {
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

        <Separator orientation="vertical" className="h-4" />

        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button size="sm" variant={activeSection ? "secondary" : "ghost"}>
              <PanelsTopLeft />
              Project
              <ChevronDown className="size-3.5 text-muted-foreground" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start">
            <DropdownMenuItem onSelect={() => onSelectSection(null)}>
              <Box className="size-3.5" />
              Build
              {activeSection === null && (
                <span className="ml-auto size-1.5 rounded-full bg-foreground" />
              )}
            </DropdownMenuItem>
            {SECTIONS.map((s) => (
              <DropdownMenuItem key={s.id} onSelect={() => onSelectSection(s.id)}>
                <s.icon className="size-3.5" />
                {s.label}
                {activeSection === s.id && (
                  <span className="ml-auto size-1.5 rounded-full bg-foreground" />
                )}
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      </div>

      <div className="flex items-center gap-2">
        <Badge variant="outline">3 agents</Badge>
        <Button size="sm">
          <Plus />
          New task
        </Button>
      </div>
    </header>
  );
}
