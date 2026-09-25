import { useEffect, useState } from "react";
import { motion } from "motion/react";
import { AlertCircle, Download, Loader2 } from "lucide-react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { widthTransition } from "@/lib/motion";
import { godotInstall, onGodotInstallProgress } from "@/lib/studio-api";
import type { GodotStatus, InstallProgress } from "@/lib/studio-types";
import { formatBytes } from "./play-format";

interface GodotInstallCardProps {
  /** Called with the real status `godot_install` returns once it finishes. */
  onInstalled: (status: GodotStatus) => void;
}

/** `phase` is a free-form string in the contract. Known codes get plain
 * labels; anything else is shown as the backend sent it rather than hidden. */
const PHASE_LABELS: Record<string, string> = {
  downloading: "Downloading Godot…",
  download: "Downloading Godot…",
  verifying: "Checking the download…",
  verify: "Checking the download…",
  extracting: "Unpacking…",
  unpacking: "Unpacking…",
  unzipping: "Unpacking…",
  installing: "Setting up…",
  done: "Finishing up…",
  finished: "Finishing up…",
};

function phaseLabel(phase: string): string {
  return PHASE_LABELS[phase.trim().toLowerCase()] ?? phase;
}

/** Shown while Godot isn't installed. The progress bar is driven only by
 * real `godot-install-progress` events: a real percentage when the download
 * reported its size, an indeterminate sweep (never a made-up percentage)
 * when `total_bytes` is null. */
export function GodotInstallCard({ onInstalled }: GodotInstallCardProps) {
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<InstallProgress | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Subscribed for the card's whole life rather than per click, so a
  // progress event that lands just before `godotInstall()`'s promise
  // resolves isn't dropped by an unsubscribe race.
  useEffect(() => onGodotInstallProgress(setProgress), []);

  async function install() {
    setInstalling(true);
    setProgress(null);
    setError(null);
    try {
      const status = await godotInstall();
      onInstalled(status);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setInstalling(false);
    }
  }

  const fraction =
    progress && progress.total_bytes
      ? Math.min(1, progress.downloaded_bytes / progress.total_bytes)
      : null;

  return (
    <div className="flex flex-col gap-3 p-3">
      <div className="flex flex-col gap-1">
        <span className="text-sm font-medium">Godot isn't set up yet</span>
        <span className="text-sm text-muted-foreground">
          Godot is the free game engine your game runs in. InfinaBox can download and set it
          up for you.
        </span>
      </div>

      {installing ? (
        <div className="flex flex-col gap-1.5">
          <div
            role="progressbar"
            aria-label="Godot download"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={fraction !== null ? Math.round(fraction * 100) : undefined}
            className="relative h-1.5 w-full overflow-hidden rounded-full bg-muted"
          >
            {fraction !== null ? (
              <motion.div
                className="h-full rounded-full bg-primary"
                initial={false}
                animate={{ width: `${fraction * 100}%` }}
                transition={widthTransition}
              />
            ) : (
              <motion.div
                className="absolute inset-y-0 w-1/3 rounded-full bg-primary/70"
                initial={{ left: "-33%" }}
                animate={{ left: "100%" }}
                transition={{ duration: 1.2, ease: "easeInOut", repeat: Infinity }}
              />
            )}
          </div>
          <div className="flex items-center justify-between gap-2 text-xs text-muted-foreground">
            <span className="flex min-w-0 items-center gap-1.5">
              <Loader2 className="size-3 shrink-0 animate-spin" />
              <span className="truncate">{progress ? phaseLabel(progress.phase) : "Starting…"}</span>
            </span>
            {progress && (
              <span className="shrink-0 tabular-nums">
                {progress.total_bytes
                  ? `${formatBytes(progress.downloaded_bytes)} of ${formatBytes(progress.total_bytes)}`
                  : formatBytes(progress.downloaded_bytes)}
              </span>
            )}
          </div>
        </div>
      ) : (
        <Button type="button" className="self-start" onClick={() => void install()}>
          <Download data-icon="inline-start" />
          Install Godot
        </Button>
      )}

      {error && (
        <Alert variant="destructive">
          <AlertCircle />
          <AlertDescription className="break-words">
            Couldn't install Godot: {error}
          </AlertDescription>
        </Alert>
      )}
    </div>
  );
}
