import { useCallback, useEffect, useMemo, useState } from "react";
import { CheckCircle2, ChevronRight, Loader2, Plug, Unplug } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import { contextList, contextRead, credentialClear, generateProviders } from "@/lib/studio-api";
import { cn } from "@/lib/utils";
import type { AssetInfo, GenKind, GenProviderInfo } from "@/lib/studio-types";
import { ConnectDialog } from "./ConnectDialog";
import { GEN_KIND_LABEL, errorText } from "./assets-format";
import { GenerateForm } from "./GenerateForms";
import { Note } from "./shared";

const KINDS: GenKind[] = ["image", "voice", "sfx", "music"];

interface GenerateTabProps {
  projectPath: string;
  onAccepted: (asset: AssetInfo) => void;
}

export function GenerateTab({ projectPath, onAccepted }: GenerateTabProps) {
  const [providers, setProviders] = useState<GenProviderInfo[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [connecting, setConnecting] = useState<GenProviderInfo | null>(null);
  const [disconnecting, setDisconnecting] = useState<string | null>(null);
  const [kind, setKind] = useState<GenKind>("image");
  const [styleGuide, setStyleGuide] = useState<string | null>(null);

  const loadProviders = useCallback(async () => {
    try {
      setProviders(await generateProviders());
      setError(null);
    } catch (e) {
      setError(errorText(e));
    }
  }, []);

  useEffect(() => {
    void loadProviders();
  }, [loadProviders]);

  // The Style Guide card, if there is one, is added to every prompt.
  useEffect(() => {
    let cancelled = false;
    setStyleGuide(null);
    (async () => {
      try {
        const cards = await contextList(projectPath);
        const guide = cards.find((c) => c.card_type === "style-guide");
        if (!guide) return;
        const card = await contextRead(projectPath, guide.path);
        if (!cancelled && card.body.trim()) setStyleGuide(card.body.trim());
      } catch {
        // No cards to read is normal for a new game; generating still works.
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [projectPath]);

  const disconnect = async (p: GenProviderInfo) => {
    setDisconnecting(p.id);
    setError(null);
    try {
      for (const name of p.needs) await credentialClear(name);
      await loadProviders();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setDisconnecting(null);
    }
  };

  const byKind = useMemo(() => {
    const m = new Map<GenKind, GenProviderInfo[]>();
    for (const k of KINDS) m.set(k, (providers ?? []).filter((p) => p.kinds.includes(k)));
    return m;
  }, [providers]);

  if (!providers) {
    return error ? (
      <div className="p-3">
        <Note tone="error">{error}</Note>
      </div>
    ) : (
      <div className="flex flex-1 items-center justify-center gap-2 text-sm text-muted-foreground">
        <Loader2 className="size-4 animate-spin" />
        Checking your accounts…
      </div>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-3">
      {error && <Note tone="error">{error}</Note>}

      <section className="flex flex-col gap-2">
        <h3 className="text-sm font-medium">Your accounts</h3>
        <ul className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
          {providers.map((p) => (
            <li key={p.id} className="flex flex-col gap-2 rounded-xl border border-border bg-card p-3" data-testid={`gen-provider-${p.id}`}>
              <div className="flex items-center gap-2">
                <span className="min-w-0 flex-1 truncate text-sm font-medium">{p.name}</span>
                <span
                  className={cn(
                    "inline-flex h-5 items-center gap-1 rounded-full px-2 text-xs font-medium",
                    p.connected
                      ? "bg-emerald-500/15 text-emerald-700 dark:text-emerald-400"
                      : "bg-muted text-muted-foreground",
                  )}
                >
                  {p.connected && <CheckCircle2 className="size-3" />}
                  {p.connected ? "Connected" : "Not connected"}
                </span>
              </div>
              <p className="text-xs">Makes: {p.kinds.map((k) => GEN_KIND_LABEL[k].toLowerCase()).join(", ")}</p>
              <div className="mt-auto">
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
              </div>
            </li>
          ))}
        </ul>
      </section>

      <section className="flex flex-col gap-3">
        <div role="tablist" aria-label="What to make" className="flex flex-wrap gap-1 border-b border-border">
          {KINDS.map((k) => {
            const ok = (byKind.get(k) ?? []).some((p) => p.connected);
            return (
              <button
                key={k}
                type="button"
                role="tab"
                aria-selected={kind === k}
                onClick={() => setKind(k)}
                className={cn(
                  "-mb-px flex items-center gap-1.5 border-b-2 px-3 py-2 text-sm transition-colors",
                  kind === k
                    ? "border-foreground font-medium"
                    : "border-transparent text-muted-foreground hover:text-foreground",
                )}
              >
                {GEN_KIND_LABEL[k]}
                {!ok && <span className="size-1.5 rounded-full bg-muted-foreground/40" aria-label="Not connected" />}
              </button>
            );
          })}
        </div>

        {styleGuide && (
          <div className="flex flex-col gap-1">
            <Collapsible>
              <CollapsibleTrigger className="group flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground">
                <ChevronRight className="size-3.5 transition-transform group-data-[state=open]:rotate-90" />
                See what will be added
              </CollapsibleTrigger>
              <CollapsibleContent>
                <pre className="mt-1 max-h-40 overflow-auto rounded-lg bg-muted/50 p-2.5 text-xs whitespace-pre-wrap select-text">
                  {styleGuide}
                </pre>
              </CollapsibleContent>
            </Collapsible>
          </div>
        )}

        {/* Every generator stays mounted so a half-written prompt or an
            un-accepted result survives switching kinds. */}
        {KINDS.map((k) => (
          <div key={k} className={cn(kind !== k && "hidden")}>
            {(byKind.get(k) ?? []).some((p) => p.connected) ? (
              <GenerateForm kind={k} projectPath={projectPath} styleGuide={styleGuide} onAccepted={onAccepted} />
            ) : (
              <NeedsProvider kind={k} providers={byKind.get(k) ?? []} onConnect={setConnecting} />
            )}
          </div>
        ))}
      </section>

      <ConnectDialog
        provider={connecting}
        onClose={() => setConnecting(null)}
        onConnected={() => {
          setConnecting(null);
          void loadProviders();
        }}
      />
    </div>
  );
}

function NeedsProvider({
  kind,
  providers,
  onConnect,
}: {
  kind: GenKind;
  providers: GenProviderInfo[];
  onConnect: (p: GenProviderInfo) => void;
}) {
  return (
    <div className="flex flex-col items-center gap-3 rounded-xl border border-dashed border-border p-8 text-center">
      <h3 className="text-base font-medium">Connect an account to make {GEN_KIND_LABEL[kind].toLowerCase()}</h3>
      {providers.length === 0 ? (
        <p className="max-w-sm text-sm text-muted-foreground">No available account can make this yet.</p>
      ) : (
        <div className="flex flex-wrap justify-center gap-2">
          {providers.map((p) => (
            <Button key={p.id} onClick={() => onConnect(p)}>
              <Plug />
              Connect {p.name} to generate
            </Button>
          ))}
        </div>
      )}
    </div>
  );
}
