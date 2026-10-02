import { useEffect, useState } from "react";
import { AlertCircle, Play, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { contextList, journeyGet, snapshotList } from "@/lib/studio-api";
import type { Journey, Snapshot } from "@/lib/studio-types";
import { projectFolderName } from "@/lib/project-picker";
import type { RecentProject } from "@/lib/recent-projects";

// Home's right-hand panel: what's really in the selected game, read from
// disk when it's selected. Nothing here is remembered or guessed — a folder
// that has gone away says so.

interface Facts {
  snapshots: Snapshot[];
  cards: number;
  journey: Journey | null;
}

type State =
  | { status: "loading" }
  | { status: "ready"; facts: Facts }
  | { status: "missing"; message: string };

const STAGE_LABEL: Record<string, string> = {
  idea: "Idea",
  prototype: "Prototype",
  vertical_slice: "Vertical slice",
  alpha: "Alpha",
  beta: "Beta",
  launch: "Launch",
};

// The history is read up to this many versions; a game with more shows "N+".
const SNAPSHOT_LIMIT = 200;

function when(unixSeconds: number): string {
  return new Date(unixSeconds * 1000).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}

export function ProjectInspector({
  project,
  onOpen,
  onRemove,
}: {
  project: RecentProject;
  onOpen: () => void;
  onRemove: () => void;
}) {
  const [state, setState] = useState<State>({ status: "loading" });

  useEffect(() => {
    let cancelled = false;
    setState({ status: "loading" });
    (async () => {
      try {
        const snapshots = await snapshotList(project.path, SNAPSHOT_LIMIT);
        const [cards, journey] = await Promise.all([
          contextList(project.path).then((c) => c.length, () => 0),
          journeyGet(project.path).catch(() => null),
        ]);
        if (!cancelled) setState({ status: "ready", facts: { snapshots, cards, journey } });
      } catch (err) {
        if (!cancelled) setState({ status: "missing", message: err instanceof Error ? err.message : String(err) });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [project.path]);

  const done = state.status === "ready" ? state.facts.journey?.stages.flatMap((s) => s.criteria).filter((c) => c.done).length : undefined;
  const total = state.status === "ready" ? state.facts.journey?.stages.flatMap((s) => s.criteria).length : undefined;

  return (
    <aside
      data-testid="project-inspector"
      className="flex w-80 shrink-0 flex-col gap-4 overflow-y-auto rounded-xl border border-border bg-card p-4"
    >
      <div data-tauri-drag-region className="flex flex-col gap-1">
        <h2 className="truncate text-lg font-semibold tracking-tight" title={projectFolderName(project.path)}>
          {projectFolderName(project.path)}
        </h2>
        <p className="text-xs wrap-anywhere text-muted-foreground">{project.path}</p>
      </div>

      <div className="flex gap-2">
        <Button type="button" className="flex-1" data-testid="inspector-open" onClick={onOpen} disabled={state.status === "missing"}>
          <Play />
          Open
        </Button>
        <Button type="button" variant="outline" data-testid="inspector-remove" onClick={onRemove} title="Remove from this list (keeps the folder)">
          <Trash2 />
        </Button>
      </div>

      {state.status === "loading" && (
        <div className="flex flex-col gap-2">
          <Skeleton className="h-10" />
          <Skeleton className="h-10" />
          <Skeleton className="h-10" />
        </div>
      )}

      {state.status === "missing" && (
        <div className="flex items-start gap-2 rounded-lg border border-destructive/40 bg-destructive/5 p-3 text-sm" role="alert">
          <AlertCircle className="mt-0.5 size-4 shrink-0 text-destructive" />
          <div className="flex min-w-0 flex-col gap-1">
            <span>This game can't be read right now.</span>
            <span className="text-xs wrap-anywhere text-muted-foreground">{state.message}</span>
            <span className="text-xs text-muted-foreground">If the folder was moved or deleted, remove it from the list.</span>
          </div>
        </div>
      )}

      {state.status === "ready" && (
        <dl className="flex flex-col gap-3 text-sm" data-testid="inspector-facts">
          <Fact label="Last opened" value={when(project.lastOpened / 1000)} />
          <Fact
            label="Stage"
            value={
              state.facts.journey
                ? `${STAGE_LABEL[state.facts.journey.current] ?? state.facts.journey.current} · ${done} of ${total} steps done`
                : "Not available"
            }
          />
          <Fact label="Context cards" value={String(state.facts.cards)} />
          <Fact
            label="Saved versions"
            value={
              state.facts.snapshots.length === 0
                ? "None yet"
                : `${state.facts.snapshots.length}${state.facts.snapshots.length >= SNAPSHOT_LIMIT ? "+" : ""}`
            }
          />
          {state.facts.snapshots[0] && (
            <Fact
              label="Latest change"
              value={`${state.facts.snapshots[0].title} — ${when(state.facts.snapshots[0].timestamp)}`}
            />
          )}
        </dl>
      )}

    </aside>
  );
}

function Fact({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex flex-col gap-0.5">
      <dt className="text-xs text-muted-foreground">{label}</dt>
      <dd className="wrap-anywhere text-foreground/90">{value}</dd>
    </div>
  );
}
