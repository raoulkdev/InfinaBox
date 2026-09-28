import { AnimatePresence, motion } from "motion/react";
import {
  AlertCircle,
  CheckCircle2,
  Download,
  Loader2,
  LogIn,
  MessageCircle,
  Square,
  X,
} from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { ExternalLink } from "@/components/studio/chat/ExternalLink";
import { fadeRise, fadeTransition } from "@/lib/motion";
import type {
  ConnectAction,
  ConnectExitPayload,
  ConnectionTest,
  ProviderInfo,
} from "@/lib/studio-types";
import { ConnectTerminal } from "./ConnectTerminal";
import {
  formatDuration,
  isUsable,
  runOutcome,
  signedInLabel,
  testErrorHeading,
} from "./connect-format";

/** One install/sign-in run, from "here's the command" to its real end. */
export type RunState =
  | { stage: "confirm"; action: ConnectAction }
  | { stage: "running"; action: ConnectAction; key: number; cancelling: boolean }
  | {
      stage: "finished";
      action: ConnectAction;
      key: number;
      exit: ConnectExitPayload | null;
      /** `connect_run` refused to start. */
      startError: string | null;
    };

export type TestState =
  | { status: "running" }
  | { status: "done"; result: ConnectionTest }
  | { status: "failed"; error: string };

interface ProviderCardProps {
  provider: ProviderInfo;
  recommended: boolean;
  inUse: boolean;
  /** Show "Use this one" as the main action (the one ready provider when
   * nothing's chosen yet). */
  highlightUse: boolean;
  run: RunState | null;
  /** Another provider's install/sign-in is on screen — only one runs at a time. */
  otherRunActive: boolean;
  test: TestState | null;
  choosing: boolean;
  onInstallClick: () => void;
  onConfirmInstall: () => void;
  onSignIn: () => void;
  onCancelRun: () => void;
  onCloseRun: () => void;
  onRunExit: (exit: ConnectExitPayload) => void;
  onRunStartError: (message: string) => void;
  onTest: () => void;
  onUse: () => void;
}

export function ProviderCard({
  provider: p,
  recommended,
  inUse,
  highlightUse,
  run,
  otherRunActive,
  test,
  choosing,
  onInstallClick,
  onConfirmInstall,
  onSignIn,
  onCancelRun,
  onCloseRun,
  onRunExit,
  onRunStartError,
  onTest,
  onUse,
}: ProviderCardProps) {
  const runBusy = run !== null && run.stage === "running";
  const actionsLocked = otherRunActive || runBusy;
  const testing = test?.status === "running";

  return (
    <div
      data-testid={`provider-${p.id}`}
      data-installed={p.installed}
      data-signed-in={p.logged_in === null ? "unknown" : String(p.logged_in)}
      className={`flex flex-col gap-3 rounded-lg border bg-background p-3 ${
        inUse ? "border-emerald-500/50" : "border-border"
      } ${run && run.stage !== "confirm" ? "md:col-span-2" : ""}`}
    >
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-sm font-medium text-foreground/90">{p.name}</span>
        {recommended && <Badge variant="secondary">Recommended</Badge>}
        {inUse && (
          <Badge variant="outline" className="border-emerald-500/40 text-emerald-400">
            <CheckCircle2 />
            In use
          </Badge>
        )}
      </div>
      <p className="text-sm text-muted-foreground">{p.blurb}</p>

      <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-xs">
        <dt className="text-muted-foreground">Installed</dt>
        <dd className="min-w-0 truncate text-foreground/90" title={p.version ?? undefined}>
          {p.installed ? (p.version ? `Yes · ${p.version}` : "Yes") : "No"}
        </dd>
        {p.installed && (
          <>
            <dt className="text-muted-foreground">Signed in</dt>
            <dd className="text-foreground/90">{signedInLabel(p.logged_in)}</dd>
          </>
        )}
      </dl>

      {!p.installed && p.install_blocker && (
        <Alert>
          <AlertCircle />
          <AlertTitle>InfinaBox can't install this one for you yet</AlertTitle>
          <AlertDescription className="break-words">
            <p>{p.install_blocker}</p>
            <ExternalLink url={p.docs_url}>How to install it yourself</ExternalLink>
          </AlertDescription>
        </Alert>
      )}

      <div className="flex flex-wrap items-center gap-2">
        {!p.installed && p.install_command && !p.install_blocker && (
          <Button
            type="button"
            size="sm"
            data-testid={`provider-install-${p.id}`}
            disabled={actionsLocked || run?.stage === "confirm"}
            onClick={onInstallClick}
          >
            <Download data-icon="inline-start" />
            Install
          </Button>
        )}
        {p.installed && p.logged_in !== true && (
          <Button
            type="button"
            size="sm"
            variant={p.logged_in === false ? "default" : "secondary"}
            data-testid={`provider-signin-${p.id}`}
            disabled={actionsLocked}
            onClick={onSignIn}
          >
            <LogIn data-icon="inline-start" />
            Sign in
          </Button>
        )}
        {p.installed && (
          <Button
            type="button"
            size="sm"
            variant="secondary"
            data-testid={`provider-test-${p.id}`}
            disabled={testing || runBusy}
            onClick={onTest}
          >
            {testing ? (
              <Loader2 data-icon="inline-start" className="animate-spin" />
            ) : (
              <MessageCircle data-icon="inline-start" />
            )}
            {testing ? "Saying hello…" : "Say hello"}
          </Button>
        )}
        {isUsable(p) && !inUse && (
          <Button
            type="button"
            size="sm"
            variant={highlightUse ? "default" : "outline"}
            data-testid={`provider-use-${p.id}`}
            disabled={choosing || runBusy}
            onClick={onUse}
          >
            {choosing ? "Saving…" : "Use this one"}
          </Button>
        )}
        {p.installed && p.logged_in === false && (
          <span className="text-xs text-muted-foreground">Sign in to use it.</span>
        )}
      </div>

      <AnimatePresence initial={false}>
        {run && (
          <motion.div
            key="run"
            {...fadeRise}
            transition={fadeTransition}
            className="flex flex-col gap-2"
          >
            <RunView
              provider={p}
              run={run}
              onConfirmInstall={onConfirmInstall}
              onCancelRun={onCancelRun}
              onCloseRun={onCloseRun}
              onRunExit={onRunExit}
              onRunStartError={onRunStartError}
            />
          </motion.div>
        )}
      </AnimatePresence>

      <AnimatePresence initial={false}>
        {test && test.status !== "running" && (
          <motion.div key="test" {...fadeRise} transition={fadeTransition}>
            <TestResultView test={test} />
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

function RunView({
  provider: p,
  run,
  onConfirmInstall,
  onCancelRun,
  onCloseRun,
  onRunExit,
  onRunStartError,
}: {
  provider: ProviderInfo;
  run: RunState;
  onConfirmInstall: () => void;
  onCancelRun: () => void;
  onCloseRun: () => void;
  onRunExit: (exit: ConnectExitPayload) => void;
  onRunStartError: (message: string) => void;
}) {
  if (run.stage === "confirm") {
    return (
      <div className="flex flex-col gap-2 rounded-lg border border-border p-3">
        <span className="text-sm">InfinaBox will run {p.name}'s own installer in a terminal here:</span>
        <code
          data-testid="connect-install-command"
          className="rounded-md bg-muted px-2 py-1.5 font-mono text-xs break-all text-foreground/90 select-all"
        >
          {p.install_command}
        </code>
        <span className="text-xs text-muted-foreground">
          You'll see everything it does. It can take a few minutes.
        </span>
        <div className="flex gap-2">
          <Button type="button" size="sm" data-testid="connect-install-confirm" onClick={onConfirmInstall}>
            Run the installer
          </Button>
          <Button type="button" size="sm" variant="ghost" onClick={onCloseRun}>
            Not now
          </Button>
        </div>
      </div>
    );
  }

  const running = run.stage === "running";
  return (
    <>
      <div className="flex items-center justify-between gap-2">
        <span className="flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground">
          {running && <Loader2 className="size-3 shrink-0 animate-spin" />}
          <span>
            {run.action === "install"
              ? running
                ? `Installing ${p.name}… Keep this screen open until it finishes.`
                : `Installing ${p.name}`
              : running
                ? "Signing in… A browser window may open — sign in there, then come back here."
                : "Signing in"}
          </span>
        </span>
        {running ? (
          <Button
            type="button"
            size="xs"
            variant="ghost"
            data-testid="connect-cancel"
            disabled={run.cancelling}
            onClick={onCancelRun}
          >
            <Square data-icon="inline-start" />
            {run.cancelling ? "Stopping…" : "Cancel"}
          </Button>
        ) : (
          <Button type="button" size="xs" variant="ghost" onClick={onCloseRun}>
            <X data-icon="inline-start" />
            Close
          </Button>
        )}
      </div>
      {run.action === "login" && (
        <span className="font-mono text-xs text-muted-foreground">{p.login_command}</span>
      )}
      {/* `key` per run: a new run gets a fresh terminal (and a fresh
          `connect_run`), while the finished one stays readable. */}
      <ConnectTerminal
        key={run.key}
        provider={p.id}
        action={run.action}
        onExit={onRunExit}
        onStartError={onRunStartError}
      />
      {run.stage === "finished" && (
        <Alert
          variant={run.exit?.success ? "default" : "destructive"}
          data-testid="connect-run-result"
          data-success={run.exit?.success ?? false}
        >
          {run.exit?.success ? <CheckCircle2 /> : <AlertCircle />}
          <AlertDescription className="break-words">
            {run.startError
              ? `Couldn't start: ${run.startError}`
              : run.exit
                ? runOutcome(run.action, p.name, run.exit.success, run.exit.code)
                : null}
          </AlertDescription>
        </Alert>
      )}
    </>
  );
}

function TestResultView({ test }: { test: Exclude<TestState, { status: "running" }> }) {
  if (test.status === "failed") {
    return (
      <Alert variant="destructive" data-testid="connect-test-result" data-ok="false">
        <AlertCircle />
        <AlertTitle>Couldn't run the test</AlertTitle>
        <AlertDescription className="break-words">{test.error}</AlertDescription>
      </Alert>
    );
  }
  const r = test.result;
  if (r.ok) {
    return (
      <Alert data-testid="connect-test-result" data-ok="true">
        <CheckCircle2 className="text-emerald-400" />
        <AlertTitle>It works — replied in {formatDuration(r.duration_ms)}</AlertTitle>
        {r.reply !== null && (
          <AlertDescription className="break-words">“{r.reply}”</AlertDescription>
        )}
      </Alert>
    );
  }
  return (
    <Alert variant="destructive" data-testid="connect-test-result" data-ok="false">
      <AlertCircle />
      <AlertTitle>{testErrorHeading(r.error_kind)}</AlertTitle>
      <AlertDescription className="break-words">
        {r.message ?? "The test didn't get a reply."}
      </AlertDescription>
    </Alert>
  );
}
