import { Radio } from "lucide-react";

export function LiveOpsSection() {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-2 text-center">
      <div className="flex size-10 items-center justify-center border border-border">
        <Radio className="size-5 text-muted-foreground" />
      </div>
      <h2 className="text-lg font-medium tracking-tight">Dormant pre-launch</h2>
      <p className="max-w-xs text-sm text-muted-foreground">
        Analytics, crash triage, and live events connect once there's a
        shipped game generating real data — and the underlying integrations
        aren't built yet either way.
      </p>
    </div>
  );
}
