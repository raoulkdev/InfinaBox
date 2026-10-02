import { Suspense, lazy, useEffect, useState } from "react";
import { motion } from "motion/react";
import { X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { assetsReadBase64 } from "@/lib/studio-api";
import { fadeTransition, springTransition } from "@/lib/motion";
import type { AssetInfo, FilePayload } from "@/lib/studio-types";
import { AudioPreview } from "./AudioPreview";
import { ImagePreview } from "./ImagePreview";
import {
  KIND_LABEL,
  dataUrl,
  errorText,
  fileExtension,
  fileName,
  formatBytes,
  formatDimensions,
  hasLicense,
} from "./assets-format";
import { KIND_ICON, LicenseBadge, Note } from "./shared";

// `three` lives only in ModelPreview; this keeps it out of the main bundle.
const ModelPreview = lazy(() => import("./ModelPreview"));

// Previews are read whole into memory, so they have a ceiling.
const PREVIEW_MAX_BYTES = 32 * 1024 * 1024;

interface AssetPreviewProps {
  projectPath: string;
  asset: AssetInfo;
  onClose: () => void;
}

/** The side sheet over the Assets section: the file itself, then what's known about it. */
export function AssetPreview({ projectPath, asset, onClose }: AssetPreviewProps) {
  const [file, setFile] = useState<FilePayload | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setFile(null);
    setError(null);
    assetsReadBase64(projectPath, asset.path, PREVIEW_MAX_BYTES)
      .then((f) => {
        if (!cancelled) setFile(f);
      })
      .catch((e) => {
        if (!cancelled) setError(errorText(e));
      });
    return () => {
      cancelled = true;
    };
  }, [projectPath, asset.path, asset.size_bytes]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const Icon = KIND_ICON[asset.kind];
  const ext = fileExtension(asset.path);
  const dims = formatDimensions(asset);
  const license = asset.license;

  return (
    <motion.div
      className="absolute inset-0 z-20 flex justify-end"
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={fadeTransition}
    >
      <button
        type="button"
        aria-label="Close preview"
        className="absolute inset-0 cursor-default bg-background/60 backdrop-blur-[1px]"
        onClick={onClose}
      />
      <motion.aside
        role="dialog"
        aria-label={`Preview of ${fileName(asset.path)}`}
        data-testid="asset-preview"
        className="relative flex h-full w-[min(560px,100%)] flex-col rounded-xl border border-border bg-card shadow-xl"
        initial={{ x: 40 }}
        animate={{ x: 0 }}
        exit={{ x: 40 }}
        transition={springTransition}
      >
        <div className="flex shrink-0 items-center gap-2 border-b border-border px-4 py-3">
          <Icon className="size-4 shrink-0 text-muted-foreground" />
          <h2 className="min-w-0 flex-1 truncate text-sm font-medium" title={asset.path}>
            {fileName(asset.path)}
          </h2>
          <Button variant="ghost" size="icon-sm" aria-label="Close" onClick={onClose}>
            <X />
          </Button>
        </div>

        <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-4">
          {error ? (
            <Note tone="error">{error}</Note>
          ) : !file ? (
            <div className="flex h-40 items-center justify-center text-sm text-muted-foreground">Loading…</div>
          ) : file.truncated ? (
            <Note tone="info">Too large to preview.</Note>
          ) : asset.kind === "image" ? (
            <ImagePreview src={dataUrl(file.mime || "image/png", file.base64)} />
          ) : asset.kind === "audio" ? (
            <AudioPreview mime={file.mime || "audio/wav"} base64={file.base64} />
          ) : asset.kind === "model3d" && (ext === "glb" || ext === "gltf") ? (
            <Suspense fallback={<div className="flex h-40 items-center justify-center text-sm text-muted-foreground">Loading the 3D viewer…</div>}>
              <ModelPreview key={asset.path} base64={file.base64} extension={ext} />
            </Suspense>
          ) : (
            <Note tone="info">
              There's no preview for {KIND_LABEL[asset.kind].toLowerCase()} files like this one yet. It's still part of your
              game.
            </Note>
          )}

          <dl className="grid grid-cols-[6rem_1fr] gap-x-3 gap-y-2 text-sm">
            <dt className="text-muted-foreground">File</dt>
            <dd className="font-mono text-xs break-all">{asset.path}</dd>
            <dt className="text-muted-foreground">Size</dt>
            <dd>
              {formatBytes(asset.size_bytes)}
              {dims ? ` · ${dims} pixels` : ""}
            </dd>
            <dt className="text-muted-foreground">License</dt>
            <dd className="flex flex-col items-start gap-1.5">
              <LicenseBadge license={license} />
              {hasLicense(license) ? (
                <div className="flex flex-col gap-0.5 text-xs text-muted-foreground">
                  {license?.source && <span>From: {license.source}</span>}
                  {license?.author && <span>By: {license.author}</span>}
                  {license?.generated_by && <span>Generated by: {license.generated_by}</span>}
                  {license?.url && <span className="break-all">Link: {license.url}</span>}
                </div>
              ) : (
                <p className="text-xs text-muted-foreground">
                  No license is recorded for this file. Add a license
                  {asset.card ? (
                    <>
                      {" "}
                      on its card in Context (<span className="font-mono">{asset.card}</span>)
                    </>
                  ) : (
                    " on an Asset card in Context"
                  )}
                  , so you know you're allowed to ship it.
                </p>
              )}
            </dd>
            <dt className="text-muted-foreground">Used by</dt>
            <dd>
              {asset.used_by.length === 0 ? (
                <span className="text-muted-foreground">Nothing uses this file yet.</span>
              ) : (
                <ul className="flex flex-col gap-0.5 font-mono text-xs">
                  {asset.used_by.map((u) => (
                    <li key={u} className="break-all">
                      {u}
                    </li>
                  ))}
                </ul>
              )}
            </dd>
          </dl>
        </div>
      </motion.aside>
    </motion.div>
  );
}
