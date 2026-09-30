import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { Popover, RadioGroup, Switch } from "radix-ui";
import { AlertCircle, Loader2, RefreshCw, Settings2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { projectSettingsGet, projectSettingsSet } from "@/lib/studio-api";
import type { PlanPolicy, ProjectSettings } from "@/lib/studio-types";

// The chat header's gear: how this game's AI works with the person — plans
// first or not, "Teach me", and automatic error fixing. These are the
// project's own settings (`.ibproject/settings.json`, committed with the
// game), read fresh each time the popover opens and saved on every change.
// Uses the Radix primitives directly (the `radix-ui` package the shadcn
// components are built on), styled to match them.

type LoadState =
  | { status: "loading" }
  | { status: "ready"; settings: ProjectSettings }
  | { status: "error"; message: string };

const PLAN_OPTIONS: { value: PlanPolicy; label: string; description: string }[] = [
  {
    value: "always_plan",
    label: "Always show me a plan first",
    description: "The AI checks with you before it changes anything.",
  },
  {
    value: "small_changes_direct",
    label: "Small changes without a plan",
    description: "Quick tweaks happen right away. Bigger changes still get a plan.",
  },
];

export function StudioSettingsPopover({ projectPath }: { projectPath: string }) {
  const [open, setOpen] = useState(false);
  const [state, setState] = useState<LoadState>({ status: "loading" });
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  // Drops a load or save that finishes after the project changed or a newer
  // request started.
  const seq = useRef(0);

  const load = useCallback(async () => {
    const mine = ++seq.current;
    setState({ status: "loading" });
    setSaveError(null);
    try {
      const settings = await projectSettingsGet(projectPath);
      if (mine === seq.current) setState({ status: "ready", settings });
    } catch (err) {
      if (mine === seq.current) setState({ status: "error", message: String(err) });
    }
  }, [projectPath]);

  // Fresh from disk every time it opens (the file travels with the project
  // and can change underneath, e.g. after going back in History).
  useEffect(() => {
    if (open) void load();
  }, [open, load]);

  async function save(next: ProjectSettings) {
    if (state.status !== "ready") return;
    const previous = state.settings;
    const mine = ++seq.current;
    // Shown right away; put back if the save fails, so the controls never
    // claim a setting that isn't saved.
    setState({ status: "ready", settings: next });
    setSaving(true);
    setSaveError(null);
    try {
      const saved = await projectSettingsSet(projectPath, next);
      if (mine === seq.current) setState({ status: "ready", settings: saved });
    } catch (err) {
      if (mine === seq.current) {
        setState({ status: "ready", settings: previous });
        setSaveError(String(err));
      }
    } finally {
      if (mine === seq.current) setSaving(false);
    }
  }

  const settings = state.status === "ready" ? state.settings : null;

  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <Popover.Trigger asChild>
        <Button
          type="button"
          size="icon-xs"
          variant="ghost"
          aria-label="Studio settings"
          data-testid="studio-settings-button"
          title="Studio settings"
          className="text-muted-foreground"
        >
          <Settings2 />
        </Button>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          align="end"
          sideOffset={6}
          collisionPadding={12}
          data-testid="studio-settings"
          className="z-50 w-80 origin-(--radix-popover-content-transform-origin) rounded-xl border border-border bg-popover p-3 text-popover-foreground shadow-lg outline-none data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:zoom-out-95 data-[state=open]:animate-in data-[state=open]:fade-in-0 data-[state=open]:zoom-in-95"
        >
          <div className="flex items-start justify-between gap-2">
            <span className="text-sm font-medium">How the AI works with you</span>
            {(state.status === "loading" || saving) && (
              <Loader2 className="mt-0.5 size-3.5 animate-spin text-muted-foreground" aria-label="Saving" />
            )}
          </div>

          {state.status === "error" ? (
            <div className="mt-3 flex flex-col items-start gap-2 rounded-lg border border-destructive/30 bg-destructive/5 p-2.5">
              <span className="flex items-center gap-1.5 text-xs font-medium">
                <AlertCircle className="size-3.5 text-destructive" />
                Couldn't load this game's settings
              </span>
              <span className="font-mono text-[11px] break-words text-muted-foreground">{state.message}</span>
              <Button type="button" size="xs" variant="outline" onClick={() => void load()}>
                <RefreshCw />
                Try again
              </Button>
            </div>
          ) : (
            <div className="mt-3 flex flex-col gap-3">
              <SettingGroup label="Plans">
                <RadioGroup.Root
                  // "" (nothing picked) until loaded, so it stays controlled.
                  value={settings?.plan_policy ?? ""}
                  disabled={!settings}
                  onValueChange={(value) => settings && void save({ ...settings, plan_policy: value as PlanPolicy })}
                  aria-label="Plans"
                  className="flex flex-col gap-1"
                >
                  {PLAN_OPTIONS.map((option) => (
                    <RadioGroup.Item
                      key={option.value}
                      value={option.value}
                      data-testid={`setting-plan-${option.value}`}
                      className="group flex items-start gap-2.5 rounded-lg border border-border px-2.5 py-2 text-left outline-none hover:bg-muted/50 focus-visible:ring-2 focus-visible:ring-ring/50 disabled:opacity-50 data-[state=checked]:border-foreground/30 data-[state=checked]:bg-muted/40"
                    >
                      <span className="mt-0.5 flex size-4 shrink-0 items-center justify-center rounded-full border border-foreground/30 group-data-[state=checked]:border-foreground">
                        <RadioGroup.Indicator className="size-2 rounded-full bg-foreground" />
                      </span>
                      <span className="flex min-w-0 flex-col gap-0.5">
                        <span className="text-sm">{option.label}</span>
                        <span className="text-xs text-muted-foreground">{option.description}</span>
                      </span>
                    </RadioGroup.Item>
                  ))}
                </RadioGroup.Root>
              </SettingGroup>

              <div className="flex flex-col gap-2.5 border-t border-border pt-3">
                <ToggleRow
                  testId="setting-teach"
                  label="Teach me as you go"
                  description="Explanations include a short lesson on how it works, pointing at the real files."
                  checked={settings?.teach ?? false}
                  disabled={!settings}
                  onChange={(teach) => settings && void save({ ...settings, teach })}
                />
                <ToggleRow
                  testId="setting-auto-fix"
                  label="Fix errors automatically"
                  description="When your game shows an error, the AI tries to fix it on its own."
                  checked={settings?.auto_fix ?? false}
                  disabled={!settings}
                  onChange={(auto_fix) => settings && void save({ ...settings, auto_fix })}
                />
              </div>

              {saveError && (
                <div className="flex flex-col gap-1 rounded-lg border border-destructive/30 bg-destructive/5 p-2.5">
                  <span className="flex items-center gap-1.5 text-xs font-medium">
                    <AlertCircle className="size-3.5 text-destructive" />
                    Couldn't save that change
                  </span>
                  <span className="font-mono text-[11px] break-words text-muted-foreground">{saveError}</span>
                </div>
              )}
            </div>
          )}
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}

function SettingGroup({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5">
      <span className="text-[11px] font-medium tracking-wide text-muted-foreground uppercase">{label}</span>
      {children}
    </div>
  );
}

function ToggleRow({
  testId,
  label,
  description,
  checked,
  disabled,
  onChange,
}: {
  testId: string;
  label: string;
  description: string;
  checked: boolean;
  disabled: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className="flex cursor-pointer items-start gap-3 has-disabled:cursor-default has-disabled:opacity-50">
      <span className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="text-sm">{label}</span>
        <span className="text-xs text-muted-foreground">{description}</span>
      </span>
      <Switch.Root
        data-testid={testId}
        checked={checked}
        disabled={disabled}
        onCheckedChange={onChange}
        className="relative mt-0.5 inline-flex h-5 w-9 shrink-0 items-center rounded-full border border-transparent bg-foreground/15 transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring/50 data-[state=checked]:bg-primary"
      >
        <Switch.Thumb className="block size-4 translate-x-0.5 rounded-full bg-background shadow-sm transition-transform data-[state=checked]:translate-x-[18px]" />
      </Switch.Root>
    </label>
  );
}
