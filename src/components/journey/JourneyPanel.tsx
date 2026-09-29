// Wave 0 stub (Phase C plan, Task FP): the Producer's journey from Idea to
// Launch, with real checklists, a next step, and "Ask the Producer".

export interface JourneyPanelProps {
  projectPath: string;
  /** Sends `message` to the chat as the Producer role. */
  onAskProducer: (message: string) => void;
}

export function JourneyPanel(_props: JourneyPanelProps) {
  return <div className="p-6 text-sm text-muted-foreground">The journey isn't built yet.</div>;
}
