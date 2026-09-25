import { useCallback, useEffect, useState, type ReactNode } from "react";
import { AnimatePresence, motion } from "motion/react";
import { AlertCircle, Check, Clock, Copy, Download, KeyRound, RefreshCw } from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { fadeRise, fadeTransition } from "@/lib/motion";
import { agentStatus } from "@/lib/studio-api";
import type { RuntimeStatus } from "@/lib/studio-types";
import type { TurnError } from "./chat-reducer";
import { ExternalLink } from "./ExternalLink";

// Anthropic's official installer commands and setup docs for Claude Code.
// Shown verbatim for the user to run themselves — Phase A never installs
// anything on their behalf (the guided install/sign-in flow is Phase B).
const DOCS_URL = "https://code.claude.com/docs/en/setup";
const IS_WINDOWS = typeof navigator !== "undefined" && /windows/i.test(navigator.userAgent);
const INSTALL_COMMAND = IS_WINDOWS
  ? "irm https://claude.ai/install.ps1 | iex"
  : "curl -fsSL https://claude.ai/install.sh | bash";
const INSTALL_SHELL = IS_WINDOWS ? "PowerShell" : "Terminal";

type StatusState =
  | { status: "loading" }
  | { status: "ready"; runtime: RuntimeStatus }
  | { status: "error"; message: string };

interface AgentStatusBannerProps {
  /** The latest turn's error, if any — sign-in and rate-limit problems only
   * show up when a turn actually runs, not from `agent_status`. */
  lastTurnError: TurnError | null;
}

// The one place the chat explains why the AI can't answer right now, in
// plain language, with the next step the user can actually take. Renders
// nothing when everything is fine.
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

  // A turn that failed because the CLI went missing (uninstalled since the
  // last check) is worth re-checking right away, so the install notice shows.
  useEffect(() => {
    if (lastTurnError?.kind === "not_installed") void check();
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

  if (state.status === "error") {
    return {
      key: "status-error",
      node: (
        <Alert variant="destructive">
          <AlertCircle />
          <AlertTitle>Couldn't check for Claude Code</AlertTitle>
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
          <AlertTitle>Claude Code isn't installed on this computer</AlertTitle>
          <AlertDescription className="flex flex-col items-start gap-2">
            <span>
              InfinaBox uses your own Claude account through the Claude Code app. To install it, open{" "}
              {INSTALL_SHELL} and run:
            </span>
            <CopyableCommand command={INSTALL_COMMAND} />
            <div className="flex items-center gap-2">
              {checkAgain}
              <ExternalLink url={DOCS_URL} className="px-2 text-xs text-foreground underline-offset-3 hover:underline">
                Installation help
              </ExternalLink>
            </div>
          </AlertDescription>
        </Alert>
      ),
    };
  }

  if (lastTurnError?.kind === "not_authenticated") {
    return {
      key: "not-authenticated",
      node: (
        <Alert>
          <KeyRound />
          <AlertTitle>Sign in to Claude Code first</AlertTitle>
          <AlertDescription>
            Claude Code is installed but not signed in. Open Advanced → Terminal, type{" "}
            <code className="rounded bg-muted px-1 py-0.5 font-mono text-xs">claude</code> and press Enter, then
            follow the sign-in steps. Once you're signed in, send your message again.
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
          <AlertTitle>You've reached your Claude plan's limit for now</AlertTitle>
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

function CopyableCommand({ command }: { command: string }) {
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!copied) return;
    const timer = window.setTimeout(() => setCopied(false), 1500);
    return () => window.clearTimeout(timer);
  }, [copied]);

  return (
    <div className="flex w-full items-center gap-1 rounded-md border border-border bg-background py-1 pr-1 pl-2">
      <code className="min-w-0 flex-1 truncate font-mono text-xs text-foreground" title={command}>
        {command}
      </code>
      <Button
        type="button"
        size="icon-xs"
        variant="ghost"
        aria-label="Copy command"
        onClick={() =>
          void navigator.clipboard.writeText(command).then(
            () => setCopied(true),
            () => undefined,
          )
        }
      >
        {copied ? <Check /> : <Copy />}
      </Button>
    </div>
  );
}
