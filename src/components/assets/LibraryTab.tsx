import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { open } from "@tauri-apps/plugin-dialog";
import { ExternalLink as ExternalIcon, FolderOpen, Loader2, Search } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { libraryImport, libraryProviders, librarySearch } from "@/lib/studio-api";
import { cn } from "@/lib/utils";
import type { AssetInfo, AssetKind, LibraryItem, LibraryProviderInfo } from "@/lib/studio-types";
import { FILTER_KINDS, KIND_LABEL, errorText, hasLicense } from "./assets-format";
import {
  Chips,
  KIND_ICON,
  LicenseFields,
  Note,
  emptyLicenseDraft,
  licenseDraftValid,
  licenseFromDraft,
} from "./shared";

interface LibraryTabProps {
  projectPath: string;
  /** Called with what was added, so the Project tab can refresh. */
  onAdded: (assets: AssetInfo[]) => void;
}

export function LibraryTab({ projectPath, onAdded }: LibraryTabProps) {
  const [providers, setProviders] = useState<LibraryProviderInfo[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    libraryProviders()
      .then((p) => {
        if (cancelled) return;
        setProviders(p);
        setSelected((cur) => cur ?? p[0]?.id ?? null);
      })
      .catch((e) => !cancelled && setError(errorText(e)));
    return () => {
      cancelled = true;
    };
  }, []);

  if (error) return <div className="p-3"><Note tone="error">{error}</Note></div>;
  if (!providers) {
    return (
      <div className="flex flex-1 items-center justify-center gap-2 text-sm text-muted-foreground">
        <Loader2 className="size-4 animate-spin" />
        Loading the libraries…
      </div>
    );
  }
  if (providers.length === 0) {
    return <div className="p-3"><Note tone="info">No asset libraries are available.</Note></div>;
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-3">
      <div role="tablist" aria-label="Libraries" className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
        {providers.map((p) => (
          <button
            key={p.id}
            type="button"
            role="tab"
            aria-selected={selected === p.id}
            onClick={() => setSelected(p.id)}
            className={cn(
              "flex flex-col gap-0.5 rounded-xl border p-3 text-left transition-colors",
              selected === p.id ? "border-foreground bg-muted/50" : "border-border hover:bg-muted/40",
            )}
          >
            <span className="text-sm font-medium">{p.name}</span>
            <span className="text-xs text-muted-foreground">{p.blurb}</span>
          </button>
        ))}
      </div>
      {/* Every provider's pane stays mounted so a search and its results survive switching. */}
      {providers.map((p) => (
        <ProviderPane
          key={p.id}
          provider={p}
          projectPath={projectPath}
          visible={selected === p.id}
          onAdded={onAdded}
        />
      ))}
    </div>
  );
}

function ProviderPane({
  provider,
  projectPath,
  visible,
  onAdded,
}: {
  provider: LibraryProviderInfo;
  projectPath: string;
  visible: boolean;
  onAdded: (assets: AssetInfo[]) => void;
}) {
  const [text, setText] = useState("");
  const [folder, setFolder] = useState<string | null>(null);
  const [kind, setKind] = useState<AssetKind | null>(null);
  const [results, setResults] = useState<LibraryItem[] | null>(null);
  const [searching, setSearching] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [importing, setImporting] = useState<string | null>(null);
  const [needLicense, setNeedLicense] = useState<LibraryItem | null>(null);
  const [licenseError, setLicenseError] = useState<string | null>(null);

  const pickFolder = async () => {
    try {
      const picked = await open({ directory: true, multiple: false });
      if (typeof picked === "string") setFolder(picked);
    } catch (e) {
      setError(errorText(e));
    }
  };

  const search = async () => {
    if (provider.needs_folder && !folder) return;
    setSearching(true);
    setError(null);
    setNotice(null);
    try {
      const q = provider.needs_folder ? `${folder}|${text.trim()}` : text.trim();
      setResults(await librarySearch(provider.id, { text: q, kind, limit: 60 }));
    } catch (e) {
      setResults(null);
      setError(errorText(e));
    } finally {
      setSearching(false);
    }
  };

  const doImport = async (item: LibraryItem): Promise<boolean> => {
    setImporting(item.id);
    setError(null);
    setNotice(null);
    try {
      const added = await libraryImport(projectPath, provider.id, item);
      setNotice(
        added.length === 0
          ? `Nothing new was added for "${item.title}".`
          : `Added to your game: ${added.map((a) => a.path).join(", ")}`,
      );
      onAdded(added);
      return true;
    } catch (e) {
      const msg = errorText(e);
      if (needLicense) setLicenseError(msg);
      else setError(msg);
      return false;
    } finally {
      setImporting(null);
    }
  };

  const onImportClick = (item: LibraryItem) => {
    if (hasLicense(item.license)) {
      void doImport(item);
    } else {
      setLicenseError(null);
      setNeedLicense(item);
    }
  };

  const folderMissing = provider.needs_folder && !folder;

  return (
    <div className={cn("flex flex-col gap-3", !visible && "hidden")} data-testid={`library-${provider.id}`}>
      <form
        className="flex flex-wrap items-center gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          void search();
        }}
      >
        {provider.needs_folder && (
          <Button type="button" variant="outline" onClick={() => void pickFolder()} className="max-w-72">
            <FolderOpen />
            <span className="truncate">{folder ?? "Choose a folder…"}</span>
          </Button>
        )}
        <div className="relative min-w-40 flex-1 basis-56">
          <Search className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground" />
          <Input
            className="pl-8"
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder={provider.needs_folder ? "Only files with this in the name (optional)" : "Search, for example: tree, footsteps"}
            aria-label="Search"
          />
        </div>
        <Chips
          label="Filter by kind"
          value={kind}
          onChange={setKind}
          options={FILTER_KINDS.map((k) => ({ id: k, label: KIND_LABEL[k] }))}
        />
        <Button type="submit" disabled={searching || folderMissing}>
          {searching ? <Loader2 className="animate-spin" /> : <Search />}
          {provider.needs_folder ? "Look in folder" : "Search"}
        </Button>
        {provider.site_url && (
          <Button type="button" variant="ghost" onClick={() => void openUrl(provider.site_url!).catch(() => undefined)}>
            <ExternalIcon />
            Browse the website
          </Button>
        )}
      </form>

      {notice && <Note tone="success">{notice}</Note>}
      {error && <Note tone="error">{error}</Note>}

      {searching ? (
        <p className="flex items-center gap-2 py-6 text-sm text-muted-foreground">
          <Loader2 className="size-4 animate-spin" />
          Searching {provider.name}…
        </p>
      ) : results === null ? (
        !error && (
          <p className="py-6 text-center text-sm text-muted-foreground">
            {provider.needs_folder ? "Choose a folder." : `Search ${provider.name}.`}
          </p>
        )
      ) : results.length === 0 ? (
        <p className="py-6 text-center text-sm text-muted-foreground">Nothing found. Try different words or another kind.</p>
      ) : (
        <ul className="grid grid-cols-[repeat(auto-fill,minmax(15rem,1fr))] gap-3" data-testid="library-results">
          {results.map((item) => (
            <ResultCard
              key={`${item.provider}:${item.id}`}
              item={item}
              busy={importing === item.id}
              disabled={importing !== null}
              onImport={() => onImportClick(item)}
            />
          ))}
        </ul>
      )}

      <LicenseNeededDialog
        item={needLicense}
        busy={importing !== null}
        error={licenseError}
        onClose={() => setNeedLicense(null)}
        onConfirm={async (license) => {
          if (!needLicense) return;
          const ok = await doImport({ ...needLicense, license });
          if (ok) setNeedLicense(null);
        }}
      />
    </div>
  );
}

function ResultCard({
  item,
  busy,
  disabled,
  onImport,
}: {
  item: LibraryItem;
  busy: boolean;
  disabled: boolean;
  onImport: () => void;
}) {
  const [thumbFailed, setThumbFailed] = useState(false);
  const Icon = KIND_ICON[item.kind];
  const known = hasLicense(item.license);
  return (
    <li className="flex gap-3 rounded-xl border border-border bg-card p-3">
      <div className="flex size-14 shrink-0 items-center justify-center overflow-hidden rounded-lg border border-border bg-muted/50">
        {item.thumbnail_url && !thumbFailed ? (
          <img
            src={item.thumbnail_url}
            alt=""
            loading="lazy"
            className="size-full object-cover"
            onError={() => setThumbFailed(true)}
          />
        ) : (
          <Icon className="size-6 text-muted-foreground/70" />
        )}
      </div>
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <span className="truncate text-sm font-medium" title={item.title}>
          {item.title}
        </span>
        <span className="text-xs text-muted-foreground">
          {KIND_LABEL[item.kind]}
          {item.license.author ? ` · by ${item.license.author}` : ""}
        </span>
        <span
          className={cn(
            "text-xs",
            known ? "text-emerald-700 dark:text-emerald-400" : "text-amber-700 dark:text-amber-400",
          )}
        >
          {known ? item.license.name ?? "Licensed" : "License unknown — you'll be asked"}
        </span>
        <div className="mt-1 flex items-center gap-2">
          <Button size="sm" onClick={onImport} disabled={disabled}>
            {busy && <Loader2 className="animate-spin" />}
            Import
          </Button>
          {item.page_url && (
            <button
              type="button"
              className="text-xs text-muted-foreground underline underline-offset-3 hover:text-foreground"
              onClick={() => void openUrl(item.page_url!).catch(() => undefined)}
            >
              Open page
            </button>
          )}
        </div>
      </div>
    </li>
  );
}

function LicenseNeededDialog({
  item,
  busy,
  error,
  onClose,
  onConfirm,
}: {
  item: LibraryItem | null;
  busy: boolean;
  error: string | null;
  onClose: () => void;
  onConfirm: (license: LibraryItem["license"]) => void | Promise<void>;
}) {
  const [draft, setDraft] = useState(emptyLicenseDraft());

  useEffect(() => {
    if (!item) return;
    setDraft({
      ...emptyLicenseDraft(item.license.source ?? ""),
      author: item.license.author ?? "",
      url: item.license.url ?? "",
    });
  }, [item]);

  return (
    <Dialog open={item !== null} onOpenChange={(o) => !o && !busy && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Which license does it come with?</DialogTitle>
          <DialogDescription>
            InfinaBox couldn't tell what license "{item?.title}" uses. Look for it in the pack's own terms (often a
            LICENSE or README file, or the download page) and enter it here, so it's recorded with the file.
          </DialogDescription>
        </DialogHeader>
        <LicenseFields value={draft} onChange={setDraft} />
        {error && <Note tone="error">{error}</Note>}
        <DialogFooter>
          <Button variant="outline" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button onClick={() => void onConfirm(licenseFromDraft(draft))} disabled={busy || !licenseDraftValid(draft)}>
            {busy && <Loader2 className="animate-spin" />}
            Import
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
