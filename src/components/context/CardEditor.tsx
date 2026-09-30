import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AlertTriangle, FileCode, Link2, Plus, Undo2, X } from "lucide-react";
import { MarkdownEditor } from "@/components/cockpit/MarkdownEditor";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { contextRead, contextWrite } from "@/lib/studio-api";
import type { Card, CardMeta, CardSummary, CardType } from "@/lib/studio-types";
import { cn } from "@/lib/utils";
import { TypeBadge } from "./CardBadges";
import {
  CARD_TYPES,
  STATUS_SUGGESTIONS,
  TASK_STATUSES,
  errorText,
  statusLabel,
  typeInfo,
} from "./cardTypes";

// Where cards live inside a project (also see ContextSection).
const CONTEXT_DIR = ".ibproject/context";

interface CardEditorProps {
  projectPath: string;
  /** Relative to the context folder. */
  path: string;
  /** Every card, for the link picker and the "Linked from" list. */
  cards: CardSummary[] | null;
  /** Bumped whenever files on disk changed; the editor re-reads then. */
  refreshTick: number;
  onDirtyChange: (dirty: boolean) => void;
  /** After a successful save, so lists and boards can refresh. */
  onSaved: () => void;
  onOpen: (path: string) => void;
}

/** The right pane of the Cards tab: a header form (title, type, status,
 * tags, links, implemented-in), the Markdown body, Save / Discard, and the
 * cards linking here. Editors are uncontrolled, so the body editor is keyed
 * by path + a version that bumps whenever the draft is replaced from disk.
 * The parent keys this whole component by path. */
export function CardEditor({ projectPath, path, cards, refreshTick, onDirtyChange, onSaved, onOpen }: CardEditorProps) {
  const [saved, setSaved] = useState<Card | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [meta, setMeta] = useState<CardMeta | null>(null);
  const [body, setBody] = useState("");
  const [version, setVersion] = useState(0);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [changedOnDisk, setChangedOnDisk] = useState(false);

  const savedRef = useRef<Card | null>(null);
  const dirtyRef = useRef(false);

  const dirty = useMemo(() => {
    if (!saved || !meta) return false;
    return JSON.stringify(meta) !== JSON.stringify(saved.meta) || body !== saved.body;
  }, [saved, meta, body]);
  dirtyRef.current = dirty;

  useEffect(() => {
    onDirtyChange(dirty);
  }, [dirty, onDirtyChange]);
  // A card that's going away can't be dirty any more.
  useEffect(() => () => onDirtyChange(false), [onDirtyChange]);

  const adopt = useCallback((card: Card) => {
    savedRef.current = card;
    setSaved(card);
    setMeta(card.meta);
    setBody(card.body);
    setVersion((v) => v + 1);
    setChangedOnDisk(false);
  }, []);

  // Load on open, and quietly re-read when files change on disk: an
  // untouched card follows the disk, a half-edited one is never clobbered.
  const firstLoad = useRef(true);
  useEffect(() => {
    let cancelled = false;
    const initial = firstLoad.current;
    firstLoad.current = false;
    contextRead(projectPath, path).then(
      (card) => {
        if (cancelled) return;
        setLoadError(null);
        const current = savedRef.current;
        const same = current !== null && JSON.stringify(current) === JSON.stringify(card);
        if (initial || !current) adopt(card);
        else if (!same) {
          if (dirtyRef.current) setChangedOnDisk(true);
          else adopt(card);
        }
      },
      (err) => {
        if (!cancelled) setLoadError(errorText(err));
      },
    );
    return () => {
      cancelled = true;
    };
  }, [projectPath, path, refreshTick, adopt]);

  const patch = (partial: Partial<CardMeta>) => setMeta((m) => (m ? { ...m, ...partial } : m));

  const save = useCallback(async () => {
    const card = savedRef.current;
    if (!card || saving) return;
    setSaving(true);
    setSaveError(null);
    try {
      if (card.header_error !== null) {
        // The header couldn't be read, so only the body is ours to change:
        // swap the old body for the new one and leave every byte before it.
        const file = `${projectPath}/${CONTEXT_DIR}/${path}`;
        const current = await invoke<string>("read_file", { path: file });
        if (!current.endsWith(card.body)) {
          throw new Error("This file changed on disk since you opened it, so nothing was saved. Discard your changes and try again.");
        }
        const head = current.slice(0, current.length - card.body.length);
        await invoke("write_file", { path: file, contents: head + body });
        adopt(await contextRead(projectPath, path));
      } else if (meta) {
        const result = await contextWrite(projectPath, path, meta, body);
        savedRef.current = result;
        setSaved(result);
        setMeta(result.meta);
        setBody(result.body);
        setChangedOnDisk(false);
      }
      onSaved();
    } catch (err) {
      setSaveError(errorText(err));
    } finally {
      setSaving(false);
    }
  }, [adopt, body, meta, onSaved, path, projectPath, saving]);

  const discard = () => {
    if (savedRef.current) adopt(savedRef.current);
    setSaveError(null);
  };

  if (loadError) {
    return (
      <Shell>
        <div role="alert" className="m-4 rounded-lg bg-destructive/10 p-3 text-sm text-destructive">
          <p className="font-medium">This card couldn't be opened.</p>
          <p className="mt-1 break-words text-xs">{loadError}</p>
        </div>
      </Shell>
    );
  }
  if (!saved || !meta) {
    return (
      <Shell>
        <p className="p-4 text-sm text-muted-foreground">Opening card…</p>
      </Shell>
    );
  }

  const summary = cards?.find((c) => c.path === path) ?? null;
  const headerBroken = saved.header_error !== null;
  const backlinks = summary?.backlinks ?? [];

  return (
    <Shell>
      <div data-tauri-drag-region className="flex h-9 shrink-0 items-center gap-2 px-3">
        <TypeBadge type={meta.type} />
        <span data-tauri-drag-region className="min-w-0 flex-1 truncate font-mono text-[11px] text-muted-foreground">
          {path}
        </span>
        {dirty && (
          <span className="flex items-center gap-1 text-[11px] text-amber-300" data-testid="dirty-indicator">
            <span className="size-1.5 rounded-full bg-amber-400" /> Unsaved changes
          </span>
        )}
        <Button size="xs" variant="ghost" disabled={!dirty || saving} onClick={discard}>
          <Undo2 /> Discard changes
        </Button>
        <Button size="xs" disabled={!dirty || saving} onClick={() => void save()} data-testid="save-card">
          {saving ? "Saving…" : "Save"}
        </Button>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto border-t border-border">
        {saveError && (
          <p role="alert" className="mx-3 mt-3 rounded-lg bg-destructive/10 px-3 py-2 text-xs break-words text-destructive">
            {saveError}
          </p>
        )}
        {changedOnDisk && (
          <p className="mx-3 mt-3 rounded-lg bg-amber-500/10 px-3 py-2 text-xs text-amber-200">
            This card was changed outside the editor. Saving will replace that change; "Discard changes" loads it.
          </p>
        )}

        {headerBroken ? (
          <HeaderProblem raw={saved.header_error ?? ""} />
        ) : (
          <HeaderForm meta={meta} patch={patch} path={path} cards={cards} summary={summary} onOpen={onOpen} />
        )}

        <div className="px-3 pt-3">
          <p className="mb-1 text-xs font-medium text-muted-foreground">
            {headerBroken ? "Card text (you can still edit this)" : "Card text"}
          </p>
          <div className="h-[24rem] overflow-hidden rounded-lg border border-border" data-testid="card-body">
            <MarkdownEditor
              key={`${path}:${version}`}
              initialValue={saved.body}
              onChange={setBody}
              onSave={() => void save()}
            />
          </div>
        </div>

        <div className="px-3 py-3">
          <p className="mb-1 text-xs font-medium text-muted-foreground">Linked from</p>
          {backlinks.length === 0 ? (
            <p className="text-xs text-muted-foreground">No other card links here yet.</p>
          ) : (
            <ul className="flex flex-wrap gap-1.5">
              {backlinks.map((p) => {
                const from = cards?.find((c) => c.path === p);
                return (
                  <li key={p}>
                    <button
                      type="button"
                      onClick={() => onOpen(p)}
                      className="inline-flex h-6 items-center gap-1.5 rounded-full border border-border px-2 text-xs hover:bg-muted"
                    >
                      <Link2 className="size-3 text-muted-foreground" />
                      {from?.title ?? p}
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      </div>
    </Shell>
  );
}

function Shell({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex h-full min-h-0 flex-col rounded-xl border border-border bg-card" data-testid="card-editor">
      {children}
    </div>
  );
}

function HeaderProblem({ raw }: { raw: string }) {
  return (
    <div className="m-3 rounded-lg border border-amber-500/40 bg-amber-500/5 p-3" data-testid="header-error">
      <p className="flex items-center gap-1.5 text-sm font-medium text-amber-200">
        <AlertTriangle className="size-4" /> Couldn't read this card's header
      </p>
      <p className="mt-1 text-xs text-muted-foreground">
        The block at the top of this file (title, type, status, links) isn't written in a form InfinaBox understands, so
        it can't be edited here and it won't be touched. You can still edit the card text below. To fix the header, edit
        the file in Code or ask your AI to repair it.
      </p>
      <pre className="mt-2 max-h-40 overflow-auto rounded-md bg-background/60 p-2 font-mono text-[11px] whitespace-pre-wrap break-all">
        {raw}
      </pre>
    </div>
  );
}

interface HeaderFormProps {
  meta: CardMeta;
  patch: (partial: Partial<CardMeta>) => void;
  path: string;
  cards: CardSummary[] | null;
  summary: CardSummary | null;
  onOpen: (path: string) => void;
}

function HeaderForm({ meta, patch, path, cards, summary, onOpen }: HeaderFormProps) {
  const isTask = meta.type === "task";
  const status = meta.status ?? "";
  const statusChoices: string[] = [...TASK_STATUSES];
  if (status && !statusChoices.includes(status)) statusChoices.push(status);
  const broken = new Set(summary?.broken_links ?? []);

  return (
    <div className="grid gap-3 p-3 sm:grid-cols-2" data-testid="card-header-form">
      <Field label="Title" className="sm:col-span-2">
        <Input
          value={meta.title ?? ""}
          aria-label="Title"
          placeholder={summary?.title ?? "Untitled"}
          onChange={(e) => patch({ title: e.target.value === "" ? null : e.target.value })}
        />
      </Field>

      <Field label="Type">
        <select
          value={meta.type ?? "other"}
          aria-label="Type"
          onChange={(e) => patch({ type: e.target.value as CardType })}
          className={SELECT_CLASS}
        >
          {CARD_TYPES.map((t) => (
            <option key={t.id} value={t.id}>
              {t.label}
            </option>
          ))}
        </select>
      </Field>

      <Field label="Status">
        {isTask ? (
          <select
            value={status}
            onChange={(e) => patch({ status: e.target.value })}
            className={SELECT_CLASS}
            aria-label="Status"
          >
            {!status && <option value="">No status</option>}
            {statusChoices.map((s) => (
              <option key={s} value={s}>
                {statusLabel(s)}
              </option>
            ))}
          </select>
        ) : (
          <>
            <Input
              value={status}
              list="card-status-suggestions"
              placeholder="draft, working, final…"
              aria-label="Status"
              onChange={(e) => patch({ status: e.target.value === "" ? null : e.target.value })}
            />
            <datalist id="card-status-suggestions">
              {STATUS_SUGGESTIONS.map((s) => (
                <option key={s} value={s} />
              ))}
            </datalist>
          </>
        )}
      </Field>

      <Field label="Tags" className="sm:col-span-2">
        <ChipInput
          values={meta.tags}
          onChange={(tags) => patch({ tags })}
          placeholder="Add a tag and press Enter"
        />
      </Field>

      <Field label="Links to other cards" className="sm:col-span-2">
        <LinkEditor
          links={meta.links}
          broken={broken}
          selfPath={path}
          cards={cards}
          onChange={(links) => patch({ links })}
          onOpen={onOpen}
        />
      </Field>

      <Field label="Implemented in" className="sm:col-span-2">
        {meta.implemented_in.length === 0 ? (
          <p className="text-xs text-muted-foreground">
            No game files yet. Your AI fills this in when it builds this card.
          </p>
        ) : (
          <ul className="flex flex-wrap gap-1.5">
            {meta.implemented_in.map((f) => (
              <li
                key={f}
                className="inline-flex h-6 items-center gap-1 rounded-md bg-muted px-2 font-mono text-[11px]"
                title={f}
              >
                <FileCode className="size-3 text-muted-foreground" />
                {f}
              </li>
            ))}
          </ul>
        )}
      </Field>
    </div>
  );
}

const SELECT_CLASS =
  "h-8 w-full rounded-lg border border-input bg-transparent px-2 text-sm text-foreground outline-none focus-visible:border-ring dark:bg-input/30";

function Field({ label, className, children }: { label: string; className?: string; children: React.ReactNode }) {
  return (
    <div className={cn("grid content-start gap-1", className)}>
      <span className="text-xs font-medium text-muted-foreground">{label}</span>
      {children}
    </div>
  );
}

/** Removable chips plus a text box that adds one on Enter or comma. */
function ChipInput({
  values,
  onChange,
  placeholder,
}: {
  values: string[];
  onChange: (values: string[]) => void;
  placeholder: string;
}) {
  const [text, setText] = useState("");
  const add = () => {
    const v = text.trim().replace(/,+$/, "").trim();
    if (v && !values.includes(v)) onChange([...values, v]);
    setText("");
  };
  return (
    <div className="flex flex-wrap items-center gap-1.5">
      {values.map((v) => (
        <Chip key={v} label={v} onRemove={() => onChange(values.filter((x) => x !== v))} />
      ))}
      <Input
        value={text}
        placeholder={placeholder}
        aria-label={placeholder}
        onChange={(e) => setText(e.target.value)}
        onBlur={add}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === ",") {
            e.preventDefault();
            add();
          } else if (e.key === "Backspace" && text === "" && values.length > 0) {
            onChange(values.slice(0, -1));
          }
        }}
        className="h-7 w-48 text-xs"
      />
    </div>
  );
}

function Chip({
  label,
  onRemove,
  onClick,
  tone = "normal",
  title,
}: {
  label: string;
  onRemove: () => void;
  onClick?: () => void;
  tone?: "normal" | "broken";
  title?: string;
}) {
  return (
    <span
      title={title}
      className={cn(
        "inline-flex h-6 items-center gap-1 rounded-full border pr-0.5 pl-2 text-xs",
        tone === "broken" ? "border-dashed border-destructive/70 text-destructive" : "border-border bg-muted/50",
      )}
    >
      {onClick ? (
        <button type="button" onClick={onClick} className="max-w-48 truncate hover:underline">
          {label}
        </button>
      ) : (
        <span className="max-w-48 truncate">{label}</span>
      )}
      <button
        type="button"
        aria-label={`Remove ${label}`}
        onClick={onRemove}
        className="flex size-5 items-center justify-center rounded-full hover:bg-foreground/10"
      >
        <X className="size-3" />
      </button>
    </span>
  );
}

function LinkEditor({
  links,
  broken,
  selfPath,
  cards,
  onChange,
  onOpen,
}: {
  links: string[];
  broken: Set<string>;
  selfPath: string;
  cards: CardSummary[] | null;
  onChange: (links: string[]) => void;
  onOpen: (path: string) => void;
}) {
  const [picking, setPicking] = useState(false);
  const [query, setQuery] = useState("");

  const candidates = useMemo(() => {
    const q = query.trim().toLowerCase();
    return (cards ?? []).filter(
      (c) =>
        c.path !== selfPath &&
        !links.includes(c.path) &&
        (!q || c.title.toLowerCase().includes(q) || c.path.toLowerCase().includes(q)),
    );
  }, [cards, links, query, selfPath]);

  return (
    <div className="grid gap-2">
      <div className="flex flex-wrap items-center gap-1.5">
        {links.map((l) => {
          const target = cards?.find((c) => c.path === l);
          const isBroken = broken.has(l) || (cards !== null && !target);
          return (
            <Chip
              key={l}
              label={target?.title ?? l}
              tone={isBroken ? "broken" : "normal"}
              title={isBroken ? `No card at ${l}` : l}
              onClick={isBroken ? undefined : () => onOpen(l)}
              onRemove={() => onChange(links.filter((x) => x !== l))}
            />
          );
        })}
        <Button size="xs" variant="outline" onClick={() => setPicking((p) => !p)} aria-expanded={picking}>
          <Plus /> Add link
        </Button>
      </div>
      {picking && (
        <div className="rounded-lg border border-border bg-background/40 p-2" data-testid="link-picker">
          <Input
            autoFocus
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search cards to link"
            aria-label="Search cards to link"
            className="h-7 text-xs"
          />
          <ul className="mt-1.5 max-h-36 overflow-y-auto">
            {candidates.length === 0 ? (
              <li className="px-2 py-1.5 text-xs text-muted-foreground">No other cards to link.</li>
            ) : (
              candidates.map((c) => (
                <li key={c.path}>
                  <button
                    type="button"
                    onClick={() => {
                      onChange([...links, c.path]);
                      setQuery("");
                      setPicking(false);
                    }}
                    className="flex w-full items-center gap-2 rounded-md px-2 py-1 text-left text-xs hover:bg-muted"
                  >
                    <span
                      className="size-2 shrink-0 rounded-full"
                      style={{ backgroundColor: typeInfo(c.card_type).color }}
                    />
                    <span className="truncate">{c.title}</span>
                    <span className="ml-auto truncate font-mono text-[10px] text-muted-foreground">{c.path}</span>
                  </button>
                </li>
              ))
            )}
          </ul>
        </div>
      )}
    </div>
  );
}
