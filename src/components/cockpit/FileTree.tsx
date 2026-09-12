import { useState } from "react";
import { ChevronRight, Folder, FolderOpen, File } from "lucide-react";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { cn } from "@/lib/utils";
import type { FileEntry } from "@/types/fs";

interface FileTreeNodeProps {
  entry: FileEntry;
  depth: number;
  defaultOpen: boolean;
  selectedPath?: string | null;
  onSelectFile?: (entry: FileEntry) => void;
}

function FileTreeNode({
  entry,
  depth,
  defaultOpen,
  selectedPath,
  onSelectFile,
}: FileTreeNodeProps) {
  const [open, setOpen] = useState(defaultOpen);

  if (!entry.is_dir) {
    const selected = selectedPath === entry.path;
    return (
      <button
        type="button"
        onClick={() => onSelectFile?.(entry)}
        className={cn(
          "flex w-full items-center gap-1.5 py-0.5 text-left text-sm text-foreground/90 hover:bg-accent",
          selected && "bg-accent text-foreground",
        )}
        style={{ paddingLeft: depth * 14 + 20 }}
      >
        <File className="size-3.5 shrink-0 text-muted-foreground" />
        <span className="truncate">{entry.name}</span>
      </button>
    );
  }

  const children = entry.children ?? [];

  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <CollapsibleTrigger asChild>
        <button
          type="button"
          className="flex w-full items-center gap-1.5 py-0.5 text-left text-sm hover:bg-accent"
          style={{ paddingLeft: depth * 14 + 4 }}
        >
          <ChevronRight
            className={cn(
              "size-3.5 shrink-0 text-muted-foreground transition-transform",
              open && "rotate-90",
            )}
          />
          {open ? (
            <FolderOpen className="size-3.5 shrink-0 text-muted-foreground" />
          ) : (
            <Folder className="size-3.5 shrink-0 text-muted-foreground" />
          )}
          <span className="truncate">{entry.name}</span>
        </button>
      </CollapsibleTrigger>
      <CollapsibleContent>
        {children.map((child) => (
          <FileTreeNode
            key={child.path}
            entry={child}
            depth={depth + 1}
            defaultOpen={false}
            selectedPath={selectedPath}
            onSelectFile={onSelectFile}
          />
        ))}
      </CollapsibleContent>
    </Collapsible>
  );
}

interface FileTreeProps {
  entries: FileEntry[];
  selectedPath?: string | null;
  onSelectFile?: (entry: FileEntry) => void;
}

export function FileTree({ entries, selectedPath, onSelectFile }: FileTreeProps) {
  return (
    <div>
      {entries.map((entry) => (
        <FileTreeNode
          key={entry.path}
          entry={entry}
          depth={0}
          defaultOpen
          selectedPath={selectedPath}
          onSelectFile={onSelectFile}
        />
      ))}
    </div>
  );
}
