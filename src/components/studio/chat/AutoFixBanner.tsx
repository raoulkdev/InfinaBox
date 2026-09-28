import { AnimatePresence, motion } from "motion/react";
import { LifeBuoy, Loader2, X } from "lucide-react";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { fadeRise, fadeTransition } from "@/lib/motion";
import type { AutoFixStatePayload } from "@/lib/studio-types";

// The automatic error-fix loop's state for the open project, straight from
// the backend's `autofix-state` event: fixing (with the real attempt count)
// or given up. Nothing shows when it's idle.
export function AutoFixBanner({ state, onDismiss }: { state: AutoFixStatePayload | null; onDismiss: () => void }) {
  const shown = state && state.state !== "idle" ? state : null;
  return (
    <AnimatePresence initial={false}>
      {shown && (
        <motion.div key={shown.state} {...fadeRise} transition={fadeTransition} data-testid="autofix-banner">
          {shown.state === "fixing" ? (
            <Alert>
              <Loader2 className="animate-spin" />
              <AlertTitle>
                Something broke — fixing it (attempt {shown.attempt} of {shown.maxAttempts})
              </AlertTitle>
              <AlertDescription>Your game showed an error, so the AI is working on it. You can watch below.</AlertDescription>
            </Alert>
          ) : (
            <Alert>
              <LifeBuoy />
              <AlertTitle className="pr-6">I couldn't fix this automatically</AlertTitle>
              <AlertDescription>
                Describe what you see, or undo the last change in History.
              </AlertDescription>
              <Button
                type="button"
                size="icon-xs"
                variant="ghost"
                aria-label="Dismiss"
                onClick={onDismiss}
                className="absolute top-2 right-2 text-muted-foreground"
              >
                <X />
              </Button>
            </Alert>
          )}
        </motion.div>
      )}
    </AnimatePresence>
  );
}
