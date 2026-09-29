import { useCallback, useEffect, useState } from "react";
import { FileText } from "lucide-react";
import { ResizablePanelGroup } from "@/components/cockpit/ResizablePanelGroup";
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
import type { CardSummary } from "@/lib/studio-types";
import { CardEditor } from "./CardEditor";
import { CardList } from "./CardList";
import { NewCardDialog } from "./NewCardDialog";

export interface OpenRequest {
  path: string;
  /** Changes on every request, so asking for the same card twice works. */
  nonce: number;
}

interface CardsTabProps {
  projectPath: string;
  cards: CardSummary[] | null;
  loading: boolean;
  error: string | null;
  refreshTick: number;
  openRequest: OpenRequest | null;
  onReload: () => void;
  /** A card was saved or created; other views should refresh. */
  onChanged: () => void;
}

/** The Cards tab: list + filters on the left, the card editor on the right.
 * Switching cards while the open one has unsaved edits asks first. */
export function CardsTab({
  projectPath,
  cards,
  loading,
  error,
  refreshTick,
  openRequest,
  onReload,
  onChanged,
}: CardsTabProps) {
  const [selected, setSelected] = useState<string | null>(null);
  const [dirty, setDirty] = useState(false);
  const [pending, setPending] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);

  const requestSelect = useCallback(
    (path: string) => {
      if (path === selected) return;
      if (dirty) setPending(path);
      else setSelected(path);
    },
    [dirty, selected],
  );

  // Board and Map ask for a card to be opened here.
  const [handled, setHandled] = useState<number | null>(null);
  useEffect(() => {
    if (openRequest && openRequest.nonce !== handled) {
      setHandled(openRequest.nonce);
      requestSelect(openRequest.path);
    }
  }, [openRequest, handled, requestSelect]);

  // A card that no longer exists (deleted on disk) is deselected once the
  // list has loaded without it.
  useEffect(() => {
    if (selected && cards && !cards.some((c) => c.path === selected)) {
      // Keep it open while it has unsaved edits: the editor says what's wrong.
      if (!dirty) setSelected(null);
    }
  }, [cards, selected, dirty]);

  return (
    <>
      <ResizablePanelGroup
        storageKey="context.cards"
        panels={[
          {
            id: "list",
            minPercent: 24,
            defaultPercent: 34,
            content: (
              <CardList
                cards={cards}
                loading={loading}
                error={error}
                selectedPath={selected}
                onSelect={requestSelect}
                onNew={() => setCreating(true)}
                onRetry={onReload}
              />
            ),
          },
          {
            id: "editor",
            minPercent: 40,
            defaultPercent: 66,
            content: selected ? (
              <CardEditor
                key={selected}
                projectPath={projectPath}
                path={selected}
                cards={cards}
                refreshTick={refreshTick}
                onDirtyChange={setDirty}
                onSaved={onChanged}
                onOpen={requestSelect}
              />
            ) : (
              <div className="flex h-full flex-col items-center justify-center gap-2 rounded-xl border border-border bg-card text-center">
                <div className="flex size-10 items-center justify-center rounded-lg border border-border">
                  <FileText className="size-5 text-muted-foreground" />
                </div>
                <p className="text-sm font-medium">No card open</p>
                <p className="max-w-xs text-xs text-muted-foreground">
                  Pick a card on the left to read or edit it, or make a new one.
                </p>
              </div>
            ),
          },
        ]}
      />

      <NewCardDialog
        open={creating}
        onOpenChange={setCreating}
        projectPath={projectPath}
        existingPaths={(cards ?? []).map((c) => c.path)}
        onCreated={(path) => {
          onChanged();
          requestSelect(path);
        }}
      />

      <AlertDialog open={pending !== null} onOpenChange={(open) => !open && setPending(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Leave without saving?</AlertDialogTitle>
            <AlertDialogDescription>
              This card has changes you haven't saved. If you open another card now, those changes are lost.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Keep editing</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              onClick={() => {
                if (pending) {
                  setDirty(false);
                  setSelected(pending);
                }
                setPending(null);
              }}
            >
              Discard and open
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}
