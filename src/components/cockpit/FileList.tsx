import { useState } from "react";
import { ChevronRight, File, Folder, Trash2 } from "lucide-react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { useContextMenu } from "@/lib/context-menu";
import { Button } from "@/components/ui/button";
import { DeleteEntryDialog } from "@/components/cockpit/DeleteEntryDialog";
import { cn } from "@/lib/utils";
import type { FileEntry } from "@/types/fs";

interface FileListProps {
  entries: FileEntry[];
  selectedPath: string | null;
  onOpenFolder: (path: string) => void;
  onSelectFile: (path: string) => void;
  onDeleteEntry: (entry: FileEntry) => void;
}

/** A compact row-per-entry list — the Design tab's docs browser, next to
 * the editor rather than above it, so a narrow column reads better than
 * the Files panel's icon grid. Same navigation model as FileGrid (click a
 * folder to drill into it, click a file to select it, right-click for
 * Delete): only the row rendering differs. */
export function FileList({
  entries,
  selectedPath,
  onOpenFolder,
  onSelectFile,
  onDeleteEntry,
}: FileListProps) {
  const [pendingDelete, setPendingDelete] = useState<FileEntry | null>(null);
  const menu = useContextMenu();
  const entryMenu = (e: React.MouseEvent, entry: FileEntry) =>
    menu(e, [
      { label: entry.is_dir ? "Open folder" : "Open", onSelect: () => (entry.is_dir ? onOpenFolder(entry.path) : onSelectFile(entry.path)) },
      { label: "Show in folder", onSelect: () => void revealItemInDir(entry.path).catch(() => {}) },
      { label: "Copy path", onSelect: () => void navigator.clipboard.writeText(entry.path).catch(() => {}) },
      "separator",
      { label: "Delete", destructive: true, icon: <Trash2 />, testId: "ctx-delete", onSelect: () => setPendingDelete(entry) },
    ]);

  return (
    <>
      {entries.length === 0 ? (
        <p className="px-3 py-2 text-sm text-muted-foreground">This folder is empty.</p>
      ) : (
        <div className="flex flex-col py-1">
          {entries.map((entry) => (
            <Button
              key={entry.path}
              onContextMenu={(e) => entryMenu(e, entry)}
                  type="button"
                  variant="ghost"
                  onClick={() => (entry.is_dir ? onOpenFolder(entry.path) : onSelectFile(entry.path))}
                  className={cn(
                    "h-auto w-full justify-start gap-1.5 rounded-none px-3 py-1.5 text-left font-normal",
                    !entry.is_dir && selectedPath === entry.path && "bg-accent text-foreground",
                  )}
                >
                  {entry.is_dir ? (
                    <Folder className="size-3.5 shrink-0 text-sky-400/80" />
                  ) : (
                    <File className="size-3.5 shrink-0 text-muted-foreground" />
                  )}
                  <span className="min-w-0 flex-1 truncate text-sm text-foreground/90">
                    {entry.name}
                  </span>
                  {entry.is_dir && (
                    <ChevronRight className="size-3.5 shrink-0 text-muted-foreground" />
                  )}
                </Button>
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
