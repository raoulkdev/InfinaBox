import { useContextMenu } from "@/lib/context-menu";
import { useCallback, useEffect, useRef, useState } from "react";
import { AlertCircle, Check, PartyPopper, RefreshCw, Sparkles } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { debounce } from "@/lib/debounce";
import { onProjectFilesChanged } from "@/lib/fs-watch";
import { journeyGet, journeySetManual, onSnapshotsChanged } from "@/lib/studio-api";
import type { Criterion, Journey, Stage } from "@/lib/studio-types";
import { cn } from "@/lib/utils";

// The Producer's journey from Idea to Launch: six stages, each with a
// checklist. Everything shown comes from `journey_get` — automatic criteria
// are read-only and carry the evidence the backend found; only "manual"
// criteria can be ticked here (through `journey_set_manual`). Nothing is
// ever marked done on this side except that optimistic manual tick, which is
// rolled back if the backend refuses it.

export interface JourneyPanelProps {
  projectPath: string;
  /** Sends `message` to the chat as the Producer role. */
  onAskProducer: (message: string) => void;
}

type LoadState =
  | { status: "loading" }
  | { status: "ready"; journey: Journey }
  | { status: "error"; message: string };

const STAGE_LABELS: Record<Stage, string> = {
  idea: "Idea",
  prototype: "Prototype",
  vertical_slice: "Vertical slice",
  alpha: "Alpha",
  beta: "Beta",
  launch: "Launch",
};

export function JourneyPanel({ projectPath, onAskProducer }: JourneyPanelProps) {
  const [state, setState] = useState<LoadState>({ status: "loading" });
  const [refreshing, setRefreshing] = useState(false);
  // The stage whose checklist is open; null follows the current stage.
  const [picked, setPicked] = useState<Stage | null>(null);
  const [toggleError, setToggleError] = useState<string | null>(null);
  const [pendingIds, setPendingIds] = useState<ReadonlySet<string>>(new Set());
  // Drops a load that finishes after the project changed or a newer request
  // started, and any load that lands while a tick is still being saved (the
  // tick's own answer is the newer truth).
  const seq = useRef(0);
  const inFlightToggles = useRef(0);

  const load = useCallback(
    async (initial: boolean) => {
      const mine = ++seq.current;
      if (initial) setState({ status: "loading" });
      else setRefreshing(true);
      try {
        const journey = await journeyGet(projectPath);
        if (mine === seq.current && inFlightToggles.current === 0) {
          setState({ status: "ready", journey });
          setToggleError(null);
        }
      } catch (err) {
        if (mine === seq.current) {
          // A failed background refresh keeps what's on screen; the person
          // can still see the last real data and hit Refresh.
          setState((prev) =>
            initial || prev.status !== "ready" ? { status: "error", message: String(err) } : prev,
          );
          if (!initial) setToggleError(String(err));
        }
      } finally {
        if (mine === seq.current) setRefreshing(false);
      }
    },
    [projectPath],
  );

  // Load on mount and whenever the project changes.
  useEffect(() => {
    setPicked(null);
    setToggleError(null);
    void load(true);
    return () => {
      seq.current++;
    };
  }, [load]);

  // Files or history changed: re-read (debounced so a burst is one read).
  useEffect(() => {
    const refresh = debounce(() => void load(false), 800);
    const offFiles = onProjectFilesChanged(refresh);
    const offSnaps = onSnapshotsChanged(refresh);
    return () => {
      refresh.cancel();
      offFiles();
      offSnaps();
    };
  }, [load]);

  async function toggle(criterion: Criterion) {
    if (state.status !== "ready" || criterion.signal !== "manual" || pendingIds.has(criterion.id)) return;
    const before = state.journey;
    const done = !criterion.done;
    setToggleError(null);
    setState({ status: "ready", journey: withCriterionDone(before, criterion.id, done) });
    setPendingIds((prev) => new Set(prev).add(criterion.id));
    inFlightToggles.current++;
    seq.current++; // any load already under way is now stale
    try {
      const journey = await journeySetManual(projectPath, criterion.id, done);
      setState({ status: "ready", journey });
    } catch (err) {
      setState({ status: "ready", journey: before });
      setToggleError(String(err));
    } finally {
      inFlightToggles.current--;
      setRefreshing(false);
      setPendingIds((prev) => {
        const next = new Set(prev);
        next.delete(criterion.id);
        return next;
      });
    }
  }

  const journey = state.status === "ready" ? state.journey : null;
  const open: Stage | null = journey ? (picked ?? journey.current) : null;
  const openStage = journey?.stages.find((s) => s.stage === open) ?? null;

  const menu = useContextMenu();
  return (
    <section
      onContextMenu={(e) => menu(e, [{ label: "Refresh", disabled: refreshing, onSelect: () => void load(false) }])}
      data-testid="journey-panel"
      aria-label="Your game's journey"
      className="flex min-w-0 flex-col gap-3 rounded-xl border border-border bg-card p-4"
    >
      <div className="flex items-center justify-between gap-2">
        <h2 className="text-sm font-medium tracking-wide text-muted-foreground">Your game's journey</h2>
        <Button
          type="button"
          size="xs"
          variant="ghost"
          className="text-muted-foreground"
          disabled={state.status === "loading" || refreshing}
          onClick={() => void load(false)}
          data-testid="journey-refresh"
        >
          <RefreshCw className={cn(refreshing && "animate-spin")} />
          Refresh
        </Button>
      </div>

      {state.status === "loading" && <JourneySkeleton />}

      {state.status === "error" && (
        <div
          role="alert"
          data-testid="journey-error"
          className="flex flex-col items-start gap-2 rounded-lg border border-destructive/30 bg-destructive/5 p-3"
        >
          <span className="flex items-center gap-1.5 text-sm font-medium">
            <AlertCircle className="size-4 text-destructive" />
            Couldn't load your game's journey
          </span>
          <span className="font-mono text-xs break-words text-muted-foreground">{state.message}</span>
          <Button type="button" size="xs" variant="outline" onClick={() => void load(true)}>
            <RefreshCw />
            Retry
          </Button>
        </div>
      )}

      {journey && (
        <>
          <Stepper journey={journey} open={open} onPick={setPicked} />

          <NextStepCard journey={journey} onAskProducer={onAskProducer} />

          {toggleError && (
            <div
              role="alert"
              data-testid="journey-toggle-error"
              className="flex flex-col gap-1 rounded-lg border border-destructive/30 bg-destructive/5 p-2.5"
            >
              <span className="flex items-center gap-1.5 text-xs font-medium">
                <AlertCircle className="size-3.5 text-destructive" />
                Couldn't update the journey
              </span>
              <span className="font-mono text-[11px] break-words text-muted-foreground">{toggleError}</span>
            </div>
          )}

          {openStage && (
            <div className="flex min-h-0 flex-col gap-1.5" data-testid="journey-checklist">
              <div className="flex items-baseline justify-between gap-2">
                <h3 className="text-sm font-medium">{openStage.title || STAGE_LABELS[openStage.stage]}</h3>
                <span className="text-xs text-muted-foreground" data-testid="journey-progress">
                  {progressHint(openStage.criteria, STAGE_LABELS[openStage.stage])}
                </span>
              </div>
              {openStage.criteria.length === 0 ? (
                <p className="text-xs text-muted-foreground">Nothing to check in this stage yet.</p>
              ) : (
                <ul className="flex max-h-60 flex-col gap-0.5 overflow-y-auto pr-1">
                  {openStage.criteria.map((c) => (
                    <CriterionRow
                      key={c.id}
                      criterion={c}
                      pending={pendingIds.has(c.id)}
                      onToggle={() => void toggle(c)}
                    />
                  ))}
                </ul>
              )}
            </div>
          )}
        </>
      )}
    </section>
  );
}

/** A copy of `journey` with one criterion's `done` flipped — used only for the
 * optimistic manual tick; the backend's answer replaces it. */
function withCriterionDone(journey: Journey, id: string, done: boolean): Journey {
  return {
    ...journey,
    stages: journey.stages.map((s) => ({
      ...s,
      criteria: s.criteria.map((c) => (c.id === id ? { ...c, done } : c)),
    })),
  };
}

function progressHint(criteria: Criterion[], stageLabel: string): string {
  if (criteria.length === 0) return "";
  const done = criteria.filter((c) => c.done).length;
  return `${done} of ${criteria.length} ${criteria.length === 1 ? "step" : "steps"} done in ${stageLabel}`;
}

function Stepper({
  journey,
  open,
  onPick,
}: {
  journey: Journey;
  open: Stage | null;
  onPick: (stage: Stage) => void;
}) {
  return (
    <ol className="flex items-start" aria-label="Stages">
      {journey.stages.map((s, i) => {
        const current = s.stage === journey.current;
        const isOpen = s.stage === open;
        const label = STAGE_LABELS[s.stage];
        const doneCount = s.criteria.filter((c) => c.done).length;
        return (
          <li key={s.stage} className="flex min-w-0 flex-1 items-start">
            <button
              type="button"
              onClick={() => onPick(s.stage)}
              aria-current={current ? "step" : undefined}
              aria-expanded={isOpen}
              data-testid={`journey-stage-${s.stage}`}
              title={`${label}: ${doneCount} of ${s.criteria.length} done${s.complete ? " (complete)" : ""}`}
              className={cn(
                "group flex w-full min-w-0 flex-col items-center gap-1 rounded-lg px-1 py-1 outline-none focus-visible:ring-2 focus-visible:ring-ring/50",
                isOpen ? "bg-muted/60" : "hover:bg-muted/40",
              )}
            >
              <span
                className={cn(
                  "flex size-6 items-center justify-center rounded-full border text-xs font-medium",
                  s.complete
                    ? "border-emerald-500/60 bg-emerald-500/15 text-emerald-600 dark:text-emerald-400"
                    : current
                      ? "border-foreground bg-foreground text-background"
                      : "border-border text-muted-foreground",
                )}
              >
                {s.complete ? <Check className="size-3.5" aria-label="Complete" /> : i + 1}
              </span>
              <span
                className={cn(
                  "w-full truncate text-center text-[11px] leading-tight",
                  current ? "font-medium text-foreground" : "text-muted-foreground",
                )}
              >
                {label}
              </span>
            </button>
          </li>
        );
      })}
    </ol>
  );
}

function NextStepCard({
  journey,
  onAskProducer,
}: {
  journey: Journey;
  onAskProducer: (message: string) => void;
}) {
  const next = journey.next_step;
  if (!next) {
    return (
      <div
        data-testid="journey-all-done"
        className="flex items-start gap-2.5 rounded-lg border border-emerald-500/30 bg-emerald-500/5 p-3"
      >
        <PartyPopper className="mt-0.5 size-4 shrink-0 text-emerald-600 dark:text-emerald-400" />
        <div className="flex flex-col gap-0.5">
          <span className="text-sm font-medium">You've reached Launch</span>
          <span className="text-xs text-muted-foreground">Everything on the checklist is ticked.</span>
        </div>
      </div>
    );
  }
  // The reason comes straight from the criterion's own evidence.
  const evidence = journey.stages
    .flatMap((s) => s.criteria)
    .find((c) => c.id === next.criterion_id)?.evidence;
  return (
    <div
      data-testid="journey-next-step"
      className="flex flex-wrap items-center justify-between gap-x-3 gap-y-2 rounded-lg border border-border bg-muted/40 p-3"
    >
      <div className="flex min-w-0 flex-1 basis-56 flex-col gap-0.5">
        <span className="text-[11px] font-medium tracking-wide text-muted-foreground uppercase">Next step</span>
        <span className="text-sm font-medium">{next.title}</span>
        {evidence && <span className="text-xs text-muted-foreground">{evidence}</span>}
      </div>
      <Button
        type="button"
        size="sm"
        onClick={() => onAskProducer(next.ask)}
        data-testid="journey-ask-producer"
      >
        <Sparkles />
        Ask the Producer
      </Button>
    </div>
  );
}

function CriterionRow({
  criterion,
  pending,
  onToggle,
}: {
  criterion: Criterion;
  pending: boolean;
  onToggle: () => void;
}) {
  const manual = criterion.signal === "manual";
  return (
    <li
      data-testid={`journey-criterion-${criterion.id}`}
      className="flex items-start gap-2.5 rounded-lg px-1.5 py-1.5 hover:bg-muted/30"
    >
      <button
        type="button"
        role="checkbox"
        aria-checked={criterion.done}
        aria-label={criterion.title}
        aria-disabled={!manual || pending}
        disabled={!manual}
        onClick={onToggle}
        className={cn(
          "mt-0.5 flex size-4 shrink-0 items-center justify-center rounded border outline-none focus-visible:ring-2 focus-visible:ring-ring/50",
          criterion.done
            ? "border-foreground bg-foreground text-background"
            : "border-foreground/30 bg-background",
          manual ? "cursor-pointer" : "cursor-default opacity-80",
          pending && "opacity-60",
        )}
      >
        {criterion.done && <Check className="size-3" strokeWidth={3} />}
      </button>
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
          <span className={cn("text-sm", criterion.done && "text-muted-foreground")}>{criterion.title}</span>
          {!manual && (
            <span className="rounded-full border border-border px-1.5 text-[10px] leading-4 text-muted-foreground">
              Automatic
            </span>
          )}
        </div>
        {criterion.evidence && (
          <span className="text-xs break-words text-muted-foreground">{criterion.evidence}</span>
        )}
      </div>
    </li>
  );
}

function JourneySkeleton() {
  return (
    <div className="flex flex-col gap-3" aria-busy="true" aria-label="Loading your game's journey">
      <div className="flex justify-between gap-2">
        {Array.from({ length: 6 }, (_, i) => (
          <Skeleton key={i} className="h-10 flex-1" />
        ))}
      </div>
      <Skeleton className="h-14" />
      <Skeleton className="h-24" />
    </div>
  );
}
