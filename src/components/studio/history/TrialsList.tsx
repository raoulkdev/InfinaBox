// Trials: changes the AI made in a separate copy of the game (the composer's
// "In a copy"). Each is reviewed here: see which files it changed, play the
// copy, apply the changes to the real game (saved to History like any other
// change, so Undo works) or throw the copy away. Real data from
// `sandbox_list`; nothing is shown when there are no trials.

import { useCallback, useEffect, useState } from "react";
import { ChevronDown, ChevronRight, FlaskConical, Play, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useContextMenu } from "@/lib/context-menu";
import {
  onAgentTurnFinished,
  onSandboxesChanged,
  sandboxApply,
  sandboxDiscard,
  sandboxList,
  sandboxPlay,
} from "@/lib/studio-api";
import type { Trial, TrialChange } from "@/lib/studio-types";

const KIND_LABEL: Record<TrialChange["kind"], string> = {
  added: "new",
  modified: "changed",
  removed: "removed",
};

function errorText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

export function TrialsList({
  projectPath,
  aiWorking,
  onNotice,
}: {
  projectPath: string;
  aiWorking: boolean;
  onNotice: (tone: "info" | "error", text: string) => void;
}) {
  const [trials, setTrials] = useState<Trial[]>([]);
  const [open, setOpen] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [tick, setTick] = useState(0);
  const refresh = useCallback(() => setTick((t) => t + 1), []);
  const menu = useContextMenu();

  useEffect(() => {
    const offA = onSandboxesChanged(refresh);
    const offB = onAgentTurnFinished(refresh);
    return () => {
      offA();
      offB();
    };
  }, [refresh]);

  useEffect(() => {
    let cancelled = false;
    sandboxList(projectPath).then(
      (list) => !cancelled && setTrials(list),
      () => !cancelled && setTrials([]),
    );
    return () => {
      cancelled = true;
    };
  }, [projectPath, tick]);

  if (trials.length === 0) return null;

  async function apply(trial: Trial) {
    setBusy(trial.id);
    try {
      const { report, snapshot } = await sandboxApply(projectPath, trial.id);
      const left = report.conflicts.length;
      if (report.applied.length === 0 && left === 0) {
        onNotice("info", "That trial changed nothing.");
      } else if (left > 0) {
        onNotice(
          "info",
          `Applied ${report.applied.length} file${report.applied.length === 1 ? "" : "s"}. ${left} left as they are, because you changed ${left === 1 ? "it" : "them"} too: ${report.conflicts.join(", ")}.`,
        );
      } else {
        onNotice("info", `Applied "${trial.title}"${snapshot ? " (saved to your history)" : ""}.`);
      }
      // Fully applied: the copy has done its job.
      if (left === 0) await sandboxDiscard(trial.id);
    } catch (err) {
      onNotice("error", `Couldn't apply it: ${errorText(err)}`);
    } finally {
      setBusy(null);
      refresh();
    }
  }

  async function discard(trial: Trial) {
    setBusy(trial.id);
    try {
      await sandboxDiscard(trial.id);
      onNotice("info", `Threw away "${trial.title}".`);
    } catch (err) {
      onNotice("error", `Couldn't throw it away: ${errorText(err)}`);
    } finally {
      setBusy(null);
      refresh();
    }
  }

  async function play(trial: Trial) {
    try {
      await sandboxPlay(trial.id);
    } catch (err) {
      onNotice("error", `Couldn't play the copy: ${errorText(err)}`);
    }
  }

  return (
    <section data-testid="trials" className="border-b border-border">
      <h3 className="flex items-center gap-1.5 px-3 pt-2 text-xs font-medium tracking-wide text-muted-foreground">
        <FlaskConical className="size-3.5" aria-hidden />
        Trials in a copy
      </h3>
      <ul>
        {trials.map((trial) => {
          const expanded = open === trial.id;
          const n = trial.changes.length;
          const conflicts = trial.changes.filter((c) => c.conflict).length;
          return (
            <li
              key={trial.id}
              data-testid="trial"
              data-trial-id={trial.id}
              onContextMenu={(e) =>
                menu(e, [
                  { label: "Play the copy", icon: <Play />, onSelect: () => void play(trial) },
                  { label: "Apply to my game", disabled: aiWorking || busy !== null || n === 0, onSelect: () => void apply(trial) },
                  { label: "Throw away", icon: <Trash2 />, disabled: busy !== null, onSelect: () => void discard(trial) },
                ])
              }
              className="px-3 py-2"
            >
              <button
                type="button"
                data-testid="trial-toggle"
                onClick={() => setOpen(expanded ? null : trial.id)}
                className="flex w-full min-w-0 items-center gap-1.5 text-left"
              >
                {expanded ? <ChevronDown className="size-3.5 shrink-0" /> : <ChevronRight className="size-3.5 shrink-0" />}
                <span className="min-w-0 truncate text-sm text-foreground/90">{trial.title}</span>
              </button>
              <p className="pl-5 text-xs text-muted-foreground" data-testid="trial-summary">
                {n === 0 ? "No changes yet" : `${n} file${n === 1 ? "" : "s"} changed`}
                {conflicts > 0 && ` · ${conflicts} also changed in your game`}
              </p>
              {expanded && n > 0 && (
                <ul className="mt-1 flex flex-col gap-0.5 pl-5 text-xs" data-testid="trial-files">
                  {trial.changes.map((c) => (
                    <li key={c.path} className="flex min-w-0 gap-1.5">
                      <span className="shrink-0 text-muted-foreground">{KIND_LABEL[c.kind]}</span>
                      <span className="min-w-0 truncate" title={c.path}>
                        {c.path}
                      </span>
                      {c.conflict && <span className="shrink-0 text-destructive">also changed in your game</span>}
                    </li>
                  ))}
                </ul>
              )}
              <div className="mt-1.5 flex gap-1.5 pl-5">
                <Button type="button" size="xs" variant="outline" data-testid="trial-play" disabled={busy !== null} onClick={() => void play(trial)}>
                  <Play data-icon="inline-start" />
                  Play the copy
                </Button>
                <span title={aiWorking ? "Wait for the AI to finish" : undefined}>
                  <Button
                    type="button"
                    size="xs"
                    data-testid="trial-apply"
                    disabled={aiWorking || busy !== null || n === 0}
                    onClick={() => void apply(trial)}
                  >
                    Apply to my game
                  </Button>
                </span>
                <Button type="button" size="xs" variant="ghost" data-testid="trial-discard" disabled={busy !== null} onClick={() => void discard(trial)}>
                  <Trash2 data-icon="inline-start" />
                  Throw away
                </Button>
              </div>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
