import { useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, LayoutGrid, List, Loader2, Receipt, RefreshCw, Search, Upload } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import type { AssetInfo, AssetKind, HealthReport } from "@/lib/studio-types";
import { FILTER_KINDS, KIND_LABEL, errorText, fileName, formatBytes, formatDimensions } from "./assets-format";
import { HealthStrip } from "./HealthStrip";
import { CreditsDialog, ImportDialog } from "./ProjectDialogs";
import { AssetThumb, Chips, KIND_ICON, LicenseBadge, Note } from "./shared";

interface ProjectTabProps {
  projectPath: string;
  assets: AssetInfo[] | null;
  health: HealthReport | null;
  loading: boolean;
  error: string | null;
  onRefresh: () => void;
  onImported: (asset: AssetInfo) => void;
  onOpenAsset: (asset: AssetInfo) => void;
  onGoTo: (tab: "library" | "generate") => void;
  /** A one-line note about something just added, shown above the grid. */
  notice: string | null;
}

type View = "grid" | "list";

export function ProjectTab({
  projectPath,
  assets,
  health,
  loading,
  error,
  onRefresh,
  onImported,
  onOpenAsset,
  onGoTo,
  notice,
}: ProjectTabProps) {
  const [kind, setKind] = useState<AssetKind | null>(null);
  const [query, setQuery] = useState("");
  const [view, setView] = useState<View>("grid");
  const [importSource, setImportSource] = useState<string | null>(null);
  const [creditsOpen, setCreditsOpen] = useState(false);
  const [pickError, setPickError] = useState<string | null>(null);

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    return (assets ?? []).filter(
      (a) => (!kind || a.kind === kind) && (!q || a.path.toLowerCase().includes(q)),
    );
  }, [assets, kind, query]);

  const pickFile = async () => {
    setPickError(null);
    try {
      const picked = await open({ directory: false, multiple: false });
      if (typeof picked === "string") setImportSource(picked);
    } catch (e) {
      setPickError(errorText(e));
    }
  };

  const openByPath = (path: string) => {
    const a = assets?.find((x) => x.path === path);
    if (a) onOpenAsset(a);
  };

  const empty = assets !== null && assets.length === 0;

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-3">
      <div className="flex flex-wrap items-center gap-2">
        <Chips
          label="Filter by kind"
          value={kind}
          onChange={setKind}
          options={FILTER_KINDS.map((k) => ({ id: k, label: KIND_LABEL[k] }))}
        />
        <div className="relative min-w-40 flex-1 basis-48">
          <Search className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground" />
          <Input
            className="pl-8"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search your assets"
            aria-label="Search your assets"
          />
        </div>
        <div className="flex rounded-lg border border-border p-0.5" role="group" aria-label="View">
          {(["grid", "list"] as const).map((v) => (
            <Button
              key={v}
              variant="ghost"
              size="icon-xs"
              aria-label={v === "grid" ? "Grid view" : "List view"}
              aria-pressed={view === v}
              className={cn(view === v && "bg-muted")}
              onClick={() => setView(v)}
            >
              {v === "grid" ? <LayoutGrid /> : <List />}
            </Button>
          ))}
        </div>
        <Button variant="outline" size="icon" aria-label="Refresh" onClick={onRefresh} disabled={loading}>
          <RefreshCw className={cn(loading && "animate-spin")} />
        </Button>
        <Button variant="outline" onClick={() => setCreditsOpen(true)}>
          <Receipt />
          Credits
        </Button>
        <Button onClick={() => void pickFile()}>
          <Upload />
          Import file…
        </Button>
      </div>

      {pickError && <Note tone="error">{pickError}</Note>}
      {error && <Note tone="error">{error}</Note>}
      {notice && <Note tone="success">{notice}</Note>}

      {health && assets && <HealthStrip health={health} assets={assets} onOpenAsset={openByPath} />}

      {assets === null && !error ? (
        <div className="flex flex-1 items-center justify-center gap-2 text-sm text-muted-foreground">
          <Loader2 className="size-4 animate-spin" />
          Looking through your game's files…
        </div>
      ) : empty ? (
        <div className="flex flex-1 flex-col items-center justify-center gap-3 rounded-xl border border-dashed border-border p-8 text-center">
          <div className="flex size-10 items-center justify-center rounded-lg border border-border">
            <FolderOpen className="size-5 text-muted-foreground" />
          </div>
          <h3 className="text-base font-medium">No assets in this game yet</h3>
          <div className="flex flex-wrap justify-center gap-2">
            <Button onClick={() => void pickFile()}>
              <Upload />
              Import file…
            </Button>
            <Button variant="outline" onClick={() => onGoTo("library")}>
              Browse the Library
            </Button>
            <Button variant="outline" onClick={() => onGoTo("generate")}>
              Generate
            </Button>
          </div>
        </div>
      ) : assets && shown.length === 0 ? (
        <p className="py-8 text-center text-sm text-muted-foreground">No assets match. Try another filter or search.</p>
      ) : view === "grid" ? (
        <ul className="grid grid-cols-[repeat(auto-fill,minmax(10.5rem,1fr))] gap-3" data-testid="asset-grid">
          {shown.map((a) => (
            <li key={a.path}>
              <button
                type="button"
                onClick={() => onOpenAsset(a)}
                className="group flex w-full flex-col overflow-hidden rounded-xl border border-border bg-card text-left transition-colors hover:border-foreground/30 focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:outline-none"
              >
                <AssetThumb projectPath={projectPath} asset={a} className="h-28 w-full border-b border-border" />
                <div className="flex flex-col gap-1 p-2.5">
                  <span className="truncate text-sm font-medium" title={a.path}>
                    {fileName(a.path)}
                  </span>
                  <span className="truncate text-xs text-muted-foreground">
                    {KIND_LABEL[a.kind]} · {formatBytes(a.size_bytes)}
                    {formatDimensions(a) ? ` · ${formatDimensions(a)}` : ""}
                  </span>
                  <LicenseBadge license={a.license} />
                </div>
              </button>
            </li>
          ))}
        </ul>
      ) : (
        <ul className="flex flex-col divide-y divide-border rounded-xl border border-border" data-testid="asset-list">
          {shown.map((a) => {
            const Icon = KIND_ICON[a.kind];
            return (
              <li key={a.path}>
                <button
                  type="button"
                  onClick={() => onOpenAsset(a)}
                  className="flex w-full items-center gap-3 px-3 py-2 text-left hover:bg-muted/60 focus-visible:bg-muted/60 focus-visible:outline-none"
                >
                  <Icon className="size-4 shrink-0 text-muted-foreground" />
                  <span className="min-w-0 flex-1 truncate text-sm" title={a.path}>
                    {a.path}
                  </span>
                  <span className="hidden shrink-0 text-xs text-muted-foreground sm:inline">
                    {formatDimensions(a) ? `${formatDimensions(a)} · ` : ""}
                    {formatBytes(a.size_bytes)}
                  </span>
                  <span className="w-32 shrink-0">
                    <LicenseBadge license={a.license} />
                  </span>
                </button>
              </li>
            );
          })}
        </ul>
      )}

      <ImportDialog
        projectPath={projectPath}
        source={importSource}
        onClose={() => setImportSource(null)}
        onImported={(asset) => {
          setImportSource(null);
          onImported(asset);
        }}
      />
      <CreditsDialog projectPath={projectPath} open={creditsOpen} onClose={() => setCreditsOpen(false)} />
    </div>
  );
}
