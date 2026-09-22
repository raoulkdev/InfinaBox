import { useState } from "react";
import { File, Folder, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import { DeleteEntryDialog } from "@/components/cockpit/DeleteEntryDialog";
import { cn } from "@/lib/utils";
import type { FileEntry } from "@/types/fs";

interface FileGridProps {
  entries: FileEntry[];
  selectedPath: string | null;
  onOpenFolder: (path: string) => void;
  onSelectFile: (path: string) => void;
  onDeleteEntry: (entry: FileEntry) => void;
}

/** A macOS Finder icon-view stand-in: folders and files as icon+label
 * tiles. Clicking a folder navigates into it (there's no separate
 * "open" gesture in this embedded browser); clicking a file selects it
 * for the inspector below. Right-click gives Delete, confirmed via
 * DeleteEntryDialog since it's the one irreversible action here. */
export function FileGrid({
  entries,
  selectedPath,
  onOpenFolder,
  onSelectFile,
  onDeleteEntry,
}: FileGridProps) {
  const [pendingDelete, setPendingDelete] = useState<FileEntry | null>(null);

  return (
    <>
      {entries.length === 0 ? (
        <p className="px-3 py-2 text-sm text-muted-foreground">This folder is empty.</p>
      ) : (
        <div className="grid grid-cols-[repeat(auto-fill,minmax(76px,1fr))] gap-1 p-3">
          {entries.map((entry) => (
            <ContextMenu key={entry.path}>
              <ContextMenuTrigger asChild>
                <Button
                  type="button"
                  variant="ghost"
                  onClick={() => (entry.is_dir ? onOpenFolder(entry.path) : onSelectFile(entry.path))}
                  className={cn(
                    "h-auto flex-col gap-1 rounded-md p-2 text-center font-normal",
                    !entry.is_dir && selectedPath === entry.path && "bg-accent",
                  )}
                >
                  {entry.is_dir ? (
                    <Folder className="size-9 shrink-0 text-sky-400/80" />
                  ) : (
                    <File className="size-9 shrink-0 text-muted-foreground" />
                  )}
                  <span className="w-full truncate text-xs font-normal text-foreground/90">
                    {entry.name}
                  </span>
                </Button>
              </ContextMenuTrigger>
              <ContextMenuContent>
                <ContextMenuItem variant="destructive" onSelect={() => setPendingDelete(entry)}>
                  <Trash2 />
                  Delete
                </ContextMenuItem>
              </ContextMenuContent>
            </ContextMenu>
          ))}
        </div>
      )}

      <DeleteEntryDialog
        entry={pendingDelete}
        onCancel={() => setPendingDelete(null)}
        onConfirm={(entry) => {
          onDeleteEntry(entry);
          setPendingDelete(null);
        }}
      />
    </>
  );
}
