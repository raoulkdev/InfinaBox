import { useState } from "react";
import { AlertTriangle, CheckCircle2, ChevronDown } from "lucide-react";
import { cn } from "@/lib/utils";
import type { AssetInfo, HealthReport } from "@/lib/studio-types";
import { formatBytes } from "./assets-format";

// A summary of what's untidy in the project's assets. Every count comes
// straight from `assets_health`; nothing is estimated here.

type Group = "unused" | "missing" | "oversized" | "unlicensed";

interface HealthStripProps {
  health: HealthReport;
  assets: AssetInfo[];
  onOpenAsset: (path: string) => void;
}

export function HealthStrip({ health, assets, onOpenAsset }: HealthStripProps) {
  const [open, setOpen] = useState<Group | null>(null);

  const groups: { id: Group; label: string; count: number }[] = [
    { id: "unused", label: "Unused", count: health.unused.length },
    { id: "missing", label: "Missing files", count: health.missing_refs.length },
    { id: "oversized", label: "Very large", count: health.oversized.length },
    { id: "unlicensed", label: "No license", count: health.unlicensed.length },
  ];
  const total = groups.reduce((n, g) => n + g.count, 0);

  if (total === 0) {
    return (
      <div className="flex items-center gap-2 rounded-lg border border-border px-3 py-2 text-sm text-muted-foreground">
        <CheckCircle2 className="size-4 text-emerald-600 dark:text-emerald-400" />
        Nothing to tidy: every asset is used, licensed and a sensible size.
      </div>
    );
  }

  const known = new Set(assets.map((a) => a.path));
  const openable = (path: string) => known.has(path);

  const item = (path: string, note?: string) => (
    <li key={path}>
      <button
        type="button"
        disabled={!openable(path)}
        onClick={() => onOpenAsset(path)}
        className="flex w-full items-center gap-2 rounded px-2 py-1 text-left text-xs hover:bg-muted disabled:cursor-default disabled:hover:bg-transparent"
      >
        <span className="min-w-0 flex-1 truncate font-mono">{path}</span>
        {note && <span className="shrink-0 text-muted-foreground">{note}</span>}
      </button>
    </li>
  );

  return (
    <div className="rounded-lg border border-border" data-testid="health-strip">
      <div className="flex flex-wrap items-center gap-1.5 px-2 py-1.5">
        <span className="flex items-center gap-1.5 px-1 text-xs font-medium text-muted-foreground">
          <AlertTriangle className="size-3.5 text-amber-600 dark:text-amber-400" />
          Worth a look
        </span>
        {groups.map((g) => (
          <button
            key={g.id}
            type="button"
            disabled={g.count === 0}
            aria-expanded={open === g.id}
            onClick={() => setOpen(open === g.id ? null : g.id)}
            className={cn(
              "flex h-7 items-center gap-1.5 rounded-full border px-2.5 text-xs transition-colors",
              g.count === 0
                ? "border-transparent text-muted-foreground/60"
                : "border-border hover:bg-muted",
              open === g.id && "border-foreground",
            )}
          >
            <span className={cn("font-semibold tabular-nums", g.count > 0 && "text-amber-700 dark:text-amber-400")}>
              {g.count}
            </span>
            {g.label}
            {g.count > 0 && <ChevronDown className={cn("size-3 transition-transform", open === g.id && "rotate-180")} />}
          </button>
        ))}
      </div>
      {open && (
        <ul className="max-h-44 overflow-y-auto border-t border-border p-1">
          {open === "unused" && health.unused.map((p) => item(p))}
          {open === "unlicensed" && health.unlicensed.map((p) => item(p))}
          {open === "oversized" &&
            health.oversized.map((o) =>
              item(o.path, `${o.width && o.height ? `${o.width} × ${o.height} · ` : ""}${formatBytes(o.size_bytes)}`),
            )}
          {open === "missing" &&
            health.missing_refs.map((m) => (
              <li key={`${m.from}\0${m.to}`} className="px-2 py-1 text-xs">
                <span className="font-mono">{m.from}</span>
                <span className="text-muted-foreground"> looks for </span>
                <span className="font-mono">{m.to}</span>
                <span className="text-muted-foreground">, which isn't there.</span>
              </li>
            ))}
        </ul>
      )}
    </div>
  );
}
