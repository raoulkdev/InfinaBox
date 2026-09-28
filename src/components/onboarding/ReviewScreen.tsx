import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { AnimatePresence, motion } from "motion/react";
import { AlertCircle, ArrowLeft, FileText, FolderPlus, Gamepad2, Hammer, Loader2, Sparkles } from "lucide-react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { fadeTransition } from "@/lib/motion";
import { onboardingCreate, onboardingPreview } from "@/lib/studio-api";
import type { CreatedProject, InterviewAnswers, OnboardingPreview } from "@/lib/studio-types";
import { friendlyCardName, joinPath } from "./answers";
import { QuestionHeading, type TemplatesState } from "./QuestionScreens";

type PreviewState =
  | { status: "loading" }
  | { status: "ready"; preview: OnboardingPreview }
  | { status: "error"; error: string };

type CreateState = { status: "idle" } | { status: "creating" } | { status: "error"; error: string };

interface ReviewScreenProps {
  answers: InterviewAnswers;
  parentDir: string;
  templates: TemplatesState;
  onBack: () => void;
  onCreated: (created: CreatedProject) => void;
  /** Tells the flow a create is running, so it can lock Back / "Not now". */
  onBusyChange: (busy: boolean) => void;
}

function errorText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

/** "Here's the plan": everything `onboarding_preview` says will happen, and
 * the button that makes it happen. Nothing here is filled in by the
 * frontend — if the preview fails, the real error shows with a retry. */
export function ReviewScreen({ answers, parentDir, templates, onBack, onCreated, onBusyChange }: ReviewScreenProps) {
  const [preview, setPreview] = useState<PreviewState>({ status: "loading" });
  const [create, setCreate] = useState<CreateState>({ status: "idle" });
  // The starting point the person picked instead of the suggested one.
  // Changing it doesn't re-run the preview: the preview's `choice` is
  // worked out from the answers alone, so a different starting point only
  // changes which `template_id` goes to `onboardingCreate` (which writes
  // that template's own cards alongside the ones listed here).
  const [templateOverride, setTemplateOverride] = useState<string | null>(null);

  const loadPreview = useCallback(() => {
    let cancelled = false;
    setPreview({ status: "loading" });
    onboardingPreview(answers)
      .then((p) => !cancelled && setPreview({ status: "ready", preview: p }))
      .catch((err) => !cancelled && setPreview({ status: "error", error: errorText(err) }));
    return () => {
      cancelled = true;
    };
  }, [answers]);

  useEffect(() => loadPreview(), [loadPreview]);

  const creating = create.status === "creating";
  useEffect(() => onBusyChange(creating), [creating, onBusyChange]);

  // The plan can be taller than the window; keep the progress note and any
  // create error (both next to the button) in view once Create is pressed.
  const footerRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (create.status !== "idle") footerRef.current?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }, [create.status]);

  const ready = preview.status === "ready" ? preview.preview : null;
  const templateId = templateOverride ?? ready?.choice.template_id ?? null;
  const templateList = templates.status === "ready" ? templates.templates : [];
  const templateName = (id: string) => templateList.find((t) => t.id === id)?.name ?? id;
  const projectPath = joinPath(parentDir, answers.name);

  async function handleCreate() {
    if (!templateId || creating) return;
    setCreate({ status: "creating" });
    try {
      const created = await onboardingCreate(parentDir, answers, templateId);
      onCreated(created);
    } catch (err) {
      setCreate({ status: "error", error: errorText(err) });
    }
  }

  return (
    <div className="flex flex-col gap-6">
      <QuestionHeading
        title="Here's the plan"
        helper="Check it over. Nothing is created until you press Create my game."
      />

      <AnimatePresence mode="wait" initial={false}>
        {preview.status === "loading" && (
          <motion.div
            key="loading"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={fadeTransition}
            className="flex flex-col gap-3"
          >
            <Skeleton className="h-20 rounded-xl" />
            <Skeleton className="h-28 rounded-xl" />
            <Skeleton className="h-24 rounded-xl" />
          </motion.div>
        )}

        {preview.status === "error" && (
          <motion.div
            key="error"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={fadeTransition}
          >
            <Alert variant="destructive" data-testid="onboarding-preview-error">
              <AlertCircle />
              <AlertDescription className="flex flex-col items-start gap-2">
                <span className="wrap-anywhere">Couldn't put the plan together: {preview.error}</span>
                <Button type="button" size="sm" variant="outline" data-enter="native" onClick={loadPreview}>
                  Retry
                </Button>
              </AlertDescription>
            </Alert>
          </motion.div>
        )}

        {ready && templateId && (
          <motion.div
            key="ready"
            initial={{ opacity: 0, y: 6 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0 }}
            transition={fadeTransition}
            className="flex flex-col gap-3"
            data-testid="onboarding-plan"
          >
            <PlanBlock icon={<Gamepad2 />} label="Starting point">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <span className="text-base font-medium text-foreground">{templateName(templateId)}</span>
                {templateList.length > 1 && (
                  <label className="flex items-center gap-2 text-xs text-muted-foreground">
                    Use a different starting point
                    <select
                      data-testid="onboarding-template-override"
                      data-enter="native"
                      disabled={creating}
                      value={templateId}
                      onChange={(e) =>
                        setTemplateOverride(e.target.value === ready.choice.template_id ? null : e.target.value)
                      }
                      className="h-7 rounded-lg border border-input bg-background px-2 text-xs text-foreground outline-none focus-visible:ring-3 focus-visible:ring-ring/50 dark:bg-input/30"
                    >
                      {templateList.map((t) => (
                        <option key={t.id} value={t.id}>
                          {t.name}
                        </option>
                      ))}
                    </select>
                  </label>
                )}
              </div>
              <p className="text-sm text-muted-foreground">
                {templateOverride
                  ? "You picked this one yourself."
                  : ready.choice.reason}
              </p>
            </PlanBlock>

            <PlanBlock icon={<FileText />} label="Your Context — notes about your game I'll write">
              {ready.cards.length === 0 ? (
                <p className="text-sm text-muted-foreground">No cards.</p>
              ) : (
                <ul className="flex flex-wrap gap-1.5">
                  {ready.cards.map((path) => {
                    const card = friendlyCardName(path);
                    return (
                      <li
                        key={path}
                        title={path}
                        className="inline-flex items-center gap-1.5 rounded-lg border border-border bg-background px-2.5 py-1 text-sm text-foreground/90"
                      >
                        {card.group && <span className="text-xs text-muted-foreground">{card.group} ·</span>}
                        {card.title}
                      </li>
                    );
                  })}
                </ul>
              )}
            </PlanBlock>

            <PlanBlock icon={<Hammer />} label="What I'll build first">
              <ol className="flex flex-col gap-1.5">
                {ready.first_build_steps.map((step, i) => (
                  <li key={i} className="flex gap-2.5 text-sm text-foreground/90">
                    <span className="flex size-5 shrink-0 items-center justify-center rounded-full bg-foreground/10 text-xs text-muted-foreground tabular-nums">
                      {i + 1}
                    </span>
                    <span className="pt-px">{step}</span>
                  </li>
                ))}
              </ol>
            </PlanBlock>

            <PlanBlock icon={<FolderPlus />} label="Where it goes">
              <p data-testid="onboarding-project-path" className="font-mono text-sm wrap-anywhere text-foreground/90">
                {projectPath}
              </p>
            </PlanBlock>
          </motion.div>
        )}
      </AnimatePresence>

      <AnimatePresence initial={false}>
        {create.status === "error" && (
          <motion.div
            key="create-error"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={fadeTransition}
          >
            <Alert variant="destructive" data-testid="onboarding-create-error">
              <AlertCircle />
              <AlertDescription className="wrap-anywhere">
                Couldn't create your game: {create.error}
              </AlertDescription>
            </Alert>
          </motion.div>
        )}
      </AnimatePresence>

      <div ref={footerRef} className="flex flex-wrap items-center justify-between gap-3 pt-1">
        <Button type="button" variant="ghost" data-enter="native" disabled={creating} onClick={onBack}>
          <ArrowLeft />
          Change answers
        </Button>
        <div className="flex items-center gap-3">
          <AnimatePresence initial={false}>
            {creating && (
              <motion.span
                key="creating"
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                exit={{ opacity: 0 }}
                transition={fadeTransition}
                className="text-xs text-muted-foreground"
              >
                Setting up your game — this takes a few seconds.
              </motion.span>
            )}
          </AnimatePresence>
          <Button
            type="button"
            size="lg"
            className="px-4"
            data-testid="onboarding-create"
            disabled={!ready || !templateId || creating}
            onClick={() => void handleCreate()}
          >
            {creating ? <Loader2 className="animate-spin" /> : <Sparkles />}
            {creating ? "Creating…" : create.status === "error" ? "Try again" : "Create my game"}
          </Button>
        </div>
      </div>
    </div>
  );
}

function PlanBlock({ icon, label, children }: { icon: ReactNode; label: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-2 rounded-xl border border-border bg-background/60 p-3.5">
      <h2 className="flex items-center gap-1.5 text-xs font-medium tracking-wide text-muted-foreground [&_svg]:size-3.5">
        {icon}
        {label}
      </h2>
      {children}
    </section>
  );
}
