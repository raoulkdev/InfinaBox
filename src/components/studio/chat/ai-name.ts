// The name to use for the person's AI in chat copy. Threads record which
// provider they talk to (`ThreadSummary.provider`, e.g. "claude-code"), and
// `agent_status` reports the selected CLI's program name (e.g. "claude") —
// both map to the product name the person knows. Anything unrecognised gets
// neutral wording rather than a guess.

export function aiDisplayName(provider: string | null | undefined): string {
  const id = provider?.trim().toLowerCase() ?? "";
  if (id === "claude-code" || id === "claude") return "Claude Code";
  if (id === "codex") return "Codex";
  return "Your AI";
}

/** `aiDisplayName` for use mid-sentence ("Sign in to your AI first"). */
export function aiNameInSentence(provider: string | null | undefined): string {
  const name = aiDisplayName(provider);
  return name === "Your AI" ? "your AI" : name;
}
