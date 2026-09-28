import type { InterviewAnswers } from "@/lib/studio-types";

// Plain data and pure helpers behind the interview screens — the fixed
// choices each question offers, the name suggestion, and the same project
// name rules the backend enforces — kept out of the components so each
// screen stays about layout.

export const EMPTY_ANSWERS: InterviewAnswers = {
  idea: "",
  genre: "",
  genre_other: null,
  feel: [],
  look: "",
  references: "",
  session_length: "",
  name: "",
};

/** Filled into the idea field when clicked — a nudge for people staring at
 * an empty box, not a default. */
export const IDEA_EXAMPLES = [
  "A cozy farm where you raise little slime pets",
  "A cat who jumps across rooftops to catch the moon",
  "A tiny spaceship fighting off waves of space bugs",
];

/** The `genre` for "Start from scratch": the blank template's id. */
export const BLANK_TEMPLATE_ID = "blank-2d";
/** The `genre` for "Something else": the backend picks a template from the
 * words in the idea. */
export const OTHER_GENRE = "other";

export const FEEL_OPTIONS = ["Cozy", "Fast", "Spooky", "Funny", "Challenging", "Relaxing", "Epic", "Cute"];

export const LOOK_OPTIONS = ["Pixel art", "Cartoon", "Simple shapes", "Hand-drawn", "Neon"];

export const SESSION_OPTIONS = [
  { value: "A couple of minutes", hint: "Quick rounds, easy to pick up" },
  { value: "5–15 minutes", hint: "A level or a run at a time" },
  { value: "Half an hour or more", hint: "Settle in and play for a while" },
];

// Words that end the "what it is" part of an idea: in "a cozy farm where
// you raise slimes", the name stops before "where".
const NAME_STOP_WORDS = new Set([
  "where", "who", "whose", "which", "that", "with", "and", "but", "or", "in", "on", "at", "to",
  "from", "for", "of", "by", "into", "across", "through", "while", "when", "you", "your", "must",
  "has", "have", "is", "are", "can", "tries", "wants", "needs",
]);

// Openers that describe the idea rather than name it ("a game about", "it's
// a story where", "a platformer about a"), stripped before picking words.
const NAME_LEAD_IN =
  /^\s*(?:it'?s\s+|this\s+is\s+)?(?:(?:a|an|the)\s+)?(?:(?:\w+\s+)?(?:game|story|platformer|shooter|adventure|rpg)\s+)?(?:(?:about|where|in\s+which|of|with)\s+)?(?:(?:a|an|the|some)\s+)?/i;

/** A starting name from the person's own idea: its first few words up to
 * where the description begins ("A cozy farm where you raise slimes" →
 * "Cozy Farm"), title-cased and stripped of characters a folder name can't
 * have. Only a suggestion — the field stays editable. */
export function suggestProjectName(idea: string): string {
  const words = idea
    .replace(NAME_LEAD_IN, "")
    .replace(/[^\p{L}\p{N}'\s-]/gu, " ")
    .split(/\s+/)
    .filter(Boolean);
  const picked: string[] = [];
  for (const word of words) {
    if (NAME_STOP_WORDS.has(word.toLowerCase()) && picked.length > 0) break;
    if (NAME_STOP_WORDS.has(word.toLowerCase())) continue;
    picked.push(word.charAt(0).toUpperCase() + word.slice(1));
    if (picked.length === 3) break;
  }
  const name = picked.join(" ").replace(/^[.'-]+|[.'-]+$/g, "").slice(0, 40).trim();
  return name || "My Game";
}

/** The same rules as `validate_name` in `crates/core/src/scaffold.rs`, so a
 * problem shows up while typing instead of after "Create my game". Returns
 * null when the name is fine. Keep in step with that function. */
export function projectNameProblem(name: string): string | null {
  if (name.trim() === "") return "Give your game a name.";
  if (/^\s|\s$/.test(name)) return "The name can't start or end with a space.";
  if (name.startsWith(".")) return "The name can't start with a dot.";
  if (name.endsWith(".")) return "The name can't end with a dot.";
  if (/[/\\:<>"|?*]/.test(name)) return 'The name can\'t contain any of / \\ : < > " | ? *';
  if (/[\u0000-\u001f\u007f-\u009f]/.test(name)) return "The name can't contain control characters.";
  // Windows reserves these device names, with or without an extension.
  const stem = (name.split(".")[0] ?? name).toUpperCase();
  if (["CON", "PRN", "AUX", "NUL"].includes(stem) || /^(COM|LPT)[1-9]$/.test(stem)) {
    return `"${name}" is a name Windows reserves — choose another.`;
  }
  return null;
}

/** `<parent>/<name>` with the parent's own separator (a Windows path picked
 * in the native dialog uses backslashes). */
export function joinPath(parent: string, name: string): string {
  const sep = parent.includes("\\") && !parent.includes("/") ? "\\" : "/";
  return `${parent.replace(/[\\/]+$/, "")}${sep}${name}`;
}

/** A Context card path (relative to `.ibproject/context/`) as people read
 * it: "tasks/first-playable.md" → { title: "First playable", group: "Tasks" }. */
export function friendlyCardName(path: string): { title: string; group: string | null } {
  const parts = path.split("/").filter(Boolean);
  const file = parts.pop() ?? path;
  const words = file.replace(/\.md$/i, "").replace(/[-_]+/g, " ").trim();
  const title = words.charAt(0).toUpperCase() + words.slice(1);
  const folder = parts.join(" / ").replace(/[-_]+/g, " ");
  return { title, group: folder ? folder.charAt(0).toUpperCase() + folder.slice(1) : null };
}
