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
import type { Snapshot } from "@/lib/studio-types";

interface GoBackDialogProps {
  snapshot: Snapshot | null;
  onCancel: () => void;
  onConfirm: (snapshot: Snapshot) => void;
}

/** Driven by `snapshot` rather than an `AlertDialogTrigger` nested in each
 * row — the same single-controlled-dialog pattern as `DeleteEntryDialog`.
 * Unlike delete, going back is fully reversible (it's a new snapshot, and
 * any unsaved work is saved first), and the copy says so plainly: the point
 * of confirming is to avoid surprise, not to warn of loss. */
export function GoBackDialog({ snapshot, onCancel, onConfirm }: GoBackDialogProps) {
  return (
    <AlertDialog
      open={snapshot !== null}
      onOpenChange={(open) => {
        if (!open) onCancel();
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Go back to "{snapshot?.title}"?</AlertDialogTitle>
          <AlertDialogDescription>
            Your game will be put back exactly the way it was at this point. Nothing is lost:
            everything after it stays in your history, and you can undo this afterwards.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction
            onClick={() => {
              if (snapshot) onConfirm(snapshot);
            }}
          >
            Go back
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
