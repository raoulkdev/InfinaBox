import { aiNameInSentence } from "./ai-name";

/** Where to fix a missing or signed-out AI: Home's "Connect your AI" panel,
 * which installs and signs in for the person. Plain text rather than a
 * button — Studio has no way to switch sections itself. */
export function ConnectHint({ what, provider }: { what: "set up" | "sign in to"; provider: string | null }) {
  return (
    <span className="text-muted-foreground">
      Go to{" "}
      <span className="font-medium text-foreground underline decoration-foreground/30 underline-offset-3">
        Home → Connect your AI
      </span>{" "}
      to {what} {aiNameInSentence(provider)}.
    </span>
  );
}
