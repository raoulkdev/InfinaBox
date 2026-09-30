import type { ReactNode } from "react";
import { Box, FolderOpen, Loader2, Square } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { InterviewAnswers, TemplateInfo } from "@/lib/studio-types";
import { DIMENSION_2D, DIMENSION_3D, joinPath } from "./answers";
import { ChoiceCard } from "./Chip";

// The four interview questions, one component per screen. Each only
// reads the answers and reports changes; which screen shows, validity and
// Back/Next live in `OnboardingFlow`.

export interface ScreenProps {
  answers: InterviewAnswers;
  update: (patch: Partial<InterviewAnswers>) => void;
}

/** The big friendly question and its one line of help, shared by every
 * screen (and the review) so they all read the same way. */
export function QuestionHeading({ title, helper }: { title: string; helper?: ReactNode }) {
  return (
    <div className="flex flex-col gap-2">
      <h1 className="text-2xl font-semibold tracking-tight text-foreground sm:text-[1.75rem]">{title}</h1>
      {helper && <p className="text-sm text-muted-foreground">{helper}</p>}
    </div>
  );
}

const textareaClass =
  "min-h-28 w-full resize-none rounded-xl border border-input bg-transparent px-3.5 py-3 text-base leading-relaxed outline-none transition-colors placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 dark:bg-input/30";

// --- 1. The idea ---

export function IdeaScreen({ answers, update }: ScreenProps) {
  return (
    <div className="flex flex-col gap-6">
      <QuestionHeading
        title="What's your game about?"
      />
      <textarea
        autoFocus
        data-testid="onboarding-idea"
        className={textareaClass}
        placeholder="A game where…"
        value={answers.idea}
        onChange={(e) => update({ idea: e.target.value })}
        maxLength={600}
      />
    </div>
  );
}

// --- 2. Kind of game: 2D or 3D ---

export type TemplatesState =
  | { status: "loading" }
  | { status: "ready"; templates: TemplateInfo[] }
  | { status: "error"; error: string };

/** Which starting template fits is worked out from the idea, within the
 * dimension picked here (the backend restricts its choice to that
 * dimension), so this question is only 2D or 3D. */
export function GenreScreen({ answers, update }: ScreenProps) {
  return (
    <div className="flex flex-col gap-5">
      <QuestionHeading title="Is your game 2D or 3D?" />
      <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-2">
        <ChoiceCard
          testId="onboarding-dimension-2d"
          selected={answers.genre === DIMENSION_2D}
          onSelect={() => update({ genre: DIMENSION_2D, genre_other: null })}
          title="2D"
          icon={<Square className="size-4 text-muted-foreground" />}
        >
          <span className="text-sm text-muted-foreground">
            Flat, like a drawing: platformers, top-down games, puzzles, story games.
          </span>
        </ChoiceCard>
        <ChoiceCard
          testId="onboarding-dimension-3d"
          selected={answers.genre === DIMENSION_3D}
          onSelect={() => update({ genre: DIMENSION_3D, genre_other: null })}
          title="3D"
          icon={<Box className="size-4 text-muted-foreground" />}
        >
          <span className="text-sm text-muted-foreground">
            Worlds you can walk around in: exploring, first-person.
          </span>
        </ChoiceCard>
      </div>
    </div>
  );
}

// --- 3. References ---

export function ReferencesScreen({ answers, update }: ScreenProps) {
  return (
    <div className="flex flex-col gap-6">
      <QuestionHeading
        title="Any games it's like?"
      />
      <Input
        autoFocus
        data-testid="onboarding-references"
        className="h-11 rounded-xl px-3.5 md:text-base"
        placeholder="e.g. Stardew Valley, Celeste"
        value={answers.references}
        maxLength={200}
        onChange={(e) => update({ references: e.target.value })}
      />
    </div>
  );
}

// --- 4. Name and folder ---

export type ParentDirState = { status: "loading" } | { status: "ready"; path: string | null };

export function NameScreen({
  answers,
  update,
  nameProblem,
  parentDir,
  onChooseFolder,
  onNameEdited,
}: ScreenProps & {
  nameProblem: string | null;
  parentDir: ParentDirState;
  onChooseFolder: () => void;
  onNameEdited: () => void;
}) {
  const parent = parentDir.status === "ready" ? parentDir.path : null;
  return (
    <div className="flex flex-col gap-6">
      <QuestionHeading
        title="What's it called, and where should it live?"
      />
      <div className="flex flex-col gap-1.5">
        <label htmlFor="onboarding-name" className="text-xs font-medium tracking-wide text-muted-foreground">
          Name
        </label>
        <Input
          id="onboarding-name"
          autoFocus
          data-testid="onboarding-name"
          aria-invalid={nameProblem ? true : undefined}
          className="h-11 rounded-xl px-3.5 md:text-base"
          value={answers.name}
          maxLength={80}
          onChange={(e) => {
            onNameEdited();
            update({ name: e.target.value });
          }}
        />
        {nameProblem && (
          <p data-testid="onboarding-name-problem" className="text-xs text-destructive">
            {nameProblem}
          </p>
        )}
      </div>
      <div className="flex flex-col gap-1.5">
        <span className="text-xs font-medium tracking-wide text-muted-foreground">Folder</span>
        <div className="flex items-center gap-2">
          <span
            data-testid="onboarding-parent-dir"
            title={parent ?? undefined}
            className="flex h-11 min-w-0 flex-1 items-center rounded-xl border border-border bg-background px-3.5 text-sm wrap-anywhere text-foreground/90"
          >
            {parentDir.status === "loading" ? (
              <Loader2 className="size-4 animate-spin text-muted-foreground" />
            ) : (
              <span className="line-clamp-2">{parent ?? "No folder chosen yet"}</span>
            )}
          </span>
          <Button
            type="button"
            variant="secondary"
            className="h-11 shrink-0 rounded-xl px-3.5"
            data-enter="native"
            data-testid="onboarding-choose-folder"
            onClick={onChooseFolder}
          >
            <FolderOpen />
            Choose…
          </Button>
        </div>
        {parent && answers.name && !nameProblem && (
          <p className="text-xs wrap-anywhere text-muted-foreground/80">
            Your game will be saved in <span className="font-mono">{joinPath(parent, answers.name)}</span>
          </p>
        )}
      </div>
    </div>
  );
}
