import { Plus, Box } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Separator } from "@/components/ui/separator";

export function TopBar() {
  return (
    <header className="flex h-12 shrink-0 items-center justify-between border border-border bg-card px-3">
      <div className="flex items-center gap-3">
        <div className="flex size-6 items-center justify-center border border-border bg-accent">
          <Box className="size-3.5" />
        </div>
        <span className="text-sm font-medium tracking-tight">InfinaBox</span>
        <Separator orientation="vertical" className="h-4" />
        <span className="text-sm text-muted-foreground">Project</span>
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
