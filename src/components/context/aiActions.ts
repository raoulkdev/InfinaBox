import type { Role } from "@/lib/studio-types";

// What the "Ask the AI" buttons in the Context studio send to Studio's chat.
// Each names the document by path so the AI opens it with its Context tools
// (`read_context_card`, `write_context_card`) instead of guessing.

export interface AiAsk {
  message: string;
  role: Role;
}

export type AskAi = (ask: AiAsk) => void;

export interface DocAction {
  id: string;
  label: string;
  hint: string;
  ask: (path: string) => AiAsk;
}

export const DOC_ACTIONS: DocAction[] = [
  {
    id: "draft",
    label: "Draft it for me",
    hint: "Fill in the empty parts, using what the game already says.",
    ask: (path) => ({
      role: "designer",
      message: `Please write a first draft of the document \`${path}\`. Read the Concept card and any related documents first (list_context_cards, read_context_card), keep anything I've already written, and fill in the empty parts using only what the game and my other documents already say. Where something creative is still open, don't invent it: leave a line starting \"TODO (your call):\" with two or three options. Then save it with write_context_card.`,
    }),
  },
  {
    id: "improve",
    label: "Make it clearer",
    hint: "Tighten the writing without changing what it says.",
    ask: (path) => ({
      role: "designer",
      message: `Please improve the writing of the document \`${path}\`: make it clearer and better organised, without changing what it means or removing anything I decided. Save it with write_context_card and summarise what you changed.`,
    }),
  },
  {
    id: "check",
    label: "Check it against the game",
    hint: "Does the game do what this document says?",
    ask: (path) => ({
      role: "qa",
      message: `Please check whether my game matches the document \`${path}\`. Read the document, then look at the game's scenes and scripts. Tell me what matches, what's missing and what's different. Don't change anything yet.`,
    }),
  },
  {
    id: "tasks",
    label: "Turn it into tasks",
    hint: "Add task cards for what's left to build.",
    ask: (path) => ({
      role: "producer",
      message: `Please read the document \`${path}\` and create task cards (write_context_card, type task, status todo, linking back to the document) for what's still left to build or decide. Keep them small and clear, and list them for me afterwards.`,
    }),
  },
  {
    id: "connect",
    label: "Link related documents",
    hint: "Find other documents this one should link to.",
    ask: (path) => ({
      role: "designer",
      message: `Please read the document \`${path}\` and the other Context documents, and add links from it to the documents it relates to (the links field in its header), and back where it makes sense. Don't rewrite the text.`,
    }),
  },
];

export function questionAbout(path: string, question: string): AiAsk {
  return {
    role: "designer",
    message: `About my document \`${path}\` (open it with read_context_card first): ${question.trim()}`,
  };
}

/** Asks the AI to fill in the gaps in the game's documents. */
export function fillGapsAsk(missing: string[]): AiAsk {
  return {
    role: "designer",
    message: `My game's documents are missing: ${missing.join(", ")}. Read what's there (list_context_cards, read_context_card), then write those documents with write_context_card using only what the Concept and the game already say. Where something creative is still open, leave a line starting \"TODO (your call):\" with two or three options instead of inventing it. Keep each short.`,
  };
}
