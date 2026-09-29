import type { CardMeta, CardType } from "@/lib/studio-types";

// Everything the Context views share about card types: what to call them,
// where a new card of that type goes, and the colour it wears everywhere
// (list badge, Board card, Map node). The type set mirrors
// `CardType` in `src/lib/studio-types.ts` (crates/core/src/context_cards.rs).

export interface CardTypeInfo {
  id: CardType;
  label: string;
  /** One line for the "New card" picker. */
  blurb: string;
  /** Folder inside the context folder new cards of this type go in ("" = root). */
  folder: string;
  /** Badge / node colour. */
  color: string;
  /** Prompt written into a new card's body. */
  bodyPrompt: string;
}

export const CARD_TYPES: CardTypeInfo[] = [
  {
    id: "concept",
    label: "Concept",
    blurb: "The big idea: what the game is, what makes it special, how it should feel.",
    folder: "",
    color: "#f59e0b",
    bodyPrompt: "Describe the idea in a few sentences. What is the game, and what should it feel like to play?",
  },
  {
    id: "mechanic",
    label: "Mechanic",
    blurb: "Something the player can do or that happens in the game, like jumping or crafting.",
    folder: "mechanics",
    color: "#38bdf8",
    bodyPrompt: "What does this do, and how should it feel? Note any numbers you want to tune (speed, damage, cooldown).",
  },
  {
    id: "character",
    label: "Character",
    blurb: "The player, an enemy, or anyone else who lives in the game.",
    folder: "characters",
    color: "#f472b6",
    bodyPrompt: "Who is this? What do they look like, how do they behave, what do they want?",
  },
  {
    id: "level",
    label: "Level",
    blurb: "A place in the game: its layout, how it flows, how hard it is.",
    folder: "levels",
    color: "#34d399",
    bodyPrompt: "Describe the place and what the player does there, from start to finish.",
  },
  {
    id: "story",
    label: "Story",
    blurb: "Story beats, dialogue and what happens in what order.",
    folder: "story",
    color: "#a78bfa",
    bodyPrompt: "Write the story beats or the dialogue here, in order.",
  },
  {
    id: "asset",
    label: "Asset",
    blurb: "A picture, sound or model, with where it came from and its license.",
    folder: "assets",
    color: "#fb923c",
    bodyPrompt: "What is this asset for, and where did it come from?",
  },
  {
    id: "style-guide",
    label: "Style guide",
    blurb: "How the game looks and sounds: colours, art style, audio mood.",
    folder: "",
    color: "#e879f9",
    bodyPrompt: "Describe the look and sound of the game: art style, colours, music mood.",
  },
  {
    id: "task",
    label: "Task",
    blurb: "A thing to do or a bug to fix, tracked on the Board.",
    folder: "tasks",
    color: "#60a5fa",
    bodyPrompt: "What needs doing, and why? For a bug: what happens, and what should happen instead?",
  },
  {
    id: "playtest",
    label: "Playtest",
    blurb: "Notes and feedback from someone playing the game.",
    folder: "playtests",
    color: "#2dd4bf",
    bodyPrompt: "Who played, what did they do, what confused or delighted them?",
  },
  {
    id: "other",
    label: "Other",
    blurb: "Anything else you want your AI to know.",
    folder: "",
    color: "#94a3b8",
    bodyPrompt: "Write whatever you want your AI to remember.",
  },
];

export const CARD_TYPE_INFO: Record<CardType, CardTypeInfo> = Object.fromEntries(
  CARD_TYPES.map((t) => [t.id, t]),
) as Record<CardType, CardTypeInfo>;

export function typeInfo(type: CardType | null | undefined): CardTypeInfo {
  return CARD_TYPE_INFO[type ?? "other"] ?? CARD_TYPE_INFO.other;
}

/** The three columns of the Board, in order. */
export const TASK_STATUSES = ["todo", "doing", "done"] as const;

export const STATUS_LABELS: Record<string, string> = {
  todo: "To do",
  doing: "Doing",
  done: "Done",
};

export function statusLabel(status: string | null | undefined): string {
  if (!status) return "No status";
  return STATUS_LABELS[status] ?? status;
}

/** Suggested statuses for cards that aren't tasks (free text is allowed). */
export const STATUS_SUGGESTIONS = ["draft", "working", "needs work", "final"];

/** Tailwind classes for a status pill. */
export function statusClasses(status: string | null | undefined): string {
  switch (status) {
    case "todo":
      return "bg-muted text-muted-foreground";
    case "doing":
      return "bg-amber-500/15 text-amber-300";
    case "done":
    case "final":
      return "bg-emerald-500/15 text-emerald-300";
    default:
      return "bg-secondary text-secondary-foreground";
  }
}

/** File-name-safe version of a title: lowercase words joined with dashes. */
export function slugify(title: string): string {
  const slug = title
    .normalize("NFKD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 60)
    .replace(/-+$/g, "");
  return slug || "card";
}

/** `<type folder>/<slug>.md`, made unique against `existing` paths. */
export function newCardPath(type: CardType, title: string, existing: Iterable<string>): string {
  const taken = new Set(existing);
  const folder = CARD_TYPE_INFO[type].folder;
  const prefix = folder ? `${folder}/` : "";
  const slug = slugify(title);
  let candidate = `${prefix}${slug}.md`;
  for (let n = 2; taken.has(candidate); n += 1) candidate = `${prefix}${slug}-${n}.md`;
  return candidate;
}

/** Front-matter and body for a brand-new card. */
export function newCardContent(
  type: CardType,
  title: string,
  extraTags: string[] = [],
): { meta: CardMeta; body: string } {
  return {
    meta: {
      type,
      title,
      status: type === "task" ? "todo" : "draft",
      links: [],
      implemented_in: [],
      tags: extraTags,
      extra: {},
    },
    body: `${CARD_TYPE_INFO[type].bodyPrompt}\n`,
  };
}

/** A backend error as plain text (Tauri rejects with the message string). */
export function errorText(err: unknown): string {
  if (typeof err === "string") return err;
  if (err instanceof Error) return err.message;
  return String(err);
}

/** Badge styling from a type colour (tinted background, coloured text). */
export function badgeStyle(color: string): { color: string; backgroundColor: string } {
  return { color, backgroundColor: `${color}26` };
}
