import { useEffect, useState } from "react";
import { Check, Copy, Loader2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { assetsCredits, assetsImport } from "@/lib/studio-api";
import type { AssetInfo } from "@/lib/studio-types";
import { errorText, fileName } from "./assets-format";
import { Field, LicenseFields, Note, emptyLicenseDraft, licenseDraftValid, licenseFromDraft } from "./shared";

/** Title, folder and license for a file the person picked, then `assets_import`. */
export function ImportDialog({
  projectPath,
  source,
  onClose,
  onImported,
}: {
  projectPath: string;
  /** The picked file; the dialog is open while this is set. */
  source: string | null;
  onClose: () => void;
  onImported: (asset: AssetInfo) => void;
}) {
  const [title, setTitle] = useState("");
  const [folder, setFolder] = useState("");
  const [license, setLicense] = useState(emptyLicenseDraft());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!source) return;
    setTitle(fileName(source).replace(/\.[^.]+$/, ""));
    setFolder("");
    setLicense(emptyLicenseDraft());
    setError(null);
    setBusy(false);
  }, [source]);

  const submit = async () => {
    if (!source) return;
    setBusy(true);
    setError(null);
    try {
      const asset = await assetsImport(projectPath, source, folder.trim() || null, title.trim(), licenseFromDraft(license));
      onImported(asset);
    } catch (e) {
      setError(errorText(e));
      setBusy(false);
    }
  };

  const ready = title.trim().length > 0 && licenseDraftValid(license) && !busy;

  return (
    <Dialog open={source !== null} onOpenChange={(open) => !open && !busy && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Import a file</DialogTitle>
          <DialogDescription className="break-all">
            A copy of <span className="font-mono text-xs">{source}</span> is added to your game. Tell InfinaBox where it
            came from so you can credit it later.
          </DialogDescription>
        </DialogHeader>
        <div className="flex flex-col gap-3">
          <Field label="Name">
            <Input value={title} onChange={(e) => setTitle(e.target.value)} />
          </Field>
          <Field label="Folder inside your game (optional)" hint="For example: sprites or sounds/music">
            <Input value={folder} onChange={(e) => setFolder(e.target.value)} placeholder="Leave empty to use the default" />
          </Field>
          <LicenseFields value={license} onChange={setLicense} />
          {error && <Note tone="error">{error}</Note>}
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button onClick={() => void submit()} disabled={!ready}>
            {busy && <Loader2 className="animate-spin" />}
            Import
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

/** The credits page (Markdown) with a Copy button. */
export function CreditsDialog({
  projectPath,
  open,
  onClose,
}: {
  projectPath: string;
  open: boolean;
  onClose: () => void;
}) {
  const [text, setText] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setText(null);
    setError(null);
    setCopied(false);
    assetsCredits(projectPath)
      .then((t) => !cancelled && setText(t))
      .catch((e) => !cancelled && setError(errorText(e)));
    return () => {
      cancelled = true;
    };
  }, [open, projectPath]);

  const copy = async () => {
    if (text === null) return;
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1800);
    } catch {
      setError("Couldn't copy to the clipboard. Select the text and copy it yourself.");
    }
  };

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>Credits</DialogTitle>
          <DialogDescription>
            Everyone whose work is in your game, built from your assets' licenses. Paste it into your credits screen or
            store page.
          </DialogDescription>
        </DialogHeader>
        {error && <Note tone="error">{error}</Note>}
        {text === null && !error ? (
          <p className="text-sm text-muted-foreground">Loading…</p>
        ) : text !== null ? (
          <pre
            data-testid="credits-text"
            className="max-h-80 overflow-auto rounded-lg border border-border bg-muted/40 p-3 text-xs whitespace-pre-wrap select-text"
          >
            {text.trim() ? text : "There's nothing to credit yet."}
          </pre>
        ) : null}
        <DialogFooter showCloseButton>
          <Button variant="outline" onClick={() => void copy()} disabled={text === null || !text.trim()}>
            {copied ? <Check /> : <Copy />}
            {copied ? "Copied" : "Copy"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
