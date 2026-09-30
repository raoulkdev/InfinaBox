import { useCallback, useEffect, useRef, useState } from "react";
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
import { DocumentAside } from "./DocumentAside";
import { NewCardDialog } from "./NewCardDialog";
import type { AskAi } from "./aiActions";
import { errorText, newCardPath } from "./cardTypes";
import { contextWrite } from "@/lib/studio-api";

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
  /** Sends a request to the AI in Studio's chat. */
  onAskAi: AskAi;
  /** Set by the Overview: open the New document dialog on this template. */
  newRequest: NewRequest | null;
}

export interface NewRequest {
  template: string;
  nonce: number;
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
  onAskAi,
  newRequest,
}: CardsTabProps) {
  const [selected, setSelectedState] = useState<string | null>(null);
  // The card list as it was when the current card was picked: a card just
  // created isn't in it yet, and mustn't be dropped as "deleted" before the
  // list has been re-read.
  const cardsAtSelect = useRef<CardSummary[] | null>(null);
  const cardsNow = useRef(cards);
  cardsNow.current = cards;
  const setSelected = useCallback((path: string | null) => {
    cardsAtSelect.current = cardsNow.current;
    setSelectedState(path);
  }, []);
  const [dirty, setDirty] = useState(false);
  const [pending, setPending] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [template, setTemplate] = useState("note");
  const [importError, setImportError] = useState<string | null>(null);

  // The Overview asks for a new document from a template.
  const [newHandled, setNewHandled] = useState<number | null>(null);
  useEffect(() => {
    if (newRequest && newRequest.nonce !== newHandled) {
      setNewHandled(newRequest.nonce);
      setTemplate(newRequest.template);
      setCreating(true);
    }
  }, [newRequest, newHandled]);

  // Text files brought in become documents (type "other", named after the file).
  async function importFiles(files: File[]) {
    setImportError(null);
    const taken = (cards ?? []).map((c) => c.path);
    let last: string | null = null;
    for (const file of files) {
      try {
        const body = await file.text();
        const title = file.name.replace(/\.[^.]+$/, "") || "Imported note";
        const path = newCardPath("other", title, taken);
        taken.push(path);
        await contextWrite(
          projectPath,
          path,
          { type: "other", title, status: "draft", links: [], implemented_in: [], tags: ["imported"], extra: {} },
          body,
        );
        last = path;
      } catch (err) {
        setImportError(`${file.name}: ${errorText(err)}`);
      }
    }
    if (last) {
      onChanged();
      requestSelect(last);
    }
  }

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
    if (selected && cards && cards !== cardsAtSelect.current && !cards.some((c) => c.path === selected)) {
      // Keep it open while it has unsaved edits: the editor says what's wrong.
      if (!dirty) setSelected(null);
    }
  }, [cards, selected, dirty]);

  return (
    <>
      <ResizablePanelGroup
        storageKey="context.documents"
        panels={[
          {
            id: "list",
            minPercent: 18,
            defaultPercent: 24,
            content: (
              <CardList
                cards={cards}
                loading={loading}
                error={error}
                selectedPath={selected}
                onSelect={requestSelect}
                onNew={() => {
                  setTemplate("note");
                  setCreating(true);
                }}
                onImport={(files) => void importFiles(files)}
                onRetry={onReload}
              />
            ),
          },
          {
            id: "editor",
            minPercent: 34,
            defaultPercent: 52,
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
                <p className="text-sm font-medium">No document open</p>
                <p className="max-w-xs text-xs text-muted-foreground">
                  Pick a document on the left to read or edit it, or make a new one.
                </p>
              </div>
            ),
          },
          {
            id: "aside",
            minPercent: 16,
            defaultPercent: 24,
            content: selected ? (
              <DocumentAside
                projectPath={projectPath}
                path={selected}
                cards={cards}
                refreshTick={refreshTick}
                onAsk={onAskAi}
                onOpen={requestSelect}
              />
            ) : (
              <div className="flex h-full flex-col items-center justify-center gap-1 rounded-xl border border-border bg-card p-4 text-center">
                <p className="text-xs text-muted-foreground">
                  Open a document to see its outline and ask the AI to work on it.
                </p>
              </div>
            ),
          },
        ]}
      />
      {importError && (
        <p role="alert" className="absolute right-4 bottom-4 max-w-sm rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive">
          {importError}
        </p>
      )}

      <NewCardDialog
        open={creating}
        onOpenChange={setCreating}
        projectPath={projectPath}
        existingPaths={(cards ?? []).map((c) => c.path)}
        initialTemplate={template}
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
