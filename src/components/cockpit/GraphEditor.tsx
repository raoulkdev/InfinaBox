import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  BaseEdge,
  Background,
  Controls,
  EdgeLabelRenderer,
  Handle,
  MarkerType,
  MiniMap,
  NodeToolbar,
  Position,
  ReactFlow,
  addEdge,
  getBezierPath,
  useEdgesState,
  useNodesState,
  type Connection,
  type DefaultEdgeOptions,
  type Edge,
  type EdgeProps,
  type Node,
  type NodeProps,
  type Viewport,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { Circle, Copy, Diamond, Plus, Square, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

interface GraphEditorProps {
  /** Initial document — a JSON-serialized `{ nodes, edges }` graph. Only
   * read on mount, same uncontrolled/remount-on-key contract as
   * MarkdownEditor: give this a `key={path}` to force a remount when
   * switching files. */
  initialValue: string;
  onChange: (value: string) => void;
  /** Called for Cmd+S / Ctrl+S while the editor has focus. */
  onSave: () => void;
}

interface GraphDoc {
  nodes: Node[];
  edges: Edge[];
  viewport?: Viewport;
}

function isViewport(value: unknown): value is Viewport {
  if (!value || typeof value !== "object") return false;
  const v = value as Record<string, unknown>;
  return typeof v.x === "number" && typeof v.y === "number" && typeof v.zoom === "number";
}

function parseGraphDoc(raw: string): GraphDoc {
  if (!raw.trim()) return { nodes: [], edges: [] };
  try {
    const parsed = JSON.parse(raw) as Partial<GraphDoc>;
    return {
      nodes: Array.isArray(parsed.nodes) ? parsed.nodes : [],
      edges: Array.isArray(parsed.edges) ? parsed.edges : [],
      viewport: isViewport(parsed.viewport) ? parsed.viewport : undefined,
    };
  } catch {
    // Malformed or non-graph JSON opens as a blank canvas rather than
    // crashing the editor — the user can still overwrite it with Save.
    return { nodes: [], edges: [] };
  }
}

// React Flow attaches its own runtime bookkeeping onto the same node/edge
// objects `useNodesState`/`useEdgesState` manage — `measured` gets set the
// moment a node is first rendered and its DOM size is observed, `selected`
// flips on every click, `dragging` while a drag is in progress. None of
// that belongs in a saved document (a doc opened and never touched would
// otherwise gain a `measured` field the instant its size is observed, and
// show up as "unsaved" purely from opening it — the same class of false
// dirty state the baselineRef comment below exists for, just from a
// different source). Persist only the fields the doc actually needs to
// round-trip.
function sanitizeNode(n: Node): Pick<Node, "id" | "type" | "position" | "data"> {
  return { id: n.id, type: n.type, position: n.position, data: n.data };
}

function sanitizeEdge(e: Edge) {
  const { id, source, target, sourceHandle, targetHandle, type, markerEnd, data } = e;
  return { id, source, target, sourceHandle, targetHandle, type, markerEnd, data };
}

let nodeCounter = 0;
function makeNodeId(): string {
  nodeCounter += 1;
  return `node-${Date.now()}-${nodeCounter}`;
}

type NodeShape = "label" | "diamond" | "circle";

/** Floating duplicate/delete controls shared by every node shape — only
 * visible while its own node is selected (NodeToolbar's default behavior,
 * and it hides itself automatically when several nodes are selected). */
function ShapeToolbar({
  id,
  onDuplicate,
  onDelete,
}: {
  id: string;
  onDuplicate?: (id: string) => void;
  onDelete?: (id: string) => void;
}) {
  return (
    <NodeToolbar className="flex gap-1">
      <Button
        type="button"
        size="icon-xs"
        variant="secondary"
        title="Duplicate"
        onClick={() => onDuplicate?.(id)}
      >
        <Copy />
      </Button>
      <Button
        type="button"
        size="icon-xs"
        variant="destructive"
        title="Delete"
        onClick={() => onDelete?.(id)}
      >
        <Trash2 />
      </Button>
    </NodeToolbar>
  );
}

/** The one node shape this editor needs for general-purpose graph
 * documents (flowcharts, dependency maps, relationship diagrams): a box
 * with an inline-editable label, no modal/prompt required to rename it. */
function LabelNode({ id, data }: NodeProps) {
  const label = typeof data.label === "string" ? data.label : "";
  const onLabelChange = data.onLabelChange as (id: string, value: string) => void;
  const onDuplicate = data.onDuplicate as ((id: string) => void) | undefined;
  const onDelete = data.onDelete as ((id: string) => void) | undefined;
  return (
    <>
      <ShapeToolbar id={id} onDuplicate={onDuplicate} onDelete={onDelete} />
      <Handle type="target" position={Position.Top} />
      <div className="min-w-[130px] rounded-lg border border-border bg-card px-3 py-2 shadow-sm">
        <input
          value={label}
          onChange={(e) => onLabelChange(id, e.target.value)}
          placeholder="Untitled"
          className="nodrag w-full bg-transparent text-sm text-foreground outline-none placeholder:text-muted-foreground"
        />
      </div>
      <Handle type="source" position={Position.Bottom} />
    </>
  );
}

/** Decision-shape node — same inline-editable label as LabelNode, just
 * clipped to a diamond. clip-path also crops the border along the same
 * polygon, which is why the border reads a little differently at the
 * points than a rectangle's does; that's expected for this shape. */
function DiamondNode({ id, data }: NodeProps) {
  const label = typeof data.label === "string" ? data.label : "";
  const onLabelChange = data.onLabelChange as (id: string, value: string) => void;
  const onDuplicate = data.onDuplicate as ((id: string) => void) | undefined;
  const onDelete = data.onDelete as ((id: string) => void) | undefined;
  return (
    <>
      <ShapeToolbar id={id} onDuplicate={onDuplicate} onDelete={onDelete} />
      <Handle type="target" position={Position.Top} />
      <div
        className="flex h-[110px] w-[150px] items-center justify-center border border-border bg-card px-7 shadow-sm"
        style={{ clipPath: "polygon(50% 0%, 100% 50%, 50% 100%, 0% 50%)" }}
      >
        <input
          value={label}
          onChange={(e) => onLabelChange(id, e.target.value)}
          placeholder="Untitled"
          className="nodrag w-full bg-transparent text-center text-sm text-foreground outline-none placeholder:text-muted-foreground"
        />
      </div>
      <Handle type="source" position={Position.Bottom} />
    </>
  );
}

/** Same interaction pattern again, this time rounded into a circle via
 * border-radius rather than clip-path since a circle needs no corners. */
function CircleNode({ id, data }: NodeProps) {
  const label = typeof data.label === "string" ? data.label : "";
  const onLabelChange = data.onLabelChange as (id: string, value: string) => void;
  const onDuplicate = data.onDuplicate as ((id: string) => void) | undefined;
  const onDelete = data.onDelete as ((id: string) => void) | undefined;
  return (
    <>
      <ShapeToolbar id={id} onDuplicate={onDuplicate} onDelete={onDelete} />
      <Handle type="target" position={Position.Top} />
      <div className="flex h-[110px] w-[110px] items-center justify-center rounded-full border border-border bg-card px-5 shadow-sm">
        <input
          value={label}
          onChange={(e) => onLabelChange(id, e.target.value)}
          placeholder="Untitled"
          className="nodrag w-full bg-transparent text-center text-sm text-foreground outline-none placeholder:text-muted-foreground"
        />
      </div>
      <Handle type="source" position={Position.Bottom} />
    </>
  );
}

const nodeTypes = { label: LabelNode, diamond: DiamondNode, circle: CircleNode };

const shapeOptions: { type: NodeShape; label: string; icon: typeof Square }[] = [
  { type: "label", label: "Rectangle", icon: Square },
  { type: "diamond", label: "Diamond", icon: Diamond },
  { type: "circle", label: "Circle", icon: Circle },
];

/** A bezier edge with an inline-editable label dropped at its midpoint —
 * the same click-and-type interaction as the node labels, via
 * EdgeLabelRenderer's HTML overlay instead of an SVG <text>. Edges saved
 * before this feature existed have no `data.label`; that just renders as
 * an empty input rather than crashing. */
function LabeledEdge({
  id,
  sourceX,
  sourceY,
  sourcePosition,
  targetX,
  targetY,
  targetPosition,
  markerEnd,
  style,
  data,
}: EdgeProps) {
  const [edgePath, labelX, labelY] = getBezierPath({
    sourceX,
    sourceY,
    sourcePosition,
    targetX,
    targetY,
    targetPosition,
  });
  const label = typeof data?.label === "string" ? data.label : "";
  const onLabelChange = data?.onLabelChange as ((id: string, value: string) => void) | undefined;
  return (
    <>
      <BaseEdge id={id} path={edgePath} markerEnd={markerEnd} style={style} />
      <EdgeLabelRenderer>
        <input
          value={label}
          onChange={(e) => onLabelChange?.(id, e.target.value)}
          placeholder="Label"
          className="nodrag nopan absolute w-20 rounded border border-border bg-card px-1.5 py-0.5 text-center text-xs text-foreground shadow-sm outline-none placeholder:text-muted-foreground"
          style={{
            pointerEvents: "all",
            transform: `translate(-50%, -50%) translate(${labelX}px, ${labelY}px)`,
          }}
        />
      </EdgeLabelRenderer>
    </>
  );
}

const edgeTypes = { labeled: LabeledEdge };

// New connections get the labeled edge type and an arrowhead; this only
// applies going forward — edges saved before these features shipped keep
// rendering as plain unlabeled bezier edges (React Flow's "default" type).
const defaultEdgeOptions: DefaultEdgeOptions = {
  type: "labeled",
  markerEnd: { type: MarkerType.ArrowClosed },
};

/** A ReactFlow-backed canvas editor for `.graph.json` documents — nodes
 * and edges the user draws, connects, and renames, round-tripped to plain
 * JSON via `onChange`/`onSave` exactly like MarkdownEditor round-trips
 * plain markdown text. This is the editor GraphsSection's file browser
 * hands `.graph.json` files to, instead of MarkdownEditor. */
export function GraphEditor({ initialValue, onChange, onSave }: GraphEditorProps) {
  const initial = useMemo(() => parseGraphDoc(initialValue), [initialValue]);
  const [nodes, setNodes, onNodesChange] = useNodesState<Node>(initial.nodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>(initial.edges);
  const [viewport, setViewport] = useState<Viewport | undefined>(initial.viewport);

  const handleLabelChange = useCallback(
    (id: string, value: string) => {
      setNodes((nds) =>
        nds.map((n) => (n.id === id ? { ...n, data: { ...n.data, label: value } } : n)),
      );
    },
    [setNodes],
  );

  const handleEdgeLabelChange = useCallback(
    (id: string, value: string) => {
      setEdges((eds) =>
        eds.map((e) => (e.id === id ? { ...e, data: { ...e.data, label: value } } : e)),
      );
    },
    [setEdges],
  );

  const handleDuplicateNode = useCallback(
    (id: string) => {
      setNodes((nds) => {
        const source = nds.find((n) => n.id === id);
        if (!source) return nds;
        const clone: Node = {
          ...source,
          id: makeNodeId(),
          position: { x: source.position.x + 24, y: source.position.y + 24 },
          selected: false,
          data: { ...source.data },
        };
        return [...nds, clone];
      });
    },
    [setNodes],
  );

  const handleDeleteNode = useCallback(
    (id: string) => {
      setNodes((nds) => nds.filter((n) => n.id !== id));
      setEdges((eds) => eds.filter((e) => e.source !== id && e.target !== id));
    },
    [setNodes, setEdges],
  );

  const onConnect = useCallback(
    (connection: Connection) => setEdges((eds) => addEdge(connection, eds)),
    [setEdges],
  );

  function handleAddNode(type: NodeShape) {
    const id = makeNodeId();
    setNodes((nds) => [
      ...nds,
      {
        id,
        type,
        position: { x: 80 + Math.random() * 240, y: 80 + Math.random() * 200 },
        data: { label: "" },
      },
    ]);
  }

  // Only track viewport moves the user actually made: a null event means
  // the change was programmatic (e.g. the initial fitView settling), and
  // folding that into state would immediately mark a freshly opened,
  // untouched graph as dirty — the same false-positive class of bug the
  // baselineRef comment below describes for nodes/edges.
  const handleMoveEnd = useCallback((event: MouseEvent | TouchEvent | null, next: Viewport) => {
    if (!event) return;
    setViewport(next);
  }, []);

  // The graph's rendered nodes/edges carry render-only callbacks
  // (ReactFlow's standard pattern for interactive custom nodes/edges); the
  // underlying `nodes`/`edges` state — what actually gets persisted —
  // never holds a function, so it stays plain, JSON-serializable data.
  const displayNodes = useMemo(
    () =>
      nodes.map((n) => ({
        ...n,
        data: {
          ...n.data,
          onLabelChange: handleLabelChange,
          onDuplicate: handleDuplicateNode,
          onDelete: handleDeleteNode,
        },
      })),
    [nodes, handleLabelChange, handleDuplicateNode, handleDeleteNode],
  );
  const displayEdges = useMemo(
    () => edges.map((e) => ({ ...e, data: { ...e.data, onLabelChange: handleEdgeLabelChange } })),
    [edges, handleEdgeLabelChange],
  );

  // Skip the mount-time echo — re-serializing the just-parsed initial
  // value would report a change even though the user hasn't touched
  // anything yet, showing a false "unsaved" state the instant a graph doc
  // opens (the same class of bug MarkdownEditor's MDXEditor normalization
  // guard exists for). Comparing against a baseline snapshot rather than
  // an invocation-count flag matters here: React 18 StrictMode
  // double-invokes effects on mount (mount → cleanup → mount again), so a
  // simple "was this the first run" ref flips to false after the first
  // invocation and still fires on the second — this only fires when the
  // content has genuinely changed from what was loaded. viewport rides
  // along in the same snapshot/effect rather than a second dirty-tracking
  // mechanism.
  const baselineRef = useRef(
    JSON.stringify(
      {
        nodes: initial.nodes.map(sanitizeNode),
        edges: initial.edges.map(sanitizeEdge),
        viewport: initial.viewport,
      },
      null,
      2,
    ),
  );
  useEffect(() => {
    const serialized = JSON.stringify(
      { nodes: nodes.map(sanitizeNode), edges: edges.map(sanitizeEdge), viewport },
      null,
      2,
    );
    if (serialized === baselineRef.current) return;
    onChange(serialized);
  }, [nodes, edges, viewport, onChange]);

  return (
    <div
      className="flex h-full min-h-0 w-full flex-col"
      onKeyDown={(event) => {
        if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "s") {
          event.preventDefault();
          onSave();
        }
      }}
    >
      <div className="flex h-9 shrink-0 items-center gap-2 border-b border-border px-3">
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button type="button" size="xs" variant="secondary">
              <Plus />
              Add Node
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start">
            {shapeOptions.map(({ type, label, icon: Icon }) => (
              <DropdownMenuItem key={type} onSelect={() => handleAddNode(type)}>
                <Icon />
                {label}
              </DropdownMenuItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
        <span className="text-xs text-muted-foreground">
          {nodes.length} node{nodes.length === 1 ? "" : "s"}
        </span>
      </div>
      <div className="min-h-0 flex-1">
        <ReactFlow
          nodes={displayNodes}
          edges={displayEdges}
          nodeTypes={nodeTypes}
          edgeTypes={edgeTypes}
          defaultEdgeOptions={defaultEdgeOptions}
          onNodesChange={onNodesChange}
          onEdgesChange={onEdgesChange}
          onConnect={onConnect}
          onMoveEnd={handleMoveEnd}
          snapToGrid
          snapGrid={[16, 16]}
          colorMode="dark"
          defaultViewport={initial.viewport}
          fitView={!initial.viewport}
        >
          <Background />
          <Controls />
          <MiniMap pannable zoomable />
        </ReactFlow>
      </div>
    </div>
  );
}
