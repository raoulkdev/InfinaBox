import type { ReactNode } from "react";
import { Check, X } from "lucide-react";
import { cn } from "cn";

interface ChipProps {
  selected: boolean;
  onToggle: () => void;
  children: ReactNode;
  /** Shows an × instead of a tick while selected — for the person's own
   * added words, where clicking removes them. */
  removable?: boolean;
}

/** One toggle-button answer. `data-chip` lets the flow's Enter handling
 * treat Enter on a chip as "Next" (Space still toggles it, like any
 * button). */
export function Chip({ selected, onToggle, children, removable }: ChipProps) {
  return (
    <button
      type="button"
      data-chip
      aria-pressed={selected}
      onClick={onToggle}
      className={cn(
        "inline-flex h-9 items-center gap-1.5 rounded-full border px-3.5 text-sm transition-colors outline-none",
        "focus-visible:ring-3 focus-visible:ring-ring/50",
        selected
          ? "border-foreground/60 bg-foreground/10 text-foreground"
          : "border-border bg-background text-foreground/80 hover:border-muted-foreground hover:text-foreground",
      )}
    >
      {selected && (removable ? <X className="size-3.5" /> : <Check className="size-3.5" />)}
      {children}
    </button>
  );
}

/** A larger, card-shaped single choice (the kinds of game, the session
 * lengths): a title with supporting lines underneath. */
export function ChoiceCard({
  selected,
  onSelect,
  title,
  children,
  icon,
  testId,
  disabled = false,
}: {
  selected: boolean;
  onSelect: () => void;
  title: string;
  children?: ReactNode;
  icon?: ReactNode;
  testId?: string;
  /** Shown, but can't be picked. */
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      disabled={disabled}
      data-chip
      data-testid={testId}
      aria-pressed={selected}
      onClick={onSelect}
      className={cn(
        "relative flex flex-col gap-1.5 rounded-xl border p-3.5 text-left transition-colors outline-none",
        "focus-visible:ring-3 focus-visible:ring-ring/50",
        disabled && "cursor-not-allowed opacity-55",
        selected
          ? "border-foreground/60 bg-foreground/[0.07]"
          : cn("border-border bg-background", !disabled && "hover:border-muted-foreground"),
      )}
    >
      <span className="flex items-center gap-2 pr-6 text-sm font-medium text-foreground">
        {icon}
        {title}
      </span>
      {children}
      {selected && (
        <span className="absolute top-3 right-3 flex size-5 items-center justify-center rounded-full bg-foreground text-background">
          <Check className="size-3" />
        </span>
      )}
    </button>
  );
}
