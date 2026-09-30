import { useEffect, useRef, useState } from "react";
import { ChevronDown, ChevronRight, FileText, Folder, FolderOpen, MoreHorizontal, Plus, Search, Upload } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import { NoteIcon } from "./note-icons";
import { baseName, parentOf, type TreeNode } from "./notes-tree";
import { DOC_TEMPLATES } from "./templates";

// The left side of the notes: a tree of folders and pages, like a file
// sidebar. Click to open, drag onto a folder to move, "…" for the rest.

interface NotesTreeProps {
  tree: TreeNode[];
  loading: boolean;
  error: string | null;
  selected: string | null;
  /** Folder the next new page or folder goes in ("" is the top). */
  targetFolder: string;
  onTargetFolder: (folder: string) => void;
  onOpen: (path: string) => void;
  onNewPage: (folder: string, template?: string) => void;
  onNewFolder: (folder: string) => void;
  onImport: (files: File[], folder: string) => void;
  onMove: (from: string, toFolder: string) => void;
  onRename: (node: TreeNode, name: string) => void;
  onDuplicate: (path: string) => void;
  onChangeIcon: (node: TreeNode) => void;
  onDelete: (node: TreeNode) => void;
  /** A folder to start renaming right away (just created). */
  renameRequest: string | null;
  onRetry: () => void;
}

const DRAG_TYPE = "application/x-infinabox-note";

export function NotesTree(props: NotesTreeProps) {
  const { tree, loading, error, targetFolder, onTargetFolder, onNewPage, onNewFolder, onImport, onMove } = props;
  const [query, setQuery] = useState("");
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [dropTarget, setDropTarget] = useState<string | null>(null);
  const importInput = useRef<HTMLInputElement | null>(null);

  const toggle = (path: string) =>
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });

  const q = query.trim().toLowerCase();
  const flat: TreeNode[] = [];
  if (q) {
    const walk = (nodes: TreeNode[]) => {
      for (const n of nodes) {
        if (n.kind === "folder") walk(n.children);
        else if (n.title.toLowerCase().includes(q) || n.path.toLowerCase().includes(q)) flat.push(n);
      }
    };
    walk(tree);
  }

  const dropOn = (folder: string) => (e: React.DragEvent) => {
    const from = e.dataTransfer.getData(DRAG_TYPE);
    setDropTarget(null);
    if (!from) return;
    e.preventDefault();
    e.stopPropagation();
    onMove(from, folder);
  };
  const dragOver = (folder: string) => (e: React.DragEvent) => {
    if (!e.dataTransfer.types.includes(DRAG_TYPE)) return;
    e.preventDefault();
    e.stopPropagation();
    setDropTarget(folder);
  };

  const empty = tree.length === 0;

  return (
    <div className="flex h-full min-h-0 flex-col rounded-xl border border-border bg-card" data-testid="notes-tree">
      <div data-tauri-drag-region className="flex h-10 shrink-0 items-center justify-between gap-1 px-3">
        <span data-tauri-drag-region className="text-sm font-medium">
          Notes
        </span>
        <div className="flex items-center gap-0.5">
          <input
            ref={importInput}
            type="file"
            multiple
            hidden
            accept=".md,.markdown,.txt,text/markdown,text/plain"
            data-testid="import-input"
            onChange={(e) => {
              const files = [...(e.target.files ?? [])];
              e.target.value = "";
              if (files.length > 0) onImport(files, targetFolder);
            }}
          />
          <Button size="icon-xs" variant="ghost" aria-label="Import a file" title="Import a Markdown or text file" onClick={() => importInput.current?.click()}>
            <Upload />
          </Button>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button size="icon-xs" variant="ghost" aria-label="New" data-testid="new-menu">
                <Plus />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end" className="w-52" onCloseAutoFocus={(e) => e.preventDefault()}>
              <DropdownMenuItem data-testid="new-page" onSelect={() => onNewPage(targetFolder)}>
                <FileText /> Page
              </DropdownMenuItem>
              <DropdownMenuItem data-testid="new-folder" onSelect={() => onNewFolder(targetFolder)}>
                <Folder /> Folder
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuLabel>From a template</DropdownMenuLabel>
              {DOC_TEMPLATES.filter((t) => t.id !== "note").map((t) => (
                <DropdownMenuItem key={t.id} data-testid={`template-${t.id}`} onSelect={() => onNewPage(targetFolder, t.id)}>
                  {t.label}
                </DropdownMenuItem>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      </div>

      <div className="relative shrink-0 px-2 pb-2">
        <Search className="pointer-events-none absolute top-1/2 left-4 size-3.5 -translate-y-[65%] text-muted-foreground" />
        <Input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search" aria-label="Search notes" className="h-8 pl-7" />
      </div>

      <div
        className={cn("min-h-0 flex-1 overflow-y-auto px-1.5 pb-2", dropTarget === "" && "bg-accent/40")}
        onDragOver={dragOver("")}
        onDragLeave={() => setDropTarget(null)}
        onDrop={dropOn("")}
        onClick={() => onTargetFolder("")}
      >
        {error ? (
          <div role="alert" className="m-2 rounded-lg bg-destructive/10 p-3 text-xs text-destructive">
            <p className="break-words">{error}</p>
            <Button size="xs" variant="outline" className="mt-2" onClick={props.onRetry}>
              Try again
            </Button>
          </div>
        ) : loading ? (
          <p className="p-3 text-xs text-muted-foreground">Loading…</p>
        ) : empty ? (
          <div className="flex flex-col items-center gap-2 p-6 text-center">
            <p className="text-sm text-muted-foreground">No notes yet</p>
            <Button size="sm" onClick={() => onNewPage("")}>
              <Plus /> New page
            </Button>
          </div>
        ) : q ? (
          flat.length === 0 ? (
            <p className="p-3 text-xs text-muted-foreground">Nothing found</p>
          ) : (
            flat.map((n) => <Row key={n.path} node={n} depth={0} {...props} collapsed={collapsed} toggle={toggle} dropTarget={dropTarget} dragOver={dragOver} dropOn={dropOn} showPath />)
          )
        ) : (
          tree.map((n) => <Row key={n.path} node={n} depth={0} {...props} collapsed={collapsed} toggle={toggle} dropTarget={dropTarget} dragOver={dragOver} dropOn={dropOn} />)
        )}
      </div>
    </div>
  );
}

interface RowProps extends NotesTreeProps {
  node: TreeNode;
  depth: number;
  collapsed: Set<string>;
  toggle: (path: string) => void;
  dropTarget: string | null;
  dragOver: (folder: string) => (e: React.DragEvent) => void;
  dropOn: (folder: string) => (e: React.DragEvent) => void;
  showPath?: boolean;
}

function Row(props: RowProps) {
  const { node, depth, collapsed, toggle, selected, targetFolder, onTargetFolder, onOpen, onNewPage, onNewFolder, onDuplicate, onChangeIcon, onDelete, onRename, dropTarget, dragOver, dropOn, renameRequest } = props;
  const isFolder = node.kind === "folder";
  const open = isFolder && !collapsed.has(node.path);
  const active = !isFolder && node.path === selected;
  const label = isFolder ? node.name : node.title;
  const [renaming, setRenaming] = useState(false);
  const [draft, setDraft] = useState(label);

  useEffect(() => {
    if (isFolder && renameRequest === node.path) {
      setDraft(label);
      setRenaming(true);
    }
  }, [renameRequest, isFolder, node.path, label]);

  const commit = () => {
    setRenaming(false);
    if (draft.trim() && draft.trim() !== label) onRename(node, draft.trim());
  };

  return (
    <div>
      <div
        role="treeitem"
        aria-selected={active}
        aria-expanded={isFolder ? open : undefined}
        data-testid={isFolder ? "tree-folder" : "tree-page"}
        data-path={node.path}
        draggable={!renaming}
        onDragStart={(e) => {
          e.dataTransfer.setData(DRAG_TYPE, node.path);
          e.dataTransfer.effectAllowed = "move";
        }}
        onDragOver={isFolder ? dragOver(node.path) : dragOver(parentOf(node.path))}
        onDrop={isFolder ? dropOn(node.path) : dropOn(parentOf(node.path))}
        onClick={(e) => {
          e.stopPropagation();
          if (isFolder) {
            toggle(node.path);
            onTargetFolder(node.path);
          } else {
            onTargetFolder(parentOf(node.path));
            onOpen(node.path);
          }
        }}
        style={{ paddingLeft: 6 + depth * 14 }}
        className={cn(
          "group flex h-7 cursor-pointer items-center gap-1 rounded-md pr-1 text-sm",
          active ? "bg-accent text-foreground" : "text-foreground/85 hover:bg-accent/50",
          dropTarget === (isFolder ? node.path : null) && isFolder && "bg-accent ring-1 ring-ring",
          isFolder && targetFolder === node.path && !active && "bg-accent/30",
        )}
      >
        {isFolder ? (
          <>
            {open ? <ChevronDown className="size-3.5 shrink-0 text-muted-foreground" /> : <ChevronRight className="size-3.5 shrink-0 text-muted-foreground" />}
            {node.icon ? (
              <span className="flex w-4 shrink-0 justify-center text-sm" data-testid="row-icon"><NoteIcon value={node.icon} className="size-4 text-sm text-muted-foreground" /></span>
            ) : open ? (
              <FolderOpen className="size-4 shrink-0 text-muted-foreground" />
            ) : (
              <Folder className="size-4 shrink-0 text-muted-foreground" />
            )}
          </>
        ) : (
          node.icon ? (
            <span className="ml-[18px] flex w-4 shrink-0 justify-center text-sm" data-testid="row-icon"><NoteIcon value={node.icon} className="size-4 text-sm text-muted-foreground" /></span>
          ) : (
            <FileText className="ml-[18px] size-4 shrink-0 text-muted-foreground" />
          )
        )}
        {renaming ? (
          <input
            autoFocus
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onBlur={commit}
            onKeyDown={(e) => {
              if (e.key === "Enter") commit();
              if (e.key === "Escape") setRenaming(false);
            }}
            onClick={(e) => e.stopPropagation()}
            data-testid="rename-input"
            className="min-w-0 flex-1 rounded bg-background px-1 text-sm outline-none ring-1 ring-ring"
          />
        ) : (
          <span className="min-w-0 flex-1 truncate">
            {label || baseName(node.path)}
            {props.showPath && parentOf(node.path) && <span className="ml-1.5 text-xs text-muted-foreground">{parentOf(node.path)}</span>}
          </span>
        )}
        {!renaming && (
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button size="icon-xs" variant="ghost" aria-label="More" data-testid="row-menu" className="size-5 opacity-0 group-hover:opacity-100 data-[state=open]:opacity-100" onClick={(e) => e.stopPropagation()}>
                <MoreHorizontal />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start" className="w-44" onClick={(e) => e.stopPropagation()} onCloseAutoFocus={(e) => e.preventDefault()}>
              {isFolder && (
                <>
                  <DropdownMenuItem onSelect={() => onNewPage(node.path)}>New page inside</DropdownMenuItem>
                  <DropdownMenuItem onSelect={() => onNewFolder(node.path)}>New folder inside</DropdownMenuItem>
                  <DropdownMenuSeparator />
                </>
              )}
              <DropdownMenuItem
                data-testid="rename"
                onSelect={() => {
                  setDraft(label);
                  setRenaming(true);
                }}
              >
                Rename
              </DropdownMenuItem>
              <DropdownMenuItem data-testid="change-icon" onSelect={() => onChangeIcon(node)}>
                {node.icon ? "Change icon" : "Add icon"}
              </DropdownMenuItem>
              {!isFolder && <DropdownMenuItem onSelect={() => onDuplicate(node.path)}>Duplicate</DropdownMenuItem>}
              <DropdownMenuItem variant="destructive" data-testid="delete" onSelect={() => onDelete(node)}>
                Delete
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        )}
      </div>
      {isFolder && open && node.children.map((child) => <Row key={child.path} {...props} node={child} depth={depth + 1} />)}
      {isFolder && open && node.children.length === 0 && (
        <div style={{ paddingLeft: 30 + depth * 14 }} className="h-6 text-xs text-muted-foreground/70">
          Empty
        </div>
      )}
    </div>
  );
}
