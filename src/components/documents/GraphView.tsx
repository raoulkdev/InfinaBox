import { useContextMenu } from "@/lib/context-menu";
import { useEffect, useMemo, useState } from "react";
import { Background, Controls, Handle, MarkerType, MiniMap, Position, ReactFlow, type Edge, type Node, type NodeProps } from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { FileText, Layers, StickyNote } from "lucide-react";
import { contextGraph } from "@/lib/studio-api";
import type { CardSummary, LinkGraph } from "@/lib/studio-types";
import { cn } from "@/lib/utils";
import { NoteIcon } from "@/components/context/note-icons";
import { forceLayout } from "./layout";
import { BLOCK_LABELS, blockText, type Boards } from "./types";

// The same boards, drawn as a graph: documents and blocks as nodes; arrows
// and [[links]] between documents as edges. Click a node to land on it.

export type GraphTarget = { kind: "block"; boardId: string; blockId: string } | { kind: "board"; boardId: string } | { kind: "doc"; path: string };

interface NodeData extends Record<string, unknown> {
  label: string;
  kind: string;
  icon?: string | null;
  target: GraphTarget;
}

function GraphNode({ data, selected }: NodeProps<Node<NodeData>>) {
  const kind = data.kind;
  const Icon = kind === "board" ? Layers : kind === "doc" ? FileText : StickyNote;
  return (
    <div
      className={cn(
        "flex max-w-[190px] items-center gap-1.5 rounded-lg border bg-card px-2.5 py-1.5 text-xs shadow-sm",
        kind === "board" && "border-sky-500/60",
        kind === "doc" && "border-border",
        kind !== "board" && kind !== "doc" && "border-dashed border-border",
        selected && "ring-2 ring-primary",
      )}
    >
      <Handle type="target" position={Position.Top} className="!opacity-0" />
      {data.icon ? <NoteIcon value={data.icon} className="size-3.5 text-xs" /> : <Icon className="size-3.5 shrink-0 text-muted-foreground" />}
      <span className="truncate">{data.label || "Untitled"}</span>
      <Handle type="source" position={Position.Bottom} className="!opacity-0" />
    </div>
  );
}

const nodeTypes = { item: GraphNode };

export function GraphView({
  projectPath,
  boards,
  cards,
  tick,
  onPick,
}: {
  projectPath: string;
  boards: Boards;
  cards: CardSummary[];
  tick: number;
  onPick: (target: GraphTarget) => void;
}) {
  const [links, setLinks] = useState<LinkGraph | null>(null);
  const [showBlocks, setShowBlocks] = useState(true);
  const [showNesting, setShowNesting] = useState(true);
  const menu = useContextMenu();

  useEffect(() => {
    let cancelled = false;
    contextGraph(projectPath).then(
      (g) => !cancelled && setLinks(g),
      () => !cancelled && setLinks({ nodes: [], edges: [] }),
    );
    return () => {
      cancelled = true;
    };
  }, [projectPath, tick]);

  const { nodes, edges } = useMemo(() => {
    const cardMap = new Map(cards.map((c) => [c.path, c]));
    const nodeList: { id: string; data: NodeData }[] = [];
    const edgeList: { id: string; a: string; b: string; kind: "arrow" | "link" | "nest"; label?: string }[] = [];
    const nodeOf = new Map<string, string>();
    const seen = new Set<string>();
    const addNode = (id: string, data: NodeData) => {
      if (seen.has(id)) return;
      seen.add(id);
      nodeList.push({ id, data });
    };
    for (const c of cards) addNode(`doc:${c.path}`, { label: c.title, kind: "doc", icon: c.icon, target: { kind: "doc", path: c.path } });
    for (const board of Object.values(boards)) {
      addNode(`board:${board.id}`, { label: board.title, kind: "board", icon: board.icon, target: { kind: "board", boardId: board.id } });
      for (const b of board.blocks) {
        if (b.type === "arrow") continue;
        if (b.type === "doc") {
          nodeOf.set(b.id, `doc:${b.ref}`);
          if (!cardMap.has(b.ref)) addNode(`doc:${b.ref}`, { label: b.ref, kind: "doc", target: { kind: "block", boardId: board.id, blockId: b.id } });
          continue;
        }
        if (b.type === "board") {
          nodeOf.set(b.id, `board:${b.ref}`);
          continue;
        }
        if (!showBlocks) continue;
        const text = blockText(b).replace(/\s+/g, " ").trim();
        addNode(`block:${b.id}`, { label: text ? text.slice(0, 40) : BLOCK_LABELS[b.type], kind: b.type, target: { kind: "block", boardId: board.id, blockId: b.id } });
        nodeOf.set(b.id, `block:${b.id}`);
      }
    }
    for (const board of Object.values(boards)) {
      for (const b of board.blocks) {
        if (b.type === "arrow") {
          const from = nodeOf.get(b.from);
          const to = nodeOf.get(b.to);
          if (from && to && from !== to) edgeList.push({ id: `arrow:${b.id}`, a: from, b: to, kind: "arrow", label: b.label });
        } else if (showNesting) {
          const to = nodeOf.get(b.id);
          const parent = b.col ? nodeOf.get(b.col) : `board:${board.id}`;
          if (to && parent && to !== parent && !(b.type === "doc" && !cardMap.has(b.ref))) edgeList.push({ id: `nest:${board.id}:${b.id}`, a: parent, b: to, kind: "nest" });
        }
      }
    }
    for (const e of links?.edges ?? []) {
      if (e.broken) continue;
      edgeList.push({ id: `link:${e.from}>${e.to}`, a: `doc:${e.from}`, b: `doc:${e.to}`, kind: "link" });
    }
    const valid = edgeList.filter((e) => seen.has(e.a) && seen.has(e.b));
    const pos = forceLayout(
      nodeList.map((n) => n.id),
      valid.map((e) => ({ a: e.a, b: e.b })),
    );
    return {
      nodes: nodeList.map<Node<NodeData>>((n) => ({ id: n.id, type: "item", position: pos.get(n.id) ?? { x: 0, y: 0 }, data: n.data })),
      edges: valid.map<Edge>((e) => ({
        id: e.id,
        source: e.a,
        target: e.b,
        label: e.label || undefined,
        markerEnd: e.kind === "nest" ? undefined : { type: MarkerType.ArrowClosed },
        style:
          e.kind === "nest"
            ? { strokeDasharray: "3 4", opacity: 0.35 }
            : e.kind === "link"
              ? { stroke: "#38bdf8" }
              : undefined,
      })),
    };
  }, [boards, cards, links, showBlocks, showNesting]);

  const chip = (on: boolean) =>
    cn("rounded-md border px-2 py-1 text-xs", on ? "border-foreground/40 bg-foreground/10 text-foreground" : "border-border text-muted-foreground hover:text-foreground");

  return (
    <div data-testid="graph-view" className="relative size-full overflow-hidden rounded-xl border border-border bg-background">
      <div className="absolute left-3 top-3 z-10 flex gap-1.5">
        <button type="button" className={chip(showBlocks)} onClick={() => setShowBlocks((v) => !v)} data-testid="graph-toggle-blocks">
          Blocks
        </button>
        <button type="button" className={chip(showNesting)} onClick={() => setShowNesting((v) => !v)} data-testid="graph-toggle-nesting">
          Nesting
        </button>
        <span className="self-center pl-1 text-xs text-muted-foreground" data-testid="graph-count">
          {nodes.length} nodes · {edges.length} edges
        </span>
      </div>
      <ReactFlow
        key={nodes.length + ":" + edges.length}
        nodes={nodes}
        edges={edges}
        nodeTypes={nodeTypes}
        colorMode="dark"
        fitView
        minZoom={0.1}
        nodesConnectable={false}
        proOptions={{ hideAttribution: true }}
        onNodeClick={(_, node) => onPick((node.data as NodeData).target)}
        onNodeContextMenu={(e, node) => menu(e, [{ label: "Open", testId: "ctx-open", onSelect: () => onPick((node.data as NodeData).target) }, { label: "Copy name", onSelect: () => void navigator.clipboard.writeText((node.data as NodeData).label).catch(() => {}) }])}
        onPaneContextMenu={(e) =>
          menu(e as unknown as React.MouseEvent, [
            { label: showBlocks ? "Hide blocks" : "Show blocks", onSelect: () => setShowBlocks((v) => !v) },
            { label: showNesting ? "Hide nesting lines" : "Show nesting lines", onSelect: () => setShowNesting((v) => !v) },
          ])
        }
      >
        <Background />
        <Controls showInteractive={false} />
        <MiniMap pannable zoomable />
      </ReactFlow>
    </div>
  );
}
