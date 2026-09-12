import { ScrollArea } from "@/components/ui/scroll-area";

export function ConsoleDock() {
  return (
    <div className="flex h-[110px] w-full shrink-0 flex-col border border-border bg-card">
      <div className="flex h-8 shrink-0 items-center border-b border-border px-3">
        <span className="text-xs font-medium tracking-wide text-muted-foreground uppercase">
          Console
        </span>
      </div>
      <ScrollArea className="min-h-0 flex-1">
        <div className="p-2 font-mono text-xs text-muted-foreground">
          <p>InfinaBox started</p>
        </div>
      </ScrollArea>
    </div>
  );
}
