import { useCallback, useEffect, useRef, useState } from "react";
import { AlertCircle, RefreshCw } from "lucide-react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import {
  aiProviders,
  aiRecommended,
  aiTestConnection,
  appSettingsGet,
  appSettingsSet,
  connectCancel,
} from "@/lib/studio-api";
import type {
  AppSettings,
  ConnectExitPayload,
  ProviderId,
  ProviderInfo,
} from "@/lib/studio-types";
import { ApiProviderCard } from "./ApiProviderCard";
import { API_PROVIDERS } from "./api-providers";
import { isApiStyle, isUsable } from "./connect-format";
import { ProviderCard, type RunState, type TestState } from "./ProviderCard";

export interface ConnectAiPanelProps {
  /** Called once a provider is chosen ("Use this one"), and again when the
   * chosen one finishes signing in — so a parent showing "connected?" can
   * re-check. */
  onConnected?: (provider: ProviderId) => void;
}

function errorText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/** Spec §8.2's connect flow: detect each supported AI (installed, version,
 * signed in — all from the backend's real checks), recommend one, install
 * or sign in through the provider's own commands in a visible terminal,
 * "say hello" with a real one-line turn, and choose the one to use.
 * Nothing here is filled in when a call fails: the error is shown as the
 * backend sent it. */
export function ConnectAiPanel({ onConnected }: ConnectAiPanelProps) {
  const [providers, setProviders] = useState<ProviderInfo[] | null>(null);
  const [detectError, setDetectError] = useState<string | null>(null);
  const [detecting, setDetecting] = useState(false);
  const [recommended, setRecommended] = useState<ProviderId | null>(null);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [settingsError, setSettingsError] = useState<string | null>(null);

  // One install/sign-in at a time: the backend runs a single connect PTY.
  const [run, setRun] = useState<{ provider: ProviderId; state: RunState } | null>(null);
  const runKeyRef = useRef(0);
  const [tests, setTests] = useState<Partial<Record<ProviderId, TestState>>>({});
  const [choosing, setChoosing] = useState<ProviderId | null>(null);
  const [chooseError, setChooseError] = useState<string | null>(null);

  // Bumps on every check so the key cards ask for their saved-key status again.
  const [checkToken, setCheckToken] = useState(0);

  const detect = useCallback(async () => {
    setDetecting(true);
    setCheckToken((n) => n + 1);
    const [infos, rec, current] = await Promise.allSettled([
      aiProviders(),
      aiRecommended(),
      appSettingsGet(),
    ]);
    if (infos.status === "fulfilled") {
      setProviders(infos.value);
      setDetectError(null);
    } else {
      setDetectError(errorText(infos.reason));
    }
    // No recommendation just means no badge — never a guessed one.
    setRecommended(rec.status === "fulfilled" ? rec.value : null);
    if (current.status === "fulfilled") {
      setSettings(current.value);
      setSettingsError(null);
    } else {
      setSettingsError(errorText(current.reason));
    }
    setDetecting(false);
    return infos.status === "fulfilled" ? infos.value : null;
  }, []);

  useEffect(() => {
    void detect();
  }, [detect]);

  // Leaving this screen mid-run stops the installer/sign-in rather than
  // leaving an invisible terminal waiting for input nobody can give it.
  // (Under StrictMode's mount→cleanup→mount nothing is running yet, so the
  // check keeps that cycle harmless.)
  const runningRef = useRef(false);
  runningRef.current = run?.state.stage === "running";
  useEffect(
    () => () => {
      if (runningRef.current) void connectCancel().catch(() => {});
    },
    [],
  );

  function startRun(provider: ProviderId, action: RunState["action"]) {
    runKeyRef.current += 1;
    setRun({ provider, state: { stage: "running", action, key: runKeyRef.current, cancelling: false } });
  }

  async function handleRunExit(provider: ProviderId, exit: ConnectExitPayload) {
    setRun((prev) =>
      prev && prev.provider === provider && prev.state.stage === "running"
        ? {
            provider,
            state: {
              stage: "finished",
              action: prev.state.action,
              key: prev.state.key,
              exit,
              startError: null,
            },
          }
        : prev,
    );
    // Re-detect: the install or sign-in may have changed everything.
    const infos = await detect();
    if (exit.action === "login" && exit.success && infos) {
      const current = await appSettingsGet().catch(() => null);
      const info = infos.find((i) => i.id === provider);
      if (current?.ai_provider === provider && info?.logged_in === true) onConnected?.(provider);
    }
  }

  function handleRunStartError(provider: ProviderId, message: string) {
    setRun((prev) =>
      prev && prev.provider === provider && prev.state.stage === "running"
        ? {
            provider,
            state: {
              stage: "finished",
              action: prev.state.action,
              key: prev.state.key,
              exit: null,
              startError: message,
            },
          }
        : prev,
    );
  }

  async function handleCancelRun() {
    setRun((prev) =>
      prev && prev.state.stage === "running"
        ? { ...prev, state: { ...prev.state, cancelling: true } }
        : prev,
    );
    try {
      await connectCancel();
      // The run's end still arrives as `connect-exit`.
    } catch (err) {
      const message = errorText(err);
      setRun((prev) =>
        prev && prev.state.stage === "running"
          ? {
              ...prev,
              state: {
                stage: "finished",
                action: prev.state.action,
                key: prev.state.key,
                exit: null,
                startError: `Couldn't stop it: ${message}`,
              },
            }
          : prev,
      );
    }
  }

  async function handleTest(provider: ProviderId) {
    setTests((t) => ({ ...t, [provider]: { status: "running" } }));
    try {
      const result = await aiTestConnection(provider);
      setTests((t) => ({ ...t, [provider]: { status: "done", result } }));
    } catch (err) {
      setTests((t) => ({ ...t, [provider]: { status: "failed", error: errorText(err) } }));
    }
  }

  async function handleUse(provider: ProviderId) {
    setChoosing(provider);
    setChooseError(null);
    try {
      // Fresh read so the other fields (Godot path, first run) are kept as
      // they are now, not as they were when this panel opened.
      const current = await appSettingsGet();
      const saved = await appSettingsSet({ ...current, ai_provider: provider });
      setSettings(saved);
      setSettingsError(null);
      onConnected?.(provider);
    } catch (err) {
      setChooseError(errorText(err));
    } finally {
      setChoosing(null);
    }
  }

  const chosen = settings?.ai_provider ?? null;
  // The API-key and local-model cards are built from a fixed list, so the
  // detected list only feeds the subscription group.
  const cliProviders = providers?.filter((p) => !isApiStyle(p.id)) ?? [];
  const ready = cliProviders.filter((p) => p.installed && p.logged_in === true);
  // Exactly one provider is ready and nothing's chosen yet: don't pick it
  // silently — make "Use this one" the obvious next click.
  const suggested = settings && chosen === null && ready.length === 1 ? ready[0].id : null;
  const runActive = run !== null && run.state.stage === "running";

  return (
    <div data-testid="connect-ai-panel" className="flex flex-col gap-3">
      <div className="flex items-start justify-between gap-3">
        <p className="text-sm text-muted-foreground">
          InfinaBox uses your own AI: a subscription you already have, your own API key, or a
          model on this computer. InfinaBox never sees your password and never charges you for
          AI.
        </p>
        <Button
          type="button"
          size="sm"
          variant="ghost"
          className="shrink-0"
          data-testid="connect-recheck"
          disabled={detecting || runActive}
          onClick={() => void detect()}
        >
          <RefreshCw data-icon="inline-start" className={detecting ? "animate-spin" : undefined} />
          {detecting ? "Checking…" : "Re-check"}
        </Button>
      </div>

      {detectError && (
        <Alert variant="destructive" data-testid="connect-detect-error">
          <AlertCircle />
          <AlertDescription className="break-words">
            Couldn't check which AI tools are on this computer: {detectError}
          </AlertDescription>
        </Alert>
      )}
      {settingsError && (
        <Alert variant="destructive">
          <AlertCircle />
          <AlertDescription className="break-words">
            Couldn't read which AI you chose: {settingsError}
          </AlertDescription>
        </Alert>
      )}
      {chooseError && (
        <Alert variant="destructive">
          <AlertCircle />
          <AlertDescription className="break-words">
            Couldn't save your choice: {chooseError}
          </AlertDescription>
        </Alert>
      )}

      {suggested && providers && (
        <p className="text-sm text-foreground/90" data-testid="connect-suggestion">
          {providers.find((p) => p.id === suggested)?.name} is installed and signed in. Want to
          use it?
        </p>
      )}

      {providers === null && !detectError ? (
        <div className="grid gap-2 md:grid-cols-2">
          <Skeleton className="h-36" />
          <Skeleton className="h-36" />
        </div>
      ) : (
        providers && (
          <section className="flex flex-col gap-2" data-testid="connect-group-subscription">
            <h3 className="text-xs font-medium tracking-wide text-muted-foreground uppercase">
              Use an AI subscription you already have
            </h3>
            <div className="grid items-start gap-2 md:grid-cols-2">
            {cliProviders.map((p) => {
              const ownRun = run?.provider === p.id ? run.state : null;
              return (
                <ProviderCard
                  key={p.id}
                  provider={p}
                  recommended={recommended === p.id}
                  inUse={chosen === p.id}
                  highlightUse={suggested === p.id || (chosen === null && isUsable(p) && ready.length === 0)}
                  run={ownRun}
                  otherRunActive={runActive && run?.provider !== p.id}
                  test={tests[p.id] ?? null}
                  choosing={choosing === p.id}
                  onInstallClick={() => setRun({ provider: p.id, state: { stage: "confirm", action: "install" } })}
                  onConfirmInstall={() => startRun(p.id, "install")}
                  onSignIn={() => startRun(p.id, "login")}
                  onCancelRun={() => void handleCancelRun()}
                  onCloseRun={() => setRun(null)}
                  onRunExit={(exit) => void handleRunExit(p.id, exit)}
                  onRunStartError={(message) => handleRunStartError(p.id, message)}
                  onTest={() => void handleTest(p.id)}
                  onUse={() => void handleUse(p.id)}
                />
              );
            })}
            </div>
          </section>
        )
      )}

      {/* Always shown: these cards don't depend on detecting CLI tools, and
          each shows its own errors. */}
      <section className="flex flex-col gap-2" data-testid="connect-group-api">
        <h3 className="text-xs font-medium tracking-wide text-muted-foreground uppercase">
          Use an API key or a model on this computer
        </h3>
        <div className="grid items-start gap-2 md:grid-cols-2">
          {API_PROVIDERS.map((def) => (
            <ApiProviderCard
              key={def.id}
              def={def}
              recommended={recommended === def.id}
              inUse={chosen === def.id}
              settings={settings}
              checkToken={checkToken}
              test={tests[def.id] ?? null}
              choosing={choosing === def.id}
              onSettingsSaved={setSettings}
              onTest={() => void handleTest(def.id)}
              onUse={() => void handleUse(def.id)}
            />
          ))}
        </div>
      </section>
    </div>
  );
}
