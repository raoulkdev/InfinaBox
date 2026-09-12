import { Megaphone } from "lucide-react";

export function BusinessSection() {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-2 text-center">
      <div className="flex size-10 items-center justify-center border border-border">
        <Megaphone className="size-5 text-muted-foreground" />
      </div>
      <h2 className="text-lg font-medium tracking-tight">Not built yet</h2>
      <p className="max-w-xs text-sm text-muted-foreground">
        Store-page readiness, marketing beats, and community sentiment arrive
        in a later phase, wired to real Steamworks/Discord data.
      </p>
    </div>
  );
}
