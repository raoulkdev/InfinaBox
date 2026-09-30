import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Popover } from "radix-ui";
import { ClipboardCopy, MoreHorizontal, SlidersHorizontal, Sparkles, X } from "lucide-react";
import { MarkdownEditor } from "@/components/cockpit/MarkdownEditor";
import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { contextRead, contextWrite } from "@/lib/studio-api";
import type { Card, CardMeta, CardSummary, CardType } from "@/lib/studio-types";
import { DOC_ACTIONS, questionAbout, type AskAi } from "./aiActions";
import { CARD_TYPES, STATUS_SUGGESTIONS, errorText } from "./cardTypes";

// One page of the notes: a big title and a body you just type in. It saves
// by itself a moment after you stop typing. Everything else (what kind of
// page it is, its status and tags, what it links to, asking the AI) is
// tucked into the two buttons in the corner.

const AUTOSAVE_MS = 700;

interface PageViewProps {
  projectPath: string;
  path: string;
  cards: CardSummary[];
  /** Bumped when files change on disk. */
  refreshTick: number;
  onAsk: AskAi;
  /** The title was committed; the page may want a new file name. */
  onTitleCommitted: (path: string, title: string) => Promise<void>;
  onDelete: () => void;
  onDuplicate: () => void;
  onSaved: () => void;
  onOpen: (path: string) => void;
  /** Change every time a brand-new page is opened, to focus its title. */
  focusTitle: number;
}

type Save = "saved" | "saving" | "error";

export function PageView({ projectPath, path, cards, refreshTick, onAsk, onTitleCommitted, onDelete, onDuplicate, onSaved, onOpen, focusTitle }: PageViewProps) {
  const [loaded, setLoaded] = useState<Card | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [title, setTitle] = useState("");
  const [meta, setMeta] = useState<CardMeta | null>(null);
  const [body, setBody] = useState("");
  const [version, setVersion] = useState(0);
  const [save, setSave] = useState<Save>("saved");
  const [saveError, setSaveError] = useState<string | null>(null);

  // What is on disk as far as this page knows, and whether there are edits not yet written.
  const savedRef = useRef({ title: "", body: "", meta: null as CardMeta | null });
  const dirtyRef = useRef(false);
  const latest = useRef({ title, body, meta });
  latest.current = { title, body, meta };
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const titleInput = useRef<HTMLInputElement | null>(null);

  const apply = useCallback((card: Card) => {
    setLoaded(card);
    const t = card.meta.title ?? "";
    setTitle(t);
    setMeta(card.meta);
    setBody(card.body);
    savedRef.current = { title: t, body: card.body, meta: card.meta };
    dirtyRef.current = false;
    setVersion((v) => v + 1);
  }, []);

  useEffect(() => {
    let cancelled = false;
    setLoaded(null);
    setLoadError(null);
    contextRead(projectPath, path).then(
      (card) => !cancelled && apply(card),
      (err) => !cancelled && setLoadError(errorText(err)),
    );
    return () => {
      cancelled = true;
    };
    // Only when the page itself changes; disk changes are handled below.
  }, [projectPath, path, apply]);

  // Changes on disk (the AI wrote to this page): take them unless there are unsaved edits here.
  useEffect(() => {
    if (!loaded || refreshTick === 0) return;
    let cancelled = false;
    contextRead(projectPath, path).then((card) => {
      if (cancelled || dirtyRef.current) return;
      const same = card.body === savedRef.current.body && (card.meta.title ?? "") === savedRef.current.title;
      if (!same) apply(card);
    }, () => {});
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refreshTick]);

  const write = useCallback(async () => {
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
    const { title: t, body: b, meta: m } = latest.current;
    if (!m || !dirtyRef.current) return;
    setSave("saving");
    try {
      const next = { ...m, title: t.trim() || null };
      const card = await contextWrite(projectPath, path, next, b);
      savedRef.current = { title: card.meta.title ?? "", body: b, meta: card.meta };
      // Edits made while this was writing stay pending.
      dirtyRef.current = latest.current.title !== t || latest.current.body !== b || latest.current.meta !== m;
      setSaveError(null);
      setSave(dirtyRef.current ? "saving" : "saved");
      if (dirtyRef.current) timer.current = setTimeout(() => void write(), AUTOSAVE_MS);
      onSaved();
    } catch (err) {
      setSave("error");
      setSaveError(errorText(err));
    }
  }, [projectPath, path, onSaved]);

  const touch = useCallback(() => {
    dirtyRef.current = true;
    setSave("saving");
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => void write(), AUTOSAVE_MS);
  }, [write]);

  // Leaving the page (or the app) writes whatever is pending.
  useEffect(() => {
    return () => {
      if (dirtyRef.current) void write();
    };
  }, [write]);

  useEffect(() => {
    if (loaded && focusTitle > 0 && (loaded.meta.title ?? "") === "Untitled") {
      titleInput.current?.focus();
      titleInput.current?.select();
    }
  }, [loaded, focusTitle]);

  const backlinks = useMemo(() => cards.find((c) => c.path === path)?.backlinks ?? [], [cards, path]);

  if (loadError) {
    return (
      <div role="alert" className="m-4 rounded-lg bg-destructive/10 p-3 text-sm text-destructive">
        <p className="font-medium">This page couldn't be opened.</p>
        <p className="mt-1 text-xs break-words">{loadError}</p>
      </div>
    );
  }
  if (!loaded || !meta) return <div className="h-full rounded-xl border border-border bg-card" />;

  const broken = loaded.header_error !== null;

  const setMetaField = (patch: Partial<CardMeta>) => {
    setMeta((m) => (m ? { ...m, ...patch } : m));
    touch();
  };

  return (
    <div className="flex h-full min-h-0 flex-col rounded-xl border border-border bg-card" data-testid="page-view">
      <div data-tauri-drag-region className="flex h-10 shrink-0 items-center justify-end gap-1 px-3">
        <span className="mr-1 text-xs text-muted-foreground" data-testid="save-state" aria-live="polite">
          {save === "saving" ? "Saving…" : save === "error" ? "Couldn't save" : ""}
        </span>
        {!broken && (
          <>
            <AiMenu path={path} onAsk={onAsk} />
            <PropertiesMenu meta={meta} cards={cards} path={path} backlinks={backlinks} onChange={setMetaField} onOpen={onOpen} />
          </>
        )}
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button size="icon-xs" variant="ghost" aria-label="Page menu" data-testid="page-menu">
              <MoreHorizontal />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="w-44">
            <DropdownMenuItem onSelect={() => void navigator.clipboard.writeText(`# ${title || "Untitled"}\n\n${body}`)}>
              <ClipboardCopy /> Copy as Markdown
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => void write().then(onDuplicate)}>Duplicate</DropdownMenuItem>
            <DropdownMenuItem variant="destructive" onSelect={onDelete}>
              Delete
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>

      {saveError && (
        <p role="alert" className="mx-6 mb-1 rounded-lg bg-destructive/10 px-3 py-2 text-xs break-words text-destructive">
          {saveError}
        </p>
      )}

      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex min-h-full w-full max-w-3xl flex-col px-8 pb-10">
          <input
            ref={titleInput}
            value={title}
            disabled={broken}
            placeholder="Untitled"
            aria-label="Title"
            data-testid="page-title"
            onChange={(e) => {
              setTitle(e.target.value);
              touch();
            }}
            onBlur={() => {
              void write().then(() => onTitleCommitted(path, latest.current.title.trim()));
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") (e.target as HTMLInputElement).blur();
            }}
            className="w-full bg-transparent pt-4 pb-3 text-3xl font-semibold tracking-tight outline-none placeholder:text-muted-foreground/50"
          />
          {broken ? (
            <div className="rounded-lg border border-amber-500/40 bg-amber-500/5 p-3 text-sm" data-testid="header-error">
              This page's header can't be read, so it's shown as it is on disk. Fix it in Code.
              <pre className="mt-2 max-h-64 overflow-auto text-xs whitespace-pre-wrap select-text">{loaded.header_error}</pre>
            </div>
          ) : (
            <div className="min-h-[60vh] flex-1" data-testid="page-body">
              <MarkdownEditor
                key={`${path}:${version}`}
                initialValue={body}
                onChange={(value) => {
                  setBody(value);
                  touch();
                }}
                onSave={() => void write()}
              />
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function AiMenu({ path, onAsk }: { path: string; onAsk: AskAi }) {
  const [open, setOpen] = useState(false);
  const [question, setQuestion] = useState("");
  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <Popover.Trigger asChild>
        <Button size="xs" variant="ghost" data-testid="ask-ai">
          <Sparkles /> Ask AI
        </Button>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content align="end" sideOffset={6} className="z-50 flex w-64 flex-col gap-1 rounded-xl border border-border bg-popover p-2 text-popover-foreground shadow-lg outline-none">
          {DOC_ACTIONS.map((a) => (
            <button
              key={a.id}
              type="button"
              data-testid={`doc-action-${a.id}`}
              title={a.hint}
              onClick={() => {
                setOpen(false);
                onAsk(a.ask(path));
              }}
              className="rounded-md px-2 py-1.5 text-left text-sm hover:bg-accent"
            >
              {a.label}
            </button>
          ))}
          <form
            className="mt-1 flex gap-1.5 border-t border-border pt-2"
            onSubmit={(e) => {
              e.preventDefault();
              if (!question.trim()) return;
              setOpen(false);
              onAsk(questionAbout(path, question));
              setQuestion("");
            }}
          >
            <Input value={question} onChange={(e) => setQuestion(e.target.value)} placeholder="Ask about this page…" aria-label="Ask about this page" data-testid="doc-question" className="h-8 text-xs" />
            <Button type="submit" size="sm" disabled={!question.trim()}>
              Ask
            </Button>
          </form>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}

function PropertiesMenu({ meta, cards, path, backlinks, onChange, onOpen }: { meta: CardMeta; cards: CardSummary[]; path: string; backlinks: string[]; onChange: (patch: Partial<CardMeta>) => void; onOpen: (path: string) => void }) {
  const others = cards.filter((c) => c.path !== path && !meta.links.includes(c.path));
  const titleOf = (p: string) => cards.find((c) => c.path === p)?.title ?? p;
  return (
    <Popover.Root>
      <Popover.Trigger asChild>
        <Button size="icon-xs" variant="ghost" aria-label="Properties" data-testid="properties">
          <SlidersHorizontal />
        </Button>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content align="end" sideOffset={6} className="z-50 flex w-72 flex-col gap-3 rounded-xl border border-border bg-popover p-3 text-sm text-popover-foreground shadow-lg outline-none">
          <Field label="Kind">
            <select
              value={meta.type ?? ""}
              data-testid="prop-kind"
              onChange={(e) => onChange({ type: (e.target.value || null) as CardType | null })}
              className="h-8 w-full rounded-lg border border-input bg-transparent px-2 text-sm dark:bg-input/30"
            >
              <option value="">None</option>
              {CARD_TYPES.filter((t) => t.id !== "other").map((t) => (
                <option key={t.id} value={t.id}>
                  {t.label}
                </option>
              ))}
            </select>
          </Field>
          <Field label="Status">
            <Input
              list="status-suggestions"
              value={meta.status ?? ""}
              placeholder="None"
              data-testid="prop-status"
              onChange={(e) => onChange({ status: e.target.value || null })}
              className="h-8"
            />
            <datalist id="status-suggestions">
              {["todo", "doing", "done", ...STATUS_SUGGESTIONS].map((s) => (
                <option key={s} value={s} />
              ))}
            </datalist>
          </Field>
          <Field label="Tags">
            <Input
              value={meta.tags.join(", ")}
              placeholder="Add tags, separated by commas"
              data-testid="prop-tags"
              onChange={(e) => onChange({ tags: e.target.value.split(",").map((t) => t.trim()).filter(Boolean) })}
              className="h-8"
            />
          </Field>
          <Field label="Links">
            <div className="flex flex-wrap gap-1">
              {meta.links.map((l) => (
                <span key={l} className="inline-flex items-center gap-1 rounded-md bg-muted px-1.5 py-0.5 text-xs">
                  <button type="button" onClick={() => onOpen(l)} className="max-w-32 truncate hover:underline">
                    {titleOf(l)}
                  </button>
                  <button type="button" aria-label={`Remove link to ${titleOf(l)}`} onClick={() => onChange({ links: meta.links.filter((x) => x !== l) })}>
                    <X className="size-3" />
                  </button>
                </span>
              ))}
            </div>
            {others.length > 0 && (
              <select
                value=""
                data-testid="prop-add-link"
                onChange={(e) => e.target.value && onChange({ links: [...meta.links, e.target.value] })}
                className="mt-1 h-8 w-full rounded-lg border border-input bg-transparent px-2 text-sm dark:bg-input/30"
              >
                <option value="">Link to a page…</option>
                {others.map((c) => (
                  <option key={c.path} value={c.path}>
                    {c.title}
                  </option>
                ))}
              </select>
            )}
          </Field>
          {backlinks.length > 0 && (
            <Field label="Linked from">
              <div className="flex flex-wrap gap-1">
                {backlinks.map((b) => (
                  <button key={b} type="button" onClick={() => onOpen(b)} className="max-w-40 truncate rounded-md bg-muted px-1.5 py-0.5 text-xs hover:underline">
                    {titleOf(b)}
                  </button>
                ))}
              </div>
            </Field>
          )}
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-1">
      <span className="text-xs font-medium text-muted-foreground">{label}</span>
      {children}
    </div>
  );
}

