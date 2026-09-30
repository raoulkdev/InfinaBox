import type { CardMeta, CardType } from "@/lib/studio-types";
import { newCardPath } from "./cardTypes";

// The document templates offered by "New document": each is a card type plus
// a body with the headings a person (and their AI) would want filled in.
// The card is written once; from then on it's an ordinary card.

export interface DocTemplate {
  id: string;
  label: string;
  /** One line under the name in the gallery. */
  blurb: string;
  type: CardType;
  tags?: string[];
  status?: string;
  body: (title: string) => string;
}

const section = (heading: string, hint: string) => `## ${heading}\n\n_${hint}_\n`;

export const DOC_TEMPLATES: DocTemplate[] = [
  {
    id: "gdd",
    label: "Game design document",
    blurb: "The whole game on one page: pillars, loop, mechanics, world, audience.",
    type: "concept",
    body: (t) =>
      `# ${t}\n\n${section("Pitch", "The game in one or two sentences.")}\n${section("Pillars", "The three things this game must get right.")}\n${section("Core loop", "What the player does again and again, and why it's fun.")}\n${section("Mechanics", "What the player can do. Link a Mechanic card for each.")}\n${section("World and story", "Where and when, and what's going on.")}\n${section("Look and sound", "Art style, colours, music mood.")}\n${section("Audience", "Who this is for.")}\n${section("Scope", "What's in the first version, and what's not.")}`,
  },
  {
    id: "pitch",
    label: "One-page pitch",
    blurb: "A short sell of the idea, for a friend, a team, or a publisher.",
    type: "concept",
    body: (t) =>
      `# ${t}\n\n${section("Hook", "One line that makes someone say “tell me more”.")}\n${section("What you do", "The player's main verbs, in plain words.")}\n${section("What's special", "Why this and not another game.")}\n${section("Comparable games", "Two or three games it's like, and how it differs.")}`,
  },
  {
    id: "mechanic",
    label: "Mechanic spec",
    blurb: "How one thing works: what it does, the numbers, edge cases.",
    type: "mechanic",
    body: (t) =>
      `# ${t}\n\n${section("What it does", "Describe it from the player's point of view.")}\n${section("Feel", "What should it feel like to use?")}\n## Tuning values\n\n| Value | Default | Notes |\n|---|---|---|\n|  |  |  |\n\n${section("Edge cases", "What happens when things go wrong or collide?")}`,
  },
  {
    id: "character",
    label: "Character sheet",
    blurb: "Who they are, how they look and behave, what they want.",
    type: "character",
    body: (t) =>
      `# ${t}\n\n${section("Who they are", "Role in the game and a line of personality.")}\n${section("Look", "Shape, colours, size, how they move.")}\n${section("Behaviour", "What they do, when, and how they react to the player.")}\n${section("Numbers", "Health, speed, damage: whatever applies.")}\n${section("Lines", "A few things they say, if they talk.")}`,
  },
  {
    id: "level",
    label: "Level design",
    blurb: "A place in the game: layout, flow, difficulty, what's in it.",
    type: "level",
    body: (t) =>
      `# ${t}\n\n${section("Goal", "What the player has to do here.")}\n${section("Layout", "Rooms, paths, landmarks: a sketch in words.")}\n${section("Flow", "What the player meets, in order, and how the challenge builds.")}\n${section("Contents", "Enemies, pickups, hazards, secrets.")}\n${section("Difficulty", "What should be hard, and what should be easy.")}`,
  },
  {
    id: "story",
    label: "Story outline",
    blurb: "The story beat by beat, from the start to the end.",
    type: "story",
    body: (t) =>
      `# ${t}\n\n${section("Setup", "Who, where, and what's normal.")}\n${section("Turning points", "The moments that change things, in order.")}\n${section("Ending", "How it ends, and any different endings.")}\n${section("Themes", "What the story is really about.")}`,
  },
  {
    id: "dialogue",
    label: "Dialogue scene",
    blurb: "A conversation, line by line, with the choices the player gets.",
    type: "story",
    body: (t) =>
      `# ${t}\n\n${section("Setting", "Where this happens and who is there.")}\n## Script\n\n**Name:** First line.\n\n**Name:** Reply.\n\n${section("Choices", "What the player can say, and where each leads.")}`,
  },
  {
    id: "style",
    label: "Style guide",
    blurb: "How everything should look and sound, so it all fits together.",
    type: "style-guide",
    body: (t) =>
      `# ${t}\n\n${section("Art style", "Pixel art, flat shapes, hand-drawn…")}\n## Colours\n\n| Name | Colour | Used for |\n|---|---|---|\n|  |  |  |\n\n${section("Sound and music", "The mood, and the kind of instruments.")}\n${section("Words", "How the game talks to the player.")}`,
  },
  {
    id: "playtest",
    label: "Playtest report",
    blurb: "What happened when someone played, and what to change.",
    type: "playtest",
    body: (t) =>
      `# ${t}\n\n${section("Who and when", "Who played, and how experienced they are.")}\n${section("What they did", "Where they went, what they tried.")}\n${section("Confusing", "Where they got stuck or misunderstood.")}\n${section("Loved", "What made them smile.")}\n${section("Changes to make", "What to fix first.")}`,
  },
  {
    id: "task",
    label: "Task",
    blurb: "One thing to do, tracked on the Board.",
    type: "task",
    status: "todo",
    body: () => `What needs doing, and why?\n\n## Done when\n\n- [ ] \n`,
  },
  {
    id: "bug",
    label: "Bug report",
    blurb: "Something broken: what happens, what should happen.",
    type: "task",
    status: "todo",
    tags: ["bug"],
    body: () => `## What happens\n\n## What should happen\n\n## How to make it happen\n\n1. \n`,
  },
  {
    id: "store",
    label: "Store page",
    blurb: "The words for a game page: title, short blurb, feature list.",
    type: "other",
    body: (t) =>
      `# ${t}\n\n${section("Short blurb", "Two sentences that sell the game.")}\n${section("Features", "A short list of what makes it worth playing.")}\n${section("Tags", "Genre and mood words people search for.")}\n${section("Screenshots to take", "Which moments show the game best.")}`,
  },
  {
    id: "note",
    label: "Blank note",
    blurb: "Anything else you want your AI to remember.",
    type: "other",
    body: (t) => `# ${t}\n\n`,
  },
];

export function templateById(id: string): DocTemplate {
  return DOC_TEMPLATES.find((t) => t.id === id) ?? DOC_TEMPLATES[DOC_TEMPLATES.length - 1]!;
}

/** Path, front-matter and body for a document made from a template. */
export function templatedCard(
  template: DocTemplate,
  title: string,
  existing: Iterable<string>,
  extraTags: string[] = [],
): { path: string; meta: CardMeta; body: string } {
  return {
    path: newCardPath(template.type, title, existing),
    meta: {
      type: template.type,
      title,
      status: template.status ?? (template.type === "task" ? "todo" : "draft"),
      links: [],
      implemented_in: [],
      tags: [...(template.tags ?? []), ...extraTags],
      extra: {},
    },
    body: template.body(title),
  };
}
