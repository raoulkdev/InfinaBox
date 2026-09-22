import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import type { FileEntry } from "@/types/fs";

interface DeleteEntryDialogProps {
  entry: FileEntry | null;
  onCancel: () => void;
  onConfirm: (entry: FileEntry) => void;
}

/** Shared by FileGrid and FileList — driven by `entry` rather than each
 * caller nesting an AlertDialogTrigger inside its own context-menu item.
 * Two different floating-UI systems (context menu + dialog) fighting over
 * focus/dismiss timing on the same click is a known-flaky pattern; a
 * single dialog controlled by external state sidesteps it entirely. */
export function DeleteEntryDialog({ entry, onCancel, onConfirm }: DeleteEntryDialogProps) {
  return (
    <AlertDialog
      open={entry !== null}
      onOpenChange={(open) => {
        if (!open) onCancel();
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Delete "{entry?.name}"?</AlertDialogTitle>
          <AlertDialogDescription>
            {entry?.is_dir
              ? "This permanently deletes this folder and everything inside it. This can't be undone."
              : "This permanently deletes this file. This can't be undone."}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction
            variant="destructive"
            onClick={() => {
              if (entry) onConfirm(entry);
            }}
          >
            Delete
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
