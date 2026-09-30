import { useCallback, useEffect, useState, type ReactNode } from "react";
import { AlertCircle, Bot, CheckCircle2, Cpu, Info, KeyRound, Loader2, Plug, RotateCcw, Unplug, type LucideIcon } from "lucide-react";
import { GodotSettings } from "./GodotSettings";
import { ConnectDialog } from "@/components/assets/ConnectDialog";
import { ConnectAiPanel } from "@/components/connect/ConnectAiPanel";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Skeleton } from "@/components/ui/skeleton";
import { appSettingsGet, appSettingsSet, credentialClear, generateProviders } from "@/lib/studio-api";
import type { AppSettings, GenProviderInfo } from "@/lib/studio-types";
import { cn } from "@/lib/utils";

// The app's settings, all in one place (they used to be split between
// the old Advanced tab and Home's Setup block). Everything shown comes from
// the backend; a failed read shows its own message.

type Category = "ai" | "godot" | "accounts" | "about";

const CATEGORIES: { id: Category; label: string; icon: LucideIcon }[] = [
  { id: "ai", label: "Your AI", icon: Bot },
  { id: "godot", label: "Godot", icon: Cpu },
  { id: "accounts", label: "Accounts", icon: KeyRound },
  { id: "about", label: "General", icon: Info },
];

function errorText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

export interface SettingsSectionProps {
  projectPath: string | null;
}

export function SettingsSection({ projectPath }: SettingsSectionProps) {
  const [category, setCategory] = useState<Category>("ai");
  // Bumped whenever the Godot page is shown, so its status is read fresh.
  const [godotShown, setGodotShown] = useState(0);

  function select(next: Category) {
    setCategory(next);
    if (next === "godot") setGodotShown((n) => n + 1);
  }

  const current = CATEGORIES.find((c) => c.id === category)!;

  return (
    <div className="flex min-h-0 min-w-0 flex-1 gap-2" data-testid="settings-section">
      <nav
        aria-label="Settings"
        className="flex w-56 shrink-0 flex-col gap-1 rounded-xl border border-border bg-card p-2"
      >
        <div data-tauri-drag-region className="px-2 pt-1 pb-2 text-xs font-medium tracking-wide text-muted-foreground">
          Settings
        </div>
        {CATEGORIES.map((c) => {
          const Icon = c.icon;
          return (
            <button
              key={c.id}
              type="button"
              data-testid={`settings-${c.id}`}
              aria-current={category === c.id ? "page" : undefined}
              onClick={() => select(c.id)}
              className={cn(
                "flex items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm text-muted-foreground outline-none transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/50",
                category === c.id && "bg-accent text-foreground",
              )}
            >
              <Icon className="size-4 shrink-0" />
              <span className="truncate">{c.label}</span>
            </button>
          );
        })}
      </nav>

      <div className="flex min-h-0 min-w-0 flex-1 flex-col rounded-xl border border-border bg-card">
        <div data-tauri-drag-region className="shrink-0 border-b border-border px-5 py-3">
          <h1 className="text-base font-semibold tracking-tight">{current.label}</h1>
        </div>
        <ScrollArea className="min-h-0 flex-1">
          <div className="flex w-full flex-col gap-4 px-5 py-5">
            {category === "ai" && <AiPage />}
            {category === "godot" && <GodotSettings projectPath={projectPath} refreshToken={godotShown} />}
            {category === "accounts" && <AccountsPage />}
            {category === "about" && <AboutPage />}
          </div>
        </ScrollArea>
      </div>
    </div>
  );
}

function AiPage() {
  return <ConnectAiPanel />;
}

function AccountsPage() {
  const [providers, setProviders] = useState<GenProviderInfo[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [connecting, setConnecting] = useState<GenProviderInfo | null>(null);
  const [disconnecting, setDisconnecting] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setProviders(await generateProviders());
      setError(null);
    } catch (err) {
      setError(errorText(err));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  async function disconnect(p: GenProviderInfo) {
    setDisconnecting(p.id);
    try {
      for (const name of p.needs) await credentialClear(name);
      await load();
    } catch (err) {
      setError(errorText(err));
    } finally {
      setDisconnecting(null);
    }
  }

  return (
    <>
      {error && (
        <Alert variant="destructive">
          <AlertCircle />
          <AlertDescription className="wrap-anywhere">{error}</AlertDescription>
        </Alert>
      )}
      {!providers && !error && <Skeleton className="h-32" />}
      <ul className="flex flex-col gap-2">
        {providers?.map((p) => (
          <li
            key={p.id}
            data-testid={`account-${p.id}`}
            className="flex items-center gap-3 rounded-lg border border-border bg-background px-3 py-2.5"
          >
            <div className="flex min-w-0 flex-1 flex-col">
              <span className="flex items-center gap-2 text-sm font-medium">
                {p.name}
                {p.connected && (
                  <span className="inline-flex items-center gap-1 rounded-full bg-emerald-500/15 px-2 text-xs text-emerald-700 dark:text-emerald-400">
                    <CheckCircle2 className="size-3" />
                    Connected
                  </span>
                )}
              </span>
            </div>
            {p.connected ? (
              <Button variant="outline" size="sm" onClick={() => void disconnect(p)} disabled={disconnecting === p.id}>
                {disconnecting === p.id ? <Loader2 className="animate-spin" /> : <Unplug />}
                Disconnect
              </Button>
            ) : (
              <Button size="sm" onClick={() => setConnecting(p)}>
                <Plug />
                Connect
              </Button>
            )}
          </li>
        ))}
      </ul>
      <ConnectDialog
        provider={connecting}
        onClose={() => setConnecting(null)}
        onConnected={() => {
          setConnecting(null);
          void load();
        }}
      />
    </>
  );
}

function AboutPage() {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    appSettingsGet().then(setSettings, (err) => setError(errorText(err)));
  }, []);

  async function showChecklistAgain() {
    if (!settings) return;
    setSaving(true);
    try {
      setSettings(await appSettingsSet({ ...settings, first_run_done: false }));
      setError(null);
    } catch (err) {
      setError(errorText(err));
    } finally {
      setSaving(false);
    }
  }

  return (
    <>
      {error && (
        <Alert variant="destructive">
          <AlertCircle />
          <AlertDescription className="wrap-anywhere">{error}</AlertDescription>
        </Alert>
      )}
      <Fact title="Welcome checklist">
        <div>
          <Button
            variant="outline"
            size="sm"
            onClick={() => void showChecklistAgain()}
            disabled={saving || !settings || settings.first_run_done === false}
          >
            <RotateCcw />
            Show it again
          </Button>
        </div>
      </Fact>
    </>
  );
}

function Fact({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="rounded-lg border border-border bg-background px-4 py-3">
      <h2 className="text-sm font-medium">{title}</h2>
      <div className="mt-1 text-sm text-muted-foreground">{children}</div>
    </section>
  );
}
