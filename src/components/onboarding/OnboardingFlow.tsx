import type { CreatedProject } from "@/lib/studio-types";

// Wave 0 stub (Phase B plan, Task FO): the first-run interview — questions,
// a review of the template and starter Context, then "Create my game".

export interface OnboardingFlowProps {
  onCreated: (created: CreatedProject) => void;
  onCancel: () => void;
}

export function OnboardingFlow({ onCancel }: OnboardingFlowProps) {
  return (
    <div className="flex flex-col items-center gap-2 p-6 text-sm text-muted-foreground">
      The interview isn't built yet.
      <button type="button" className="underline" onClick={onCancel}>
        Back
      </button>
    </div>
  );
}
