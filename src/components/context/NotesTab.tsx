import { useCallback, useEffect, useMemo, useRef, useState } from "react";
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
import { contextCreateFolder, contextDelete, contextFolders, contextMove, contextRead, contextWrite } from "@/lib/studio-api";
import type { CardSummary } from "@/lib/studio-types";
import { NotesTree } from "./NotesTree";
import { PageView } from "./PageView";
import type { AskAi } from "./aiActions";
import { errorText } from "./cardTypes";
import { baseName, buildTree, join, parentOf, safeName, uniquePath, type TreeNode } from "./notes-tree";
import { templateById, templatedCard } from "./templates";

export interface OpenRequest {
  path: string;
  /** Changes on every request, so asking for the same page twice works. */
  nonce: number;
}

interface NotesTabProps {
  projectPath: string;
  cards: CardSummary[] | null;
  cardsError: string | null;
  refreshTick: number;
  openRequest: OpenRequest | null;
  onReload: () => void;
  /** A page was saved, created, moved or deleted; other views should refresh. */
  onChanged: () => void;
  onAskAi: AskAi;
}

/** The notes: your own folders and pages, in a tree on the left and the open page on the right. */
export function NotesTab({ projectPath, cards, cardsError, refreshTick, openRequest, onReload, onChanged, onAskAi }: NotesTabProps) {
  const [folders, setFolders] = useState<string[]>([]);
  const [selected, setSelectedState] = useState<string | null>(null);
  // The list as it was when the page was picked: a page just created isn't in
  // it yet, and mustn't be closed as "gone" before the list has been re-read.
  const cardsAtSelect = useRef<CardSummary[] | null>(null);
  const cardsNow = useRef(cards);
  cardsNow.current = cards;
  const setSelected = useCallback((path: string | null) => {
    cardsAtSelect.current = cardsNow.current;
    setSelectedState(path);
  }, []);
  const [targetFolder, setTargetFolder] = useState("");
  const [renameRequest, setRenameRequest] = useState<string | null>(null);
  const [focusTitle, setFocusTitle] = useState(0);
  const [deleting, setDeleting] = useState<TreeNode | null>(null);
  const [problem, setProblem] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    contextFolders(projectPath).then(
      (list) => !cancelled && setFolders(list),
      () => {},
    );
    return () => {
      cancelled = true;
    };
  }, [projectPath, refreshTick]);

  const tree = useMemo(() => buildTree(cards ?? [], folders), [cards, folders]);
  const taken = useMemo(() => new Set([...(cards ?? []).map((c) => c.path), ...folders]), [cards, folders]);

  // The Board and Map ask for a page to be opened here.
  const [handled, setHandled] = useState<number | null>(null);
  useEffect(() => {
    if (openRequest && openRequest.nonce !== handled) {
      setHandled(openRequest.nonce);
      setSelected(openRequest.path);
    }
  }, [openRequest, handled]);

  // A page that isn't in the list any more is closed, but only after checking
  // the file really is gone: the list may be a moment behind a create or a move.
  useEffect(() => {
    if (!selected || !cards || cards === cardsAtSelect.current || cards.some((c) => c.path === selected)) return;
    let cancelled = false;
    contextRead(projectPath, selected).catch(() => {
      if (!cancelled) setSelected(null);
    });
    return () => {
      cancelled = true;
    };
  }, [cards, selected, setSelected, projectPath]);

  const run = useCallback(
    async (work: () => Promise<void>) => {
      setProblem(null);
      try {
        await work();
      } catch (err) {
        setProblem(errorText(err));
      }
    },
    [],
  );

  const newPage = (folder: string, template = "note") =>
    run(async () => {
      const t = templateById(template);
      const title = template === "note" ? "Untitled" : t.label;
      const made = templatedCard(t, title, taken);
      const path = uniquePath(folder, safeName(title), ".md", taken);
      await contextWrite(projectPath, path, { ...made.meta, status: template === "note" ? null : made.meta.status }, made.body);
      onChanged();
      setSelected(path);
      setFocusTitle((n) => n + 1);
    });

  const newFolder = (parent: string) =>
    run(async () => {
      const path = uniquePath(parent, "new-folder", "", taken);
      await contextCreateFolder(projectPath, path);
      setFolders((f) => [...f, path]);
      setRenameRequest(path);
      onChanged();
    });

  const importFiles = (files: File[], folder: string) =>
    run(async () => {
      const used = new Set(taken);
      let last: string | null = null;
      for (const file of files) {
        const body = await file.text();
        const title = file.name.replace(/\.[^.]+$/, "") || "Imported note";
        const path = uniquePath(folder, safeName(title), ".md", used);
        used.add(path);
        await contextWrite(projectPath, path, { type: null, title, status: null, links: [], implemented_in: [], tags: [], extra: {} }, body);
        last = path;
      }
      onChanged();
      if (last) setSelected(last);
    });

  const move = (from: string, toFolder: string) =>
    run(async () => {
      if (parentOf(from) === toFolder || from === toFolder) return;
      const to = join(toFolder, baseName(from));
      if (taken.has(to)) throw new Error(`There's already something called ${baseName(from)} in there.`);
      const now = await contextMove(projectPath, from, to);
      if (selected === from) setSelected(now);
      else if (selected?.startsWith(`${from}/`)) setSelected(now + selected.slice(from.length));
      onChanged();
    });

  const rename = (node: TreeNode, name: string) =>
    run(async () => {
      if (node.kind === "folder") {
        const to = join(parentOf(node.path), safeName(name));
        if (to !== node.path && taken.has(to)) throw new Error(`There's already a folder called ${name} here.`);
        const now = await contextMove(projectPath, node.path, to);
        if (selected?.startsWith(`${node.path}/`)) setSelected(now + selected.slice(node.path.length));
      } else {
        await retitle(node.path, name);
      }
      onChanged();
    });

  // A page's title is its name: set it, and give the file a matching name.
  const retitle = async (path: string, title: string) => {
    const card = await contextRead(projectPath, path);
    if (card.header_error) return;
    if ((card.meta.title ?? "") !== title) await contextWrite(projectPath, path, { ...card.meta, title: title || null }, card.body);
    if (!title) return;
    const wanted = uniquePath(parentOf(path), safeName(title), ".md", new Set([...taken].filter((p) => p !== path)));
    if (wanted !== path) {
      const now = await contextMove(projectPath, path, wanted);
      if (selected === path) setSelected(now);
    }
  };

  const duplicate = (path: string) =>
    run(async () => {
      const card = await contextRead(projectPath, path);
      const title = `${card.meta.title || baseName(path).replace(/\.md$/, "")} copy`;
      const copy = uniquePath(parentOf(path), safeName(title), ".md", taken);
      await contextWrite(projectPath, copy, { ...card.meta, title, links: [...card.meta.links] }, card.body);
      onChanged();
      setSelected(copy);
    });

  const confirmDelete = () =>
    run(async () => {
      if (!deleting) return;
      await contextDelete(projectPath, deleting.path);
      if (selected && (selected === deleting.path || selected.startsWith(`${deleting.path}/`))) setSelected(null);
      setDeleting(null);
      onChanged();
    });

  const inside = deleting?.kind === "folder" ? (cards ?? []).filter((c) => c.path.startsWith(`${deleting.path}/`)).length : 0;

  return (
    <>
      <ResizablePanelGroup
        storageKey="context.notes"
        panels={[
          {
            id: "tree",
            minPercent: 16,
            defaultPercent: 24,
            content: (
              <NotesTree
                tree={tree}
                loading={cards === null && cardsError === null}
                error={cardsError}
                selected={selected}
                targetFolder={targetFolder}
                onTargetFolder={setTargetFolder}
                onOpen={setSelected}
                onNewPage={(folder, template) => void newPage(folder, template)}
                onNewFolder={(folder) => void newFolder(folder)}
                onImport={(files, folder) => void importFiles(files, folder)}
                onMove={(from, to) => void move(from, to)}
                onRename={(node, name) => void rename(node, name)}
                onDuplicate={(path) => void duplicate(path)}
                onDelete={setDeleting}
                renameRequest={renameRequest}
                onRetry={onReload}
              />
            ),
          },
          {
            id: "page",
            minPercent: 40,
            defaultPercent: 76,
            content: selected ? (
              <PageView
                key={selected}
                projectPath={projectPath}
                path={selected}
                cards={cards ?? []}
                refreshTick={refreshTick}
                onAsk={onAskAi}
                onTitleCommitted={(path, title) => run(async () => { await retitle(path, title); onChanged(); })}
                onDelete={() => setDeleting({ kind: "page", name: baseName(selected), path: selected, title: "" })}
                onDuplicate={() => void duplicate(selected)}
                onSaved={onChanged}
                onOpen={setSelected}
                focusTitle={focusTitle}
              />
            ) : (
              <div className="h-full rounded-xl border border-border bg-card" />
            ),
          },
        ]}
      />
      {problem && (
        <p role="alert" className="absolute right-4 bottom-4 z-30 max-w-sm rounded-lg bg-destructive/15 px-3 py-2 text-xs break-words text-destructive">
          {problem}
        </p>
      )}
      <AlertDialog open={deleting !== null} onOpenChange={(open) => !open && setDeleting(null)}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete {deleting?.kind === "folder" ? "this folder" : "this page"}?</AlertDialogTitle>
            <AlertDialogDescription>
              {deleting?.kind === "folder"
                ? `The folder and its ${inside} page${inside === 1 ? "" : "s"} will be deleted.`
                : "It will be deleted."}{" "}
              Your game's history keeps earlier versions.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction variant="destructive" data-testid="confirm-delete" onClick={() => void confirmDelete()}>
              Delete
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}

