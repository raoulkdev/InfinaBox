import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { AnimatePresence, motion } from "motion/react";
import { ArrowLeft, ArrowRight, Sparkles } from "lucide-react";
import { documentDir, homeDir } from "@tauri-apps/api/path";
import { cn } from "cn";
import { Button } from "@/components/ui/button";
import { fadeTransition, springTransition, widthTransition } from "@/lib/motion";
import { pickProjectFolder } from "@/lib/project-picker";
import type { CreatedProject, InterviewAnswers } from "@/lib/studio-types";
import { EMPTY_ANSWERS, projectNameProblem, suggestProjectName } from "./answers";
import {
  GenreScreen,
  IdeaScreen,
  NameScreen,
  ReferencesScreen,
  TechnicalScreen,
  type ParentDirState,
} from "./QuestionScreens";
import { ReviewScreen } from "./ReviewScreen";

// The new-game interview (spec §6.2): five short questions, one per screen, then a review of what `onboarding_preview`
// says will be created, then "Create my game". The questions are a fixed
// flow in the UI rather than an AI conversation — fast, free, and it works
// before any AI turn has succeeded. All answers live here, so Back (or a
// progress dot) never loses anything.

export interface OnboardingFlowProps {
  onCreated: (created: CreatedProject) => void;
  onCancel: () => void;
}

const QUESTION_COUNT = 5;
const IDEA_STEP = 0;
const REFERENCES_STEP = 2;
const NAME_STEP = 4;
const REVIEW_STEP = QUESTION_COUNT;

/** The folder new games go in until the person picks another: their
 * Documents folder (home as a fallback), resolved by Tauri for this OS.
 * Null when neither resolves — then they choose one themselves. */
async function defaultParentDir(): Promise<string | null> {
  for (const resolve of [documentDir, homeDir]) {
    try {
      const dir = await resolve();
      if (dir) return dir;
    } catch {
      // Try the next one.
    }
  }
  return null;
}

/** What's sent to the backend: trimmed. The name isn't trimmed —
 * leading/trailing spaces are a validation error, not silently fixed. */
function normalize(a: InterviewAnswers): InterviewAnswers {
  return { ...a, idea: a.idea.trim(), references: a.references.trim() };
}

export function OnboardingFlow({ onCreated, onCancel }: OnboardingFlowProps) {
  const [answers, setAnswers] = useState<InterviewAnswers>(EMPTY_ANSWERS);
  const [step, setStep] = useState(0);
  // +1 forward, -1 back: which way the screens slide.
  const [direction, setDirection] = useState(1);
  // Once the person types a name, stop replacing it with a suggestion
  // from their (possibly edited) idea.
  const [nameEdited, setNameEdited] = useState(false);
  const [parentDir, setParentDir] = useState<ParentDirState>({ status: "loading" });
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let cancelled = false;
    void defaultParentDir().then((path) => {
      // Never overwrite a folder the person already chose.
      if (!cancelled) setParentDir((prev) => (prev.status === "loading" ? { status: "ready", path } : prev));
    });
    return () => {
      cancelled = true;
    };
  }, []);

  const update = useCallback((patch: Partial<InterviewAnswers>) => {
    setAnswers((prev) => ({ ...prev, ...patch }));
  }, []);

  async function chooseFolder() {
    const folder = await pickProjectFolder();
    if (folder) setParentDir({ status: "ready", path: folder });
  }

  const nameProblem = projectNameProblem(answers.name);
  const parent = parentDir.status === "ready" ? parentDir.path : null;

  const canContinue = [
    true, // The idea is optional: some games start from a feeling, not a sentence.
    answers.genre !== "",
    true, // "Any games it's like?" is optional.
    true, // A technical level is always chosen (balanced to begin with).
    !nameProblem && parent !== null,
  ][step] ?? false;

  const goTo = useCallback(
    (next: number) => {
      setDirection(next > step ? 1 : -1);
      if (next === NAME_STEP && !nameEdited) {
        setAnswers((prev) => ({ ...prev, name: suggestProjectName(prev.idea) }));
      }
      setStep(next);
    },
    [step, nameEdited],
  );

  const goNext = useCallback(() => {
    if (step < REVIEW_STEP && canContinue) goTo(step + 1);
  }, [step, canContinue, goTo]);

  // Enter moves on from any question: from a text field, or with a chip
  // focused (Space still toggles chips). Shift+Enter keeps a newline in the
  // idea box; controls marked `data-enter="native"` (Back, Retry, Choose…,
  // the "Add" field) keep Enter for themselves, as does every other button.
  const goNextRef = useRef(goNext);
  goNextRef.current = goNext;
  const rootRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (e.key !== "Enter" || e.shiftKey || e.isComposing || e.defaultPrevented) return;
      const target = e.target as HTMLElement | null;
      const root = rootRef.current;
      if (!root || !target) return;
      // Only when focus is inside the flow, or nowhere in particular.
      if (target !== document.body && !root.contains(target)) return;
      if (target.closest('[data-enter="native"]')) return;
      if (target.tagName === "BUTTON" && !target.hasAttribute("data-chip")) return;
      if (target.tagName === "SELECT") return;
      e.preventDefault();
      goNextRef.current();
    }
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, []);

  // Stable for the review's preview fetch: only a real answer change re-runs it.
  const normalized = useMemo(() => normalize(answers), [answers]);

  const screens: ReactNode[] = [
    <IdeaScreen answers={answers} update={update} />,
    <GenreScreen answers={answers} update={update} />,
    <ReferencesScreen answers={answers} update={update} />,
    <TechnicalScreen answers={answers} update={update} />,
    <NameScreen
      answers={answers}
      update={update}
      nameProblem={answers.name === "" && !nameEdited ? null : nameProblem}
      parentDir={parentDir}
      onChooseFolder={() => void chooseFolder()}
      onNameEdited={() => setNameEdited(true)}
    />,
  ];

  const isReview = step === REVIEW_STEP;
  const nextLabel =
    (step === REFERENCES_STEP && answers.references.trim() === "") || (step === IDEA_STEP && answers.idea.trim() === "")
      ? "Skip"
      : step === NAME_STEP
        ? "Review the plan"
        : "Next";

  return (
    <div ref={rootRef} data-testid="onboarding-flow" className="flex h-full min-h-0 w-full flex-col">
      <header className="flex shrink-0 items-center justify-between gap-4 px-6 pt-5">
        <span className="flex items-center gap-1.5 text-xs font-medium tracking-wide text-muted-foreground">
          <Sparkles className="size-3.5" />
          New game
        </span>
        <ProgressDots
          count={QUESTION_COUNT + 1}
          current={step}
          disabled={busy}
          onJump={(i) => goTo(i)}
        />
        <Button
          type="button"
          size="sm"
          variant="ghost"
          data-enter="native"
          data-testid="onboarding-cancel"
          disabled={busy}
          onClick={onCancel}
          className="text-muted-foreground"
        >
          Not now
        </Button>
      </header>

      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex min-h-full w-full max-w-2xl flex-col px-6 pt-[max(1.5rem,7vh)] pb-8">
          <div className="overflow-hidden rounded-2xl border border-border bg-card p-6 shadow-sm sm:p-8">
            <AnimatePresence mode="wait" initial={false} custom={direction}>
              <motion.div
                key={step}
                custom={direction}
                variants={slide}
                initial="enter"
                animate="center"
                exit="exit"
                transition={{ x: springTransition, opacity: fadeTransition }}
                data-testid={`onboarding-step-${step}`}
              >
                {isReview && parent ? (
                  <ReviewScreen
                    answers={normalized}
                    parentDir={parent}
                    onBack={() => goTo(NAME_STEP)}
                    onCreated={onCreated}
                    onBusyChange={setBusy}
                  />
                ) : (
                  screens[step]
                )}
              </motion.div>
            </AnimatePresence>

            {!isReview && (
              <div className="mt-8 flex items-center justify-between gap-3">
                <Button
                  type="button"
                  variant="ghost"
                  data-enter="native"
                  data-testid="onboarding-back"
                  onClick={() => goTo(step - 1)}
                  className={cn(step === 0 && "invisible")}
                >
                  <ArrowLeft />
                  Back
                </Button>
                <div className="flex items-center gap-3">
                  <span className="hidden text-xs text-muted-foreground/70 sm:inline">
                    {canContinue ? "Press Enter ↵" : ""}
                  </span>
                  <Button
                    type="button"
                    size="lg"
                    className="px-4"
                    data-enter="native"
                    data-testid="onboarding-next"
                    disabled={!canContinue}
                    onClick={goNext}
                  >
                    {nextLabel}
                    <ArrowRight />
                  </Button>
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

const slide = {
  enter: (dir: number) => ({ opacity: 0, x: dir * 28 }),
  center: { opacity: 1, x: 0 },
  exit: (dir: number) => ({ opacity: 0, x: dir * -28 }),
};

/** One dot per screen (the four questions and the review); the current
 * one stretches into a pill. Earlier dots jump back to that question. */
function ProgressDots({
  count,
  current,
  disabled,
  onJump,
}: {
  count: number;
  current: number;
  disabled: boolean;
  onJump: (index: number) => void;
}) {
  return (
    <div className="flex items-center gap-1.5" aria-label={`Step ${current + 1} of ${count}`} role="group">
      {Array.from({ length: count }, (_, i) => {
        const done = i < current;
        return (
          <button
            key={i}
            type="button"
            data-enter="native"
            tabIndex={done ? 0 : -1}
            disabled={!done || disabled}
            aria-current={i === current ? "step" : undefined}
            aria-label={i === count - 1 ? "Review" : `Question ${i + 1}`}
            onClick={() => onJump(i)}
            className="flex h-4 items-center outline-none focus-visible:ring-3 focus-visible:ring-ring/50 enabled:cursor-pointer"
          >
            <motion.span
              className={cn(
                "block h-1.5 rounded-full",
                i === current ? "bg-foreground" : done ? "bg-foreground/45" : "bg-foreground/15",
              )}
              initial={false}
              animate={{ width: i === current ? 20 : 6 }}
              transition={widthTransition}
            />
          </button>
        );
      })}
    </div>
  );
}
