import { useState, type ReactNode } from "react";
import { AlertCircle, FolderOpen, Gamepad2, Keyboard, Lightbulb, Loader2, Plus, Sparkles } from "lucide-react";
import { cn } from "cn";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import type { InterviewAnswers, TemplateInfo } from "@/lib/studio-types";
import {
  BLANK_TEMPLATE_ID,
  FEEL_OPTIONS,
  IDEA_EXAMPLES,
  LOOK_OPTIONS,
  OTHER_GENRE,
  SESSION_OPTIONS,
  joinPath,
} from "./answers";
import { Chip, ChoiceCard } from "./Chip";

// The seven interview questions, one component per screen. Each only
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
        helper="A sentence or two is plenty — you can change everything later."
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
      <div className="flex flex-col gap-2">
        <span className="flex items-center gap-1.5 text-xs font-medium tracking-wide text-muted-foreground">
          <Lightbulb className="size-3.5" />
          Need a spark? Try one of these
        </span>
        <div className="flex flex-wrap gap-2">
          {IDEA_EXAMPLES.map((example) => (
            <button
              key={example}
              type="button"
              data-enter="native"
              onClick={() => update({ idea: example })}
              className="rounded-lg border border-dashed border-border px-3 py-1.5 text-left text-sm text-foreground/75 transition-colors outline-none hover:border-muted-foreground hover:text-foreground focus-visible:ring-3 focus-visible:ring-ring/50"
            >
              {example}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}

// --- 2. Kind of game ---

export type TemplatesState =
  | { status: "loading" }
  | { status: "ready"; templates: TemplateInfo[] }
  | { status: "error"; error: string };

export function GenreScreen({
  answers,
  update,
  templates,
  onRetryTemplates,
}: ScreenProps & { templates: TemplatesState; onRetryTemplates: () => void }) {
  const isOther = answers.genre === OTHER_GENRE;
  const blank = templates.status === "ready" ? templates.templates.find((t) => t.id === BLANK_TEMPLATE_ID) : undefined;

  return (
    <div className="flex flex-col gap-5">
      <QuestionHeading
        title="What kind of game is it?"
        helper="Pick the closest one. It's where your game starts — the AI shapes it into your idea from there."
      />

      {templates.status === "error" && (
        <Alert variant="destructive">
          <AlertCircle />
          <AlertDescription className="flex flex-wrap items-center justify-between gap-2">
            <span className="wrap-anywhere">Couldn't load the kinds of game: {templates.error}</span>
            <Button type="button" size="sm" variant="outline" data-enter="native" onClick={onRetryTemplates}>
              Retry
            </Button>
          </AlertDescription>
        </Alert>
      )}

      <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-2">
        {templates.status === "loading" &&
          [0, 1, 2].map((i) => <Skeleton key={i} className="h-[7.5rem] rounded-xl" />)}
        {templates.status === "ready" &&
          templates.templates
            .filter((t) => t.id !== BLANK_TEMPLATE_ID)
            .map((t) => (
              <ChoiceCard
                key={t.id}
                testId={`onboarding-genre-${t.id}`}
                selected={answers.genre === t.id}
                onSelect={() => update({ genre: t.id, genre_other: null })}
                title={`${t.name} · ${t.dimension.toUpperCase()}`}
                icon={<Gamepad2 className="size-4 text-muted-foreground" />}
              >
                <span className="text-sm text-muted-foreground">{t.description}</span>
                <span className="flex items-start gap-1.5 text-xs text-muted-foreground/80">
                  <Keyboard className="mt-px size-3.5 shrink-0" />
                  {t.controls}
                </span>
              </ChoiceCard>
            ))}
        <ChoiceCard
          testId="onboarding-genre-other"
          selected={isOther}
          onSelect={() => update({ genre: OTHER_GENRE, genre_other: answers.genre_other ?? "" })}
          title="Something else"
          icon={<Sparkles className="size-4 text-muted-foreground" />}
        >
          <span className="text-sm text-muted-foreground">
            Describe it and I'll pick the closest starting point from your idea.
          </span>
        </ChoiceCard>
      </div>

      {isOther && (
        <Input
          autoFocus
          data-testid="onboarding-genre-other-text"
          className="h-10 rounded-xl px-3.5"
          placeholder="What kind of game? e.g. a racing game, a puzzle game"
          value={answers.genre_other ?? ""}
          onChange={(e) => update({ genre_other: e.target.value })}
          maxLength={120}
        />
      )}

      <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2 text-xs text-muted-foreground">
        <span>2D games are the easiest place to start; 3D ones take a little more from the AI.</span>
        <button
          type="button"
          data-chip
          data-testid="onboarding-genre-blank"
          aria-pressed={answers.genre === BLANK_TEMPLATE_ID}
          title={blank?.description}
          onClick={() => update({ genre: BLANK_TEMPLATE_ID, genre_other: null })}
          className={cn(
            "rounded-md px-2 py-1 underline-offset-4 transition-colors outline-none hover:text-foreground hover:underline focus-visible:ring-3 focus-visible:ring-ring/50",
            answers.genre === BLANK_TEMPLATE_ID && "bg-foreground/10 text-foreground",
          )}
        >
          {answers.genre === BLANK_TEMPLATE_ID ? "✓ Starting from scratch" : "Or start from scratch"}
        </button>
      </div>
    </div>
  );
}

// --- Own words added to a chip list (feel) or replacing a pick (look) ---

function OwnWordsInput({
  placeholder,
  value,
  onChange,
  onAdd,
  addLabel,
}: {
  placeholder: string;
  value: string;
  onChange: (value: string) => void;
  /** With `onAdd`, Enter (or the button) adds the words as a chip instead
   * of moving to the next question. */
  onAdd?: () => void;
  addLabel?: string;
}) {
  return (
    <div className="flex items-center gap-2">
      <Input
        className="h-10 rounded-xl px-3.5"
        data-enter={onAdd ? "native" : undefined}
        placeholder={placeholder}
        value={value}
        maxLength={60}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => {
          if (onAdd && e.key === "Enter" && !e.nativeEvent.isComposing) {
            e.preventDefault();
            onAdd();
          }
        }}
      />
      {onAdd && (
        <Button
          type="button"
          variant="secondary"
          className="h-10 rounded-xl px-3.5"
          data-enter="native"
          disabled={!value.trim()}
          onClick={onAdd}
        >
          <Plus />
          {addLabel}
        </Button>
      )}
    </div>
  );
}

// --- 3. Feel ---

export function FeelScreen({ answers, update }: ScreenProps) {
  const [draft, setDraft] = useState("");
  const own = answers.feel.filter((f) => !FEEL_OPTIONS.includes(f));

  const toggle = (word: string) =>
    update({
      feel: answers.feel.includes(word) ? answers.feel.filter((f) => f !== word) : [...answers.feel, word],
    });

  const add = () => {
    const word = draft.trim();
    if (!word) return;
    const existing = [...FEEL_OPTIONS, ...answers.feel].find((f) => f.toLowerCase() === word.toLowerCase());
    if (existing) {
      if (!answers.feel.includes(existing)) update({ feel: [...answers.feel, existing] });
    } else {
      update({ feel: [...answers.feel, word] });
    }
    setDraft("");
  };

  return (
    <div className="flex flex-col gap-6">
      <QuestionHeading title="How should it feel?" helper="Pick as many as you like." />
      <div className="flex flex-wrap gap-2">
        {FEEL_OPTIONS.map((word) => (
          <Chip key={word} selected={answers.feel.includes(word)} onToggle={() => toggle(word)}>
            {word}
          </Chip>
        ))}
        {own.map((word) => (
          <Chip key={word} selected removable onToggle={() => toggle(word)}>
            {word}
          </Chip>
        ))}
      </div>
      <OwnWordsInput
        placeholder="Something else? e.g. mysterious, chaotic"
        value={draft}
        onChange={setDraft}
        onAdd={add}
        addLabel="Add"
      />
    </div>
  );
}

// --- 4. Look ---

export function LookScreen({ answers, update }: ScreenProps) {
  const isOwn = answers.look !== "" && !LOOK_OPTIONS.includes(answers.look);
  return (
    <div className="flex flex-col gap-6">
      <QuestionHeading
        title="What should it look like?"
        helper="Pick one, or say it in your own words. It sets the style for your game's art."
      />
      <div className="flex flex-wrap gap-2">
        {LOOK_OPTIONS.map((look) => (
          <Chip
            key={look}
            selected={answers.look === look}
            onToggle={() => update({ look: answers.look === look ? "" : look })}
          >
            {look}
          </Chip>
        ))}
      </div>
      <OwnWordsInput
        placeholder="Or in your own words, e.g. watercolor storybook"
        value={isOwn ? answers.look : ""}
        onChange={(look) => update({ look })}
      />
    </div>
  );
}

// --- 5. References ---

export function ReferencesScreen({ answers, update }: ScreenProps) {
  return (
    <div className="flex flex-col gap-6">
      <QuestionHeading
        title="Any games it's like?"
        helper="Name a game or two it reminds you of. Totally optional — skip it if nothing comes to mind."
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

// --- 6. Session length ---

export function SessionScreen({ answers, update }: ScreenProps) {
  return (
    <div className="flex flex-col gap-6">
      <QuestionHeading title="How long is one play session?" helper="How long someone plays in one go." />
      <div className="grid grid-cols-1 gap-2.5 sm:grid-cols-3">
        {SESSION_OPTIONS.map((option) => (
          <ChoiceCard
            key={option.value}
            selected={answers.session_length === option.value}
            onSelect={() => update({ session_length: option.value })}
            title={option.value}
          >
            <span className="text-sm text-muted-foreground">{option.hint}</span>
          </ChoiceCard>
        ))}
      </div>
    </div>
  );
}

// --- 7. Name and folder ---

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
        helper="Here's a name from your idea — change it to anything you like."
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
