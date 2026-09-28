import type { ProviderId } from "@/lib/studio-types";

// Wave 0 stub (Phase B plan, Task FH): detect, install, sign in, test and
// choose the user's own AI (Claude Code or Codex).

export interface ConnectAiPanelProps {
  onConnected?: (provider: ProviderId) => void;
}

export function ConnectAiPanel(_props: ConnectAiPanelProps) {
  return <div className="p-6 text-sm text-muted-foreground">Connecting your AI isn't built yet.</div>;
}
