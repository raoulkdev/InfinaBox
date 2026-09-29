import { useEffect, useMemo, useState } from "react";
import { Background, Controls, MarkerType, Panel, ReactFlow, type Edge, type Node } from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { Button } from "@/components/ui/button";
import { contextGraph } from "@/lib/studio-api";
import type { CardSummary, CardType, LinkGraph } from "@/lib/studio-types";
import { CARD_TYPES, errorText, typeInfo } from "./cardTypes";

interface MapTabProps {
  projectPath: string;
  refreshTick: number;
  /** Open a card in the Cards tab. */
  onOpen: (path: string) => void;
}

const NODE_W = 176;
const NODE_H = 40;
const GAP_X = 24;
const GAP_Y = 72;
const PER_ROW = 6;
const BROKEN = "#f87171";

// Rows, top to bottom. The concept sits alone at the top; the four kinds of
// thing in the game follow; everything else goes below them.
const ROWS: CardType[][] = [
  ["concept"],
  ["mechanic"],
  ["character"],
  ["level"],
  ["story"],
  ["style-guide", "asset", "task", "playtest", "other"],
];

/** A deterministic layout: one band per row above, cards centred and
 * wrapped after six across; missing link targets sit in a ghost row at the
 * bottom. */
export function layoutGraph(graph: LinkGraph): { nodes: Node[]; edges: Edge[] } {
  const nodes: Node[] = [];
  let y = 0;

  const placeRow = (items: { id: string; data: Record<string, unknown>; style: React.CSSProperties }[]) => {
    for (let start = 0; start < items.length; start += PER_ROW) {
      const chunk = items.slice(start, start + PER_ROW);
      const width = chunk.length * NODE_W + (chunk.length - 1) * GAP_X;
      chunk.forEach((item, i) => {
        nodes.push({
          id: item.id,
          position: { x: -width / 2 + i * (NODE_W + GAP_X), y },
          data: item.data,
          style: item.style,
          draggable: false,
          connectable: false,
        });
      });
      y += NODE_H + GAP_Y;
    }
  };

  const byType = (types: CardType[]): CardSummary[] =>
    graph.nodes.filter((n) => types.includes(n.card_type)).sort((a, b) => a.title.localeCompare(b.title));

  for (const types of ROWS) {
    placeRow(
      byType(types).map((card) => {
        const info = typeInfo(card.card_type);
        return {
          id: card.path,
          data: { label: card.title },
          style: {
            width: NODE_W,
            height: NODE_H,
            border: `1.5px solid ${info.color}`,
            background: `${info.color}26`,
            color: "#f1f5f9",
            borderRadius: 10,
            fontSize: 12,
            padding: "0 8px",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            textAlign: "center",
            overflow: "hidden",
            cursor: "pointer",
          },
        };
      }),
    );
  }

  const missing = [...new Set(graph.edges.filter((e) => e.broken).map((e) => e.to))].sort();
  placeRow(
    missing.map((target) => ({
      id: ghostId(target),
      data: { label: target },
      style: {
        width: NODE_W,
        height: NODE_H,
        border: `1.5px dashed ${BROKEN}`,
        background: "transparent",
        color: BROKEN,
        borderRadius: 10,
        fontSize: 11,
        fontFamily: "ui-monospace, monospace",
        padding: "0 8px",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        textAlign: "center",
        overflow: "hidden",
        cursor: "default",
      },
    })),
  );

  const known = new Set(graph.nodes.map((n) => n.path));
  const edges: Edge[] = graph.edges
    .filter((e) => known.has(e.from))
    .map((e) => {
      const color = e.broken ? BROKEN : "#94a3b8";
      return {
        id: `${e.from}->${e.to}`,
        source: e.from,
        target: e.broken ? ghostId(e.to) : e.to,
        markerEnd: { type: MarkerType.ArrowClosed, color },
        style: { stroke: color, strokeWidth: 1.5, strokeDasharray: e.broken ? "5 4" : undefined },
      };
    });

  return { nodes, edges };
}

const ghostId = (target: string) => `missing:${target}`;

/** A read-only picture of how cards link to each other. Click a card to
 * open it. Links to cards that don't exist are dashed red, pointing at a
 * ghost box that names the missing path. */
export function MapTab({ projectPath, refreshTick, onOpen }: MapTabProps) {
  const [graph, setGraph] = useState<LinkGraph | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [retry, setRetry] = useState(0);

  useEffect(() => {
    let cancelled = false;
    contextGraph(projectPath).then(
      (g) => {
        if (cancelled) return;
        setGraph(g);
        setError(null);
      },
      (err) => {
        if (!cancelled) setError(errorText(err));
      },
    );
    return () => {
      cancelled = true;
    };
  }, [projectPath, refreshTick, retry]);

  const laid = useMemo(() => (graph ? layoutGraph(graph) : null), [graph]);
  // Re-fit the view only when the shape of the map changes.
  const shapeKey = graph ? `${graph.nodes.map((n) => n.path).join("|")}#${graph.edges.length}` : "";

  return (
    <div className="flex h-full min-h-0 w-full min-w-0 flex-col rounded-xl border border-border bg-card" data-testid="map-tab">
      <div data-tauri-drag-region className="flex h-9 shrink-0 items-center px-3">
        <span data-tauri-drag-region className="text-xs font-medium tracking-wide text-muted-foreground">
          Map of links between cards
        </span>
      </div>
      <div className="relative min-h-0 flex-1 overflow-hidden rounded-b-xl border-t border-border">
        {error ? (
          <div role="alert" className="m-3 rounded-lg bg-destructive/10 p-3 text-xs text-destructive">
            <p className="break-words">{error}</p>
            <Button size="xs" variant="outline" className="mt-2" onClick={() => setRetry((n) => n + 1)}>
              Try again
            </Button>
          </div>
        ) : !laid || !graph ? (
          <p className="p-6 text-center text-xs text-muted-foreground">Loading the map…</p>
        ) : graph.nodes.length === 0 ? (
          <p className="p-6 text-center text-sm text-muted-foreground">No cards yet. Make some in the Cards tab.</p>
        ) : (
          <>
            <ReactFlow
              key={shapeKey}
              nodes={laid.nodes}
              edges={laid.edges}
              nodesDraggable={false}
              nodesConnectable={false}
              edgesFocusable={false}
              colorMode="dark"
              fitView
              fitViewOptions={{ padding: 0.2, maxZoom: 1 }}
              minZoom={0.2}
              proOptions={{ hideAttribution: true }}
              onNodeClick={(_, node) => {
                if (!node.id.startsWith("missing:")) onOpen(node.id);
              }}
            >
              <Background />
              <Controls showInteractive={false} />
              <Panel position="top-left">
                <Legend />
              </Panel>
            </ReactFlow>
            {graph.edges.length === 0 && (
              <p className="pointer-events-none absolute inset-x-0 bottom-4 text-center text-sm text-muted-foreground">
                No links between cards yet. Add links in a card's editor.
              </p>
            )}
          </>
        )}
      </div>
    </div>
  );
}

function Legend() {
  return (
    <div className="rounded-lg border border-border bg-card/90 p-2 text-[11px] text-muted-foreground" data-testid="map-legend">
      <ul className="grid grid-cols-2 gap-x-3 gap-y-0.5">
        {CARD_TYPES.map((t) => (
          <li key={t.id} className="flex items-center gap-1.5">
            <span className="size-2.5 rounded-sm border" style={{ borderColor: t.color, backgroundColor: `${t.color}40` }} />
            {t.label}
          </li>
        ))}
      </ul>
      <p className="mt-1.5 flex items-center gap-1.5">
        <span className="inline-block w-5 border-t-2 border-dashed" style={{ borderColor: BROKEN }} />
        <span style={{ color: BROKEN }}>Link to a missing card</span>
      </p>
    </div>
  );
}
