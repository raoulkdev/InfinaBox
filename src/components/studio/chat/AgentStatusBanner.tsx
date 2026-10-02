import { useCallback, useEffect, useState, type ReactNode } from "react";
import { AnimatePresence, motion } from "motion/react";
import { AlertCircle, Clock, Download, KeyRound, RefreshCw } from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { fadeRise, fadeTransition } from "@/lib/motion";
import { agentStatus } from "@/lib/studio-api";
import type { RuntimeStatus } from "@/lib/studio-types";
import { aiDisplayName, aiNameInSentence } from "./ai-name";
import type { TurnError } from "./chat-reducer";
import { ConnectHint } from "./ConnectHint";

type StatusState =
  | { status: "loading" }
  | { status: "ready"; runtime: RuntimeStatus }
  | { status: "error"; message: string };

interface AgentStatusBannerProps {
  /** The latest turn's error, if any — rate-limit problems (and sign-in
   * problems the CLI's own status can't see) only show up when a turn
   * actually runs. */
  lastTurnError: TurnError | null;
}

// The one place the chat explains why the AI can't answer right now, in
// plain language, with the next step the user can actually take. Renders
// nothing when everything is fine. Installing and signing in happen in
// Home's "Connect your AI" panel, so that's where it points.
export function AgentStatusBanner({ lastTurnError }: AgentStatusBannerProps) {
  const [state, setState] = useState<StatusState>({ status: "loading" });
  const [checking, setChecking] = useState(false);

  const check = useCallback(async () => {
    setChecking(true);
    try {
      setState({ status: "ready", runtime: await agentStatus() });
    } catch (err) {
      setState({ status: "error", message: String(err) });
    } finally {
      setChecking(false);
    }
  }, []);

  useEffect(() => {
    void check();
  }, [check]);

  // A turn that failed because the CLI went missing or signed out is worth
  // re-checking right away, so the matching notice shows.
  useEffect(() => {
    if (lastTurnError?.kind === "not_installed" || lastTurnError?.kind === "not_authenticated") void check();
  }, [lastTurnError, check]);

  const content = renderContent(state, lastTurnError, checking, () => void check());

  return (
    <AnimatePresence initial={false}>
      {content && (
        <motion.div key={content.key} {...fadeRise} transition={fadeTransition}>
          {content.node}
        </motion.div>
      )}
    </AnimatePresence>
  );
}

function renderContent(
  state: StatusState,
  lastTurnError: TurnError | null,
  checking: boolean,
  onCheckAgain: () => void,
): { key: string; node: ReactNode } | null {
  const checkAgain = (
    <Button type="button" size="xs" variant="outline" onClick={onCheckAgain} disabled={checking}>
      <RefreshCw className={checking ? "animate-spin" : undefined} />
      Check again
    </Button>
  );
  // The selected AI's program name, when the status check got that far.
  const provider = state.status === "ready" ? state.runtime.name : null;

  if (state.status === "error") {
    return {
      key: "status-error",
      node: (
        <Alert variant="destructive">
          <AlertCircle />
          <AlertTitle>Couldn't check on your AI</AlertTitle>
          <AlertDescription className="flex flex-col items-start gap-2">
            <span className="font-mono text-xs break-all">{state.message}</span>
            {checkAgain}
          </AlertDescription>
        </Alert>
      ),
    };
  }

  // Decided only by the latest real `agent_status` result. A turn's
  // `not_installed` error just triggers a fresh check (see above), so once
  // the user installs and presses "Check again", this goes away.
  if (state.status === "ready" && !state.runtime.installed) {
    return {
      key: "not-installed",
      node: (
        <Alert>
          <Download />
          <AlertTitle>{aiDisplayName(provider)} isn't installed on this computer</AlertTitle>
          <AlertDescription className="flex flex-col items-start gap-2">
            <span>
              InfinaBox works through the AI app you already use.{" "}
              <ConnectHint what="set up" provider={provider} />
            </span>
            {checkAgain}
          </AlertDescription>
        </Alert>
      ),
    };
  }

  // Signed out, by the CLI's own status command (`logged_in` is null when
  // it can't tell — then only a turn's real error says so).
  const signedOut =
    (state.status === "ready" && state.runtime.logged_in === false) || lastTurnError?.kind === "not_authenticated";
  if (signedOut) {
    return {
      key: "not-authenticated",
      node: (
        <Alert>
          <KeyRound />
          <AlertTitle>Sign in to {aiNameInSentence(provider)} first</AlertTitle>
          <AlertDescription className="flex flex-col items-start gap-2">
            <span>
              It's installed but not signed in. <ConnectHint what="sign in to" provider={provider} /> Then send your
              message again.
            </span>
            {checkAgain}
          </AlertDescription>
        </Alert>
      ),
    };
  }

  if (lastTurnError?.kind === "rate_limited") {
    return {
      key: "rate-limited",
      node: (
        <Alert>
          <Clock />
          <AlertTitle>You've reached your plan's limit for now</AlertTitle>
          <AlertDescription>
            {/* The provider's own words — they say when the limit resets. */}
            <span className="break-words">{lastTurnError.message}</span>
          </AlertDescription>
        </Alert>
      ),
    };
  }

  return null;
}
