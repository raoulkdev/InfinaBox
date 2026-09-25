// Studio's chat with the user's own AI (Phase A Task F1 fills this in).

export interface ChatPanelProps {
  projectPath: string;
  /** Called once on mount with a function that sends `text` as a user
   * message in the active thread — how the Play panel's "Ask AI to fix"
   * reaches the chat without the two panels sharing state. */
  onRegisterSend: (send: (text: string) => void) => void;
}

export function ChatPanel(_props: ChatPanelProps) {
  return null;
}
