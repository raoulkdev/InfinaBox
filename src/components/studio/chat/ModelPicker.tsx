import { useEffect, useState } from "react";
import { Popover, RadioGroup } from "radix-ui";
import { ChevronDown } from "lucide-react";
import { Input } from "@/components/ui/input";
import type { Effort, ProviderId } from "@/lib/studio-types";
import { cn } from "@/lib/utils";

// "Model" and "Effort" for the next message, in the chat composer's footer.
// The choice is remembered per AI on this computer. What's offered is only
// what the AI's own program accepts: Claude Code takes model aliases and
// five effort levels, Codex a model name and four levels; an API or local
// model is set up in Settings, so only a different model name can be typed.

export interface ModelChoice {
  /** null: the AI's own default. */
  model: string | null;
  effort: Effort | null;
}

const KEY = (provider: string) => `infinabox.modelChoice.${provider}`;

export function loadChoice(provider: string): ModelChoice {
  try {
    const raw = localStorage.getItem(KEY(provider));
    if (raw) {
      const parsed = JSON.parse(raw) as Partial<ModelChoice>;
      return { model: parsed.model ?? null, effort: parsed.effort ?? null };
    }
  } catch {
    // Falls back to the defaults.
  }
  return { model: null, effort: null };
}

export function saveChoice(provider: string, choice: ModelChoice) {
  try {
    localStorage.setItem(KEY(provider), JSON.stringify(choice));
  } catch {
    // Not remembering is harmless.
  }
}

const EFFORT_LABEL: Record<Effort, string> = {
  low: "Low",
  medium: "Medium",
  high: "High",
  xhigh: "Extra high",
  max: "Max",
};

interface Offer {
  models: { value: string; label: string }[];
  efforts: Effort[];
}

function offerFor(provider: ProviderId | null): Offer {
  switch (provider) {
    case "claude-code":
      return {
        models: [
          { value: "fable", label: "Fable" },
          { value: "opus", label: "Opus" },
          { value: "sonnet", label: "Sonnet" },
        ],
        efforts: ["low", "medium", "high", "xhigh", "max"],
      };
    case "codex":
      return { models: [], efforts: ["low", "medium", "high", "xhigh"] };
    default:
      return { models: [], efforts: [] };
  }
}

const DEFAULT_VALUE = "__default__";

export interface ModelPickerProps {
  provider: ProviderId | null;
  value: ModelChoice;
  onChange: (next: ModelChoice) => void;
  disabled?: boolean;
}

export function ModelPicker({ provider, value, onChange, disabled }: ModelPickerProps) {
  const [open, setOpen] = useState(false);
  const offer = offerFor(provider);
  const [custom, setCustom] = useState(value.model && !offer.models.some((m) => m.value === value.model) ? value.model : "");

  useEffect(() => {
    setCustom(value.model && !offer.models.some((m) => m.value === value.model) ? value.model : "");
  }, [provider]); // eslint-disable-line react-hooks/exhaustive-deps

  const modelLabel = value.model ? (offer.models.find((m) => m.value === value.model)?.label ?? value.model) : "Default";
  const summary = [modelLabel, value.effort && offer.efforts.length ? EFFORT_LABEL[value.effort] : null]
    .filter(Boolean)
    .join(" · ");
  const changed = value.model !== null || (value.effort !== null && offer.efforts.length > 0);

  return (
    <Popover.Root open={open && !disabled} onOpenChange={setOpen}>
      <Popover.Trigger asChild>
        <button
          type="button"
          disabled={disabled}
          data-testid="model-picker"
          aria-label={`Model and effort: ${summary}`}
          title="Which model to use, and how hard it thinks"
          className={cn(
            "inline-flex h-6 items-center gap-1 rounded-full border px-2 text-xs outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring/50 disabled:pointer-events-none disabled:opacity-50",
            changed
              ? "border-foreground/25 bg-muted text-foreground"
              : "border-transparent text-muted-foreground hover:bg-muted/60 hover:text-foreground",
          )}
        >
          <span className="text-muted-foreground">Model:</span>
          <span className={cn(changed && "font-medium")}>{summary}</span>
          <ChevronDown className="size-3 text-muted-foreground" aria-hidden />
        </button>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          align="start"
          side="top"
          sideOffset={6}
          collisionPadding={12}
          data-testid="model-picker-menu"
          className="z-50 flex w-80 flex-col gap-3 rounded-xl border border-border bg-popover p-3 text-popover-foreground shadow-lg outline-none"
        >
          <section className="flex flex-col gap-1.5">
            <h3 className="text-xs font-medium text-muted-foreground">Model</h3>
            <RadioGroup.Root
              value={value.model ?? DEFAULT_VALUE}
              onValueChange={(next) => {
                setCustom("");
                onChange({ ...value, model: next === DEFAULT_VALUE ? null : next });
              }}
              aria-label="Model"
              className="flex flex-wrap gap-1.5"
            >
              {[{ value: DEFAULT_VALUE, label: "Default" }, ...offer.models].map((m) => (
                <RadioGroup.Item
                  key={m.value}
                  value={m.value}
                  data-testid={`model-${m.value}`}
                  className="h-7 rounded-full border border-border px-3 text-xs outline-none transition-colors hover:border-muted-foreground focus-visible:ring-2 focus-visible:ring-ring/50 data-[state=checked]:border-foreground/60 data-[state=checked]:bg-foreground/10"
                >
                  {m.label}
                </RadioGroup.Item>
              ))}
            </RadioGroup.Root>
            <Input
              value={custom}
              data-testid="model-custom"
              className="h-8 text-xs"
              placeholder="Or type a model name"
              maxLength={100}
              onChange={(e) => {
                setCustom(e.target.value);
                onChange({ ...value, model: e.target.value.trim() || null });
              }}
            />
            <p className="text-xs text-muted-foreground">
              {provider === "claude-code" || provider === "codex"
                ? "Default is whatever your AI uses on its own."
                : "Default is the model you set in Settings → Your AI."}
            </p>
          </section>

          {offer.efforts.length > 0 && (
            <section className="flex flex-col gap-1.5">
              <h3 className="text-xs font-medium text-muted-foreground">Effort</h3>
              <RadioGroup.Root
                value={value.effort ?? DEFAULT_VALUE}
                onValueChange={(next) => onChange({ ...value, effort: next === DEFAULT_VALUE ? null : (next as Effort) })}
                aria-label="Effort"
                className="flex flex-wrap gap-1.5"
              >
                {[{ value: DEFAULT_VALUE, label: "Default" }, ...offer.efforts.map((e) => ({ value: e, label: EFFORT_LABEL[e] }))].map((e) => (
                  <RadioGroup.Item
                    key={e.value}
                    value={e.value}
                    data-testid={`effort-${e.value}`}
                    className="h-7 rounded-full border border-border px-3 text-xs outline-none transition-colors hover:border-muted-foreground focus-visible:ring-2 focus-visible:ring-ring/50 data-[state=checked]:border-foreground/60 data-[state=checked]:bg-foreground/10"
                  >
                    {e.label}
                  </RadioGroup.Item>
                ))}
              </RadioGroup.Root>
              <p className="text-xs text-muted-foreground">Higher effort thinks longer: better on hard changes, slower.</p>
            </section>
          )}
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
