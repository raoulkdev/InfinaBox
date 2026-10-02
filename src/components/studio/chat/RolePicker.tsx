import { useState } from "react";
import { Popover, RadioGroup } from "radix-ui";
import { ChevronDown } from "lucide-react";
import type { Role } from "@/lib/studio-types";
import { cn } from "@/lib/utils";

// "Ask as: Auto" in the chat composer's footer: which member of the studio
// team the next message goes to. Auto is the Director, who decides for
// themselves; the others are specialists (see the spec's §7.1 "Roles"). The
// lead wires it into ChatComposer. Built on the Radix primitives the shadcn
// components use, styled like StudioSettingsPopover.

/** Short names shown in the UI. `director` is "Auto" to a person. */
export const ROLE_LABELS: Record<Role, string> = {
  director: "Auto",
  designer: "Designer",
  programmer: "Programmer",
  artist: "Artist",
  sound: "Sound",
  qa: "QA",
  producer: "Producer",
  marketer: "Marketer",
};

/** One plain line per role, for people with no game-dev background. */
export const ROLE_DESCRIPTIONS: Record<Role, string> = {
  director: "Picks the right helper for you",
  designer: "How the game plays: mechanics, levels, balance",
  programmer: "Makes it work: code, fixes, technical changes",
  artist: "How it looks: characters, scenery, animation",
  sound: "How it sounds: music, effects, voices",
  qa: "Finds what's broken, checks it works",
  producer: "Keeps you on track: what to do next",
  marketer: "Tells the world: store page, trailer, posts",
};

const ROLE_ORDER: Role[] = [
  "director",
  "designer",
  "programmer",
  "artist",
  "sound",
  "qa",
  "producer",
  "marketer",
];

export interface RolePickerProps {
  value: Role;
  onChange: (role: Role) => void;
  disabled?: boolean;
}

export function RolePicker({ value, onChange, disabled }: RolePickerProps) {
  const [open, setOpen] = useState(false);
  const chosen = value !== "director";

  return (
    <Popover.Root open={open && !disabled} onOpenChange={setOpen}>
      <Popover.Trigger asChild>
        <button
          type="button"
          disabled={disabled}
          data-testid="role-picker"
          aria-label={`Ask as: ${ROLE_LABELS[value]}. ${ROLE_DESCRIPTIONS[value]}`}
          title={ROLE_DESCRIPTIONS[value]}
          className={cn(
            "inline-flex h-6 items-center gap-1 rounded-full border px-2 text-xs outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring/50 disabled:pointer-events-none disabled:opacity-50",
            chosen
              ? "border-foreground/25 bg-muted text-foreground"
              : "border-transparent text-muted-foreground hover:bg-muted/60 hover:text-foreground",
          )}
        >
          <span className="text-muted-foreground">Ask as:</span>
          <span className={cn(chosen && "font-medium")}>{ROLE_LABELS[value]}</span>
          <ChevronDown className="size-3 text-muted-foreground" aria-hidden />
        </button>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          align="start"
          side="top"
          sideOffset={6}
          collisionPadding={12}
          data-testid="role-picker-menu"
          className="z-50 max-h-(--radix-popover-content-available-height) w-56 overflow-y-auto origin-(--radix-popover-content-transform-origin) rounded-xl border border-border bg-popover p-1.5 text-popover-foreground shadow-lg outline-none data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:zoom-out-95 data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:zoom-in-95"
        >
          <RadioGroup.Root
            value={value}
            onValueChange={(next) => {
              onChange(next as Role);
              setOpen(false);
            }}
            aria-label="Ask as"
            className="flex flex-col gap-0.5"
          >
            {ROLE_ORDER.map((role) => (
              <RadioGroup.Item
                key={role}
                value={role}
                data-testid={`role-${role}`}
                title={ROLE_DESCRIPTIONS[role]}
                className="group flex items-start gap-2.5 rounded-lg px-2 py-1.5 text-left outline-none hover:bg-muted/60 focus-visible:bg-muted/60 focus-visible:ring-2 focus-visible:ring-ring/50 data-[state=checked]:bg-muted"
              >
                <span className="mt-0.5 flex size-3.5 shrink-0 items-center justify-center rounded-full border border-foreground/30 group-data-[state=checked]:border-foreground">
                  <RadioGroup.Indicator className="size-1.5 rounded-full bg-foreground" />
                </span>
                <span className="flex min-w-0 flex-col">
                  <span className="text-sm">{ROLE_LABELS[role]}</span>
                </span>
              </RadioGroup.Item>
            ))}
          </RadioGroup.Root>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
