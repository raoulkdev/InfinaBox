// Studio's Play panel: Godot install, run/stop, output, and errors (Phase A
// Task F2 fills this in).

export interface PlayPanelProps {
  projectPath: string;
  /** Sends a message to the chat (wired to ChatPanel's registered send). */
  onAskAiToFix: (message: string) => void;
}

export function PlayPanel(_props: PlayPanelProps) {
  return null;
}
