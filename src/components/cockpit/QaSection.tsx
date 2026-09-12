import { ShieldCheck } from "lucide-react";

export function QaSection() {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-2 text-center">
      <div className="flex size-10 items-center justify-center border border-border">
        <ShieldCheck className="size-5 text-muted-foreground" />
      </div>
      <h2 className="text-lg font-medium tracking-tight">Not built yet</h2>
      <p className="max-w-xs text-sm text-muted-foreground">
        Bug tracking and the automated playtest runner arrive in a later
        phase, once the core agent loop is proven.
      </p>
    </div>
  );
}
