import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import { getLayout, setLayout } from "@/lib/layout-store";
import {
  ArrowUpRight, CaseSensitive, ChevronsLeft, ChevronsRight, CheckSquare, Columns3, FileText, Hand, Image as ImageIcon, Layers, Link2, MessageCircle, MousePointer2, Paperclip, Palette, Pencil, Redo2,
  StickyNote, Table2, Undo2,
} from "lucide-react";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { DOC_TEMPLATES } from "@/components/context/templates";
import { cn } from "@/lib/utils";
import type { BlockType } from "./types";

export type Tool = "select" | "hand" | "arrow";

// Collapsed it is a column of icons; expanded, each has its name beside it.
const Expanded = createContext(false);

function Btn({ label, shortcut, active, disabled, onClick, children, testId }: { label: string; shortcut?: string; active?: boolean; disabled?: boolean; onClick?: () => void; children: ReactNode; testId?: string }) {
  const expanded = useContext(Expanded);
  return (
    <button
      type="button"
      title={shortcut ? `${label} (${shortcut})` : label}
      aria-label={label}
      aria-pressed={active}
      disabled={disabled}
      data-testid={testId}
      onClick={onClick}
      className={cn(
        "flex h-8 items-center rounded-md text-muted-foreground hover:bg-foreground/10 hover:text-foreground disabled:pointer-events-none disabled:opacity-40",
        expanded ? "w-full gap-2 px-2" : "w-8 justify-center",
        active && "bg-foreground/10 text-foreground",
      )}
    >
      {children}
      {expanded && <span className="flex-1 truncate text-left text-sm">{label}</span>}
      {expanded && shortcut && <span className="text-xs text-muted-foreground/70">{shortcut}</span>}
    </button>
  );
}

const Sep = () => <div className="mx-1 my-0.5 h-px bg-border" />;

export function Toolbar({
  tool,
  setTool,
  onAdd,
  onNewDoc,
  onUndo,
  onRedo,
  canUndo,
  canRedo,
}: {
  tool: Tool;
  setTool: (t: Tool) => void;
  onAdd: (type: BlockType) => void;
  onNewDoc: (template: string | null) => void;
  onUndo: () => void;
  onRedo: () => void;
  canUndo: boolean;
  canRedo: boolean;
}) {
  const [expanded, setExpanded] = useState(() => getLayout<boolean>("documents.toolbar") === true);
  useEffect(() => setLayout("documents.toolbar", expanded), [expanded]);
  const add = (type: BlockType, label: string, icon: ReactNode) => (
    <Btn label={label} onClick={() => onAdd(type)} testId={`add-${type}`}>
      {icon}
    </Btn>
  );
  return (
    <Expanded.Provider value={expanded}>
    <div
      data-testid="canvas-toolbar"
      data-canvas-ui
      className={cn("absolute left-3 top-1/2 z-30 flex max-h-[calc(100%-24px)] -translate-y-1/2 flex-col gap-0.5 overflow-y-auto rounded-xl border border-border bg-card/95 p-1 shadow-md backdrop-blur", expanded && "w-44")}
    >
      <Btn label={expanded ? "Collapse" : "Expand"} onClick={() => setExpanded((v) => !v)} testId="toolbar-toggle">
        {expanded ? <ChevronsLeft className="size-4" /> : <ChevronsRight className="size-4" />}
      </Btn>
      <Sep />
      <Btn label="Select" shortcut="V" active={tool === "select"} onClick={() => setTool("select")} testId="tool-select">
        <MousePointer2 className="size-4" />
      </Btn>
      <Btn label="Move canvas" shortcut="H" active={tool === "hand"} onClick={() => setTool("hand")} testId="tool-hand">
        <Hand className="size-4" />
      </Btn>
      <Sep />
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button
            type="button"
            title="Document"
            aria-label="Document"
            data-testid="add-doc"
            className={cn("flex h-8 items-center rounded-md text-muted-foreground hover:bg-foreground/10 hover:text-foreground", expanded ? "w-full gap-2 px-2" : "w-8 justify-center")}
          >
            <FileText className="size-4" />
            {expanded && <span className="flex-1 truncate text-left text-sm">Document</span>}
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent side="right" align="start" className="max-h-96 w-56 overflow-y-auto">
          <DropdownMenuItem data-testid="new-doc-blank" onSelect={() => onNewDoc(null)}>
            Blank document
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuLabel className="text-xs text-muted-foreground">From a template</DropdownMenuLabel>
          {DOC_TEMPLATES.map((t) => (
            <DropdownMenuItem key={t.id} onSelect={() => onNewDoc(t.id)}>
              {t.label}
            </DropdownMenuItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>
      {add("note", "Note", <StickyNote className="size-4" />)}
      {add("todo", "To-do list", <CheckSquare className="size-4" />)}
      {add("column", "Column", <Columns3 className="size-4" />)}
      {add("board", "Board", <Layers className="size-4" />)}
      {add("image", "Image", <ImageIcon className="size-4" />)}
      {add("file", "File", <Paperclip className="size-4" />)}
      {add("link", "Link", <Link2 className="size-4" />)}
      <Btn label="Arrow" shortcut="A" active={tool === "arrow"} onClick={() => setTool(tool === "arrow" ? "select" : "arrow")} testId="tool-arrow">
        <ArrowUpRight className="size-4" />
      </Btn>
      {add("sketch", "Sketch", <Pencil className="size-4" />)}
      {add("swatch", "Color", <Palette className="size-4" />)}
      {add("table", "Table", <Table2 className="size-4" />)}
      {add("text", "Text", <CaseSensitive className="size-4" />)}
      {add("comment", "Comment", <MessageCircle className="size-4" />)}
      <Sep />
      <Btn label="Undo" shortcut="⌘Z" disabled={!canUndo} onClick={onUndo} testId="canvas-undo">
        <Undo2 className="size-4" />
      </Btn>
      <Btn label="Redo" shortcut="⇧⌘Z" disabled={!canRedo} onClick={onRedo} testId="canvas-redo">
        <Redo2 className="size-4" />
      </Btn>
    </div>
    </Expanded.Provider>
  );
}
