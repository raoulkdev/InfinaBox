import type { LucideIcon } from "lucide-react";

interface NotBuiltYetSectionProps {
  icon: LucideIcon;
  description: string;
}

export function NotBuiltYetSection({ icon: Icon, description }: NotBuiltYetSectionProps) {
  return (
    <div className="flex h-full min-w-0 flex-1 flex-col items-center justify-center gap-2 rounded-xl border border-border bg-card py-8 text-center">
      <div className="flex size-10 items-center justify-center rounded-lg border border-border">
        <Icon className="size-5 text-muted-foreground" />
      </div>
      <h2 className="text-lg font-medium tracking-tight">Not built yet</h2>
      <p className="max-w-xs text-sm text-muted-foreground">{description}</p>
    </div>
  );
}
