import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { AlertCircle, CheckCircle2, File, FileAudio, Box, Image as ImageIcon, Type } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import type { AssetInfo, AssetKind, LicenseInfo } from "@/lib/studio-types";
import { LICENSE_CHOICES, hasLicense, licenseLabel } from "./assets-format";
import { canThumbnail, loadThumbnail } from "./thumbnails";

/** The transparency checkerboard, theme-aware (no stylesheet edit needed). */
export const checkerStyle: CSSProperties = {
  backgroundColor: "var(--background)",
  backgroundImage:
    "conic-gradient(color-mix(in oklch, var(--foreground) 10%, transparent) 25%, transparent 0 50%, color-mix(in oklch, var(--foreground) 10%, transparent) 0 75%, transparent 0)",
  backgroundSize: "16px 16px",
};

export const KIND_ICON: Record<AssetKind, typeof File> = {
  image: ImageIcon,
  audio: FileAudio,
  model3d: Box,
  font: Type,
  other: File,
};

/** Green when there's a license, amber "License unknown" otherwise. */
export function LicenseBadge({ license }: { license: LicenseInfo | null }) {
  const known = hasLicense(license);
  return (
    <Badge
      variant="secondary"
      className={cn(
        "max-w-full truncate",
        known
          ? "bg-emerald-500/15 text-emerald-700 dark:text-emerald-400"
          : "bg-amber-500/15 text-amber-700 dark:text-amber-400",
      )}
    >
      {licenseLabel(license)}
    </Badge>
  );
}

/** A picture for the asset when it's a small image; an icon otherwise. */
export function AssetThumb({
  projectPath,
  asset,
  className,
}: {
  projectPath: string;
  asset: AssetInfo;
  className?: string;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [src, setSrc] = useState<string | null>(null);
  const Icon = KIND_ICON[asset.kind];

  useEffect(() => {
    setSrc(null);
    const el = ref.current;
    if (!el || !canThumbnail(asset)) return;
    let cancelled = false;
    const load = () => {
      void loadThumbnail(projectPath, asset).then((url) => {
        if (!cancelled) setSrc(url);
      });
    };
    if (typeof IntersectionObserver === "undefined") {
      load();
      return () => {
        cancelled = true;
      };
    }
    const io = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) {
          io.disconnect();
          load();
        }
      },
      { rootMargin: "120px" },
    );
    io.observe(el);
    return () => {
      cancelled = true;
      io.disconnect();
    };
  }, [projectPath, asset]);

  return (
    <div
      ref={ref}
      style={checkerStyle}
      className={cn("flex items-center justify-center overflow-hidden", className)}
    >
      {src ? (
        <img src={src} alt="" className="size-full object-contain [image-rendering:pixelated]" />
      ) : (
        <Icon className="size-7 text-muted-foreground/70" />
      )}
    </div>
  );
}

/** A short line of feedback, always shown as plain text. */
export function Note({
  tone,
  children,
  className,
}: {
  tone: "error" | "success" | "info";
  children: ReactNode;
  className?: string;
}) {
  const Icon = tone === "success" ? CheckCircle2 : AlertCircle;
  return (
    <div
      role={tone === "error" ? "alert" : "status"}
      className={cn(
        "flex items-start gap-2 rounded-lg border px-3 py-2 text-sm",
        tone === "error" && "border-destructive/30 bg-destructive/10 text-destructive",
        tone === "success" && "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-400",
        tone === "info" && "border-border bg-muted/50 text-muted-foreground",
        className,
      )}
    >
      <Icon className="mt-0.5 size-4 shrink-0" />
      <div className="min-w-0 break-words">{children}</div>
    </div>
  );
}

export function Field({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <label className="flex flex-col gap-1 text-xs font-medium text-muted-foreground">
      {label}
      {children}
      {hint && <span className="font-normal">{hint}</span>}
    </label>
  );
}

export const selectClass =
  "h-8 w-full rounded-lg border border-input bg-transparent px-2 text-sm text-foreground outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 dark:bg-input/30 [&>option]:bg-popover";

/** A row of toggle chips (kind filters, size presets). */
export function Chips<T extends string>({
  options,
  value,
  onChange,
  label,
}: {
  options: { id: T; label: string }[];
  value: T | null;
  onChange: (id: T | null) => void;
  label: string;
}) {
  return (
    <div role="group" aria-label={label} className="flex flex-wrap gap-1">
      {options.map((o) => {
        const on = value === o.id;
        return (
          <button
            key={o.id}
            type="button"
            aria-pressed={on}
            onClick={() => onChange(on ? null : o.id)}
            className={cn(
              "h-7 rounded-full border px-2.5 text-xs font-medium transition-colors",
              on
                ? "border-foreground bg-foreground text-background"
                : "border-border text-muted-foreground hover:bg-muted hover:text-foreground",
            )}
          >
            {o.label}
          </button>
        );
      })}
    </div>
  );
}

// ---- License entry, shared by "Import file" and the Library's unknown-license prompt ----

export interface LicenseDraft {
  choice: string;
  other: string;
  source: string;
  author: string;
  url: string;
}

export const emptyLicenseDraft = (source = ""): LicenseDraft => ({
  choice: "CC0-1.0",
  other: "",
  source,
  author: "",
  url: "",
});

export function licenseFromDraft(d: LicenseDraft): LicenseInfo {
  const name = d.choice === "Other…" ? d.other.trim() : d.choice;
  const clean = (s: string) => (s.trim() ? s.trim() : null);
  return {
    name: name || null,
    source: clean(d.source),
    author: clean(d.author),
    url: clean(d.url),
    generated_by: null,
  };
}

export function licenseDraftValid(d: LicenseDraft): boolean {
  return d.choice !== "Other…" || d.other.trim().length > 0;
}

export function LicenseFields({ value, onChange }: { value: LicenseDraft; onChange: (d: LicenseDraft) => void }) {
  const set = (patch: Partial<LicenseDraft>) => onChange({ ...value, ...patch });
  return (
    <div className="flex flex-col gap-3">
      <Field label="License">
        <select className={selectClass} value={value.choice} onChange={(e) => set({ choice: e.target.value })}>
          {LICENSE_CHOICES.map((c) => (
            <option key={c} value={c}>
              {c}
            </option>
          ))}
        </select>
      </Field>
      {value.choice === "Other…" && (
        <Field label="License name">
          <Input
            value={value.other}
            onChange={(e) => set({ other: e.target.value })}
            placeholder="As written in the pack's terms"
          />
        </Field>
      )}
      <div className="grid grid-cols-2 gap-3">
        <Field label="Where it came from (optional)">
          <Input value={value.source} onChange={(e) => set({ source: e.target.value })} placeholder="Website or pack name" />
        </Field>
        <Field label="Made by (optional)">
          <Input value={value.author} onChange={(e) => set({ author: e.target.value })} placeholder="Author" />
        </Field>
      </div>
      <Field label="Link to the license or page (optional)">
        <Input value={value.url} onChange={(e) => set({ url: e.target.value })} placeholder="https://…" />
      </Field>
    </div>
  );
}
