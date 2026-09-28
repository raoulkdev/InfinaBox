// Advanced → Settings → Godot: which Godot InfinaBox runs the game with,
// a way to point it at the person's own copy instead of the managed one,
// and "Open in Godot editor". Everything shown comes from the backend
// (`godot_status`, `app_settings_get`); when a call fails, its own message
// is shown as-is rather than a guessed value.

import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { AlertCircle, ExternalLink, FolderOpen, Gamepad2, Loader2, RotateCcw } from "lucide-react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/skeleton";
import { appSettingsGet, appSettingsSet, godotOpenEditor, godotStatus } from "@/lib/studio-api";
import type { AppSettings, GodotStatus } from "@/lib/studio-types";

export interface GodotSettingsProps {
  projectPath: string | null;
  /** Bumped by the parent whenever the Settings tab is shown, so the status
   * is re-read then: Godot may have been set up from Home or Studio since
   * this last looked. */
  refreshToken: number;
}

type Loadable<T> = { status: "loading" } | { status: "error"; message: string } | { status: "ready"; value: T };

type Notice = { tone: "info" | "error"; text: string };

function errorText(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

export function GodotSettings({ projectPath, refreshToken }: GodotSettingsProps) {
  const [godot, setGodot] = useState<Loadable<GodotStatus>>({ status: "loading" });
  const [settings, setSettings] = useState<Loadable<AppSettings>>({ status: "loading" });
  // The custom path as typed — a draft until saved.
  const [draft, setDraft] = useState("");
  const [saving, setSaving] = useState(false);
  const [pathNotice, setPathNotice] = useState<Notice | null>(null);
  const [opening, setOpening] = useState(false);
  const [editorNotice, setEditorNotice] = useState<Notice | null>(null);

  const loadGodot = useCallback(async () => {
    setGodot({ status: "loading" });
    try {
      setGodot({ status: "ready", value: await godotStatus() });
    } catch (err) {
      setGodot({ status: "error", message: errorText(err) });
    }
  }, []);

  const loadSettings = useCallback(async () => {
    setSettings({ status: "loading" });
    try {
      const value = await appSettingsGet();
      setSettings({ status: "ready", value });
      setDraft(value.godot_path ?? "");
    } catch (err) {
      setSettings({ status: "error", message: errorText(err) });
    }
  }, []);

  useEffect(() => {
    void loadGodot();
  }, [loadGodot, refreshToken]);

  useEffect(() => {
    void loadSettings();
  }, [loadSettings]);

  // A note about opening the editor belongs to the project it was about.
  useEffect(() => {
    setEditorNotice(null);
  }, [projectPath]);

  const savedPath = settings.status === "ready" ? settings.value.godot_path : null;
  const trimmedDraft = draft.trim();

  async function savePath(path: string | null) {
    setSaving(true);
    setPathNotice(null);
    try {
      // Re-read first so a field changed elsewhere since this card loaded
      // (the chosen AI, first-run state) is kept, not overwritten.
      const current = await appSettingsGet();
      const saved = await appSettingsSet({ ...current, godot_path: path });
      setSettings({ status: "ready", value: saved });
      setDraft(saved.godot_path ?? "");
      setPathNotice({
        tone: "info",
        text: saved.godot_path ? "Saved. InfinaBox will use this Godot." : "Saved. InfinaBox will use its own Godot.",
      });
      void loadGodot();
    } catch (err) {
      setPathNotice({ tone: "error", text: errorText(err) });
    } finally {
      setSaving(false);
    }
  }

  async function choosePath() {
    try {
      const picked = await open({ directory: false, multiple: false, title: "Choose your Godot program" });
      if (typeof picked === "string") {
        setDraft(picked);
        setPathNotice(null);
      }
    } catch (err) {
      setPathNotice({ tone: "error", text: errorText(err) });
    }
  }

  async function openEditor() {
    if (!projectPath) return;
    setOpening(true);
    setEditorNotice(null);
    try {
      await godotOpenEditor(projectPath);
      setEditorNotice({ tone: "info", text: "Godot is opening your project in its editor." });
    } catch (err) {
      setEditorNotice({ tone: "error", text: errorText(err) });
    } finally {
      setOpening(false);
    }
  }

  return (
    <section className="flex flex-col gap-4 rounded-xl border border-border bg-card p-4">
      <div className="flex items-start gap-3">
        <div className="flex size-9 shrink-0 items-center justify-center rounded-lg border border-border">
          <Gamepad2 className="size-4 text-muted-foreground" />
        </div>
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <h2 className="text-sm font-medium">Godot</h2>
          <p className="text-sm text-muted-foreground">
            The free game engine your game runs in. InfinaBox sets up its own copy, or you can use one you
            already have.
          </p>
        </div>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={() => void loadGodot()}
          disabled={godot.status === "loading"}
        >
          <RotateCcw />
          Check again
        </Button>
      </div>

      {/* Current status */}
      <div className="rounded-lg border border-border px-3 py-2.5">
        {godot.status === "loading" && (
          <div className="flex flex-col gap-2">
            <Skeleton className="h-4 w-40" />
            <Skeleton className="h-3 w-3/4" />
          </div>
        )}
        {godot.status === "error" && (
          <Alert variant="destructive">
            <AlertCircle />
            <AlertDescription>{godot.message}</AlertDescription>
          </Alert>
        )}
        {godot.status === "ready" &&
          (godot.value.installed ? (
            <div className="flex min-w-0 flex-col gap-1">
              <div className="flex flex-wrap items-center gap-2">
                <span className="text-sm font-medium">
                  {godot.value.version ? `Godot ${godot.value.version}` : "Godot (version unknown)"}
                </span>
                <Badge variant="secondary">
                  {godot.value.managed ? "Set up by InfinaBox" : "Your own copy"}
                </Badge>
              </div>
              {godot.value.path && (
                <p className="font-mono text-xs break-all text-muted-foreground">{godot.value.path}</p>
              )}
            </div>
          ) : (
            <p className="text-sm text-muted-foreground">
              {savedPath
                ? "No Godot found at the path you chose below. Check the path, or switch back to the managed Godot."
                : "No Godot found yet. Set it up from Home, or choose a copy you already have below."}
            </p>
          ))}
      </div>

      {/* Custom path */}
      <div className="flex flex-col gap-2">
        <label htmlFor="godot-custom-path" className="text-sm font-medium">
          Use my own Godot
        </label>
        {settings.status === "error" ? (
          <Alert variant="destructive">
            <AlertCircle />
            <AlertDescription className="flex flex-col items-start gap-2">
              <span>{settings.message}</span>
              <Button type="button" size="xs" variant="outline" onClick={() => void loadSettings()}>
                Try again
              </Button>
            </AlertDescription>
          </Alert>
        ) : (
          <>
            <div className="flex gap-2">
              <Input
                id="godot-custom-path"
                value={draft}
                placeholder="Path to the Godot program"
                disabled={settings.status !== "ready" || saving}
                onChange={(e) => {
                  setDraft(e.target.value);
                  setPathNotice(null);
                }}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && trimmedDraft && trimmedDraft !== savedPath) void savePath(trimmedDraft);
                }}
                className="font-mono text-xs md:text-xs"
              />
              <Button
                type="button"
                variant="outline"
                onClick={() => void choosePath()}
                disabled={settings.status !== "ready" || saving}
              >
                <FolderOpen />
                Choose…
              </Button>
            </div>
            <div className="flex flex-wrap gap-2">
              <Button
                type="button"
                size="sm"
                onClick={() => void savePath(trimmedDraft)}
                disabled={settings.status !== "ready" || saving || !trimmedDraft || trimmedDraft === savedPath}
              >
                {saving && <Loader2 className="animate-spin" />}
                Save
              </Button>
              <Button
                type="button"
                size="sm"
                variant="ghost"
                onClick={() => void savePath(null)}
                disabled={settings.status !== "ready" || saving || savedPath === null}
              >
                Use the managed Godot
              </Button>
            </div>
          </>
        )}
        {pathNotice && (
          <p className={pathNotice.tone === "error" ? "text-sm text-destructive" : "text-sm text-muted-foreground"}>
            {pathNotice.text}
          </p>
        )}
      </div>

      {/* Open in Godot editor */}
      <div className="flex flex-col gap-2 border-t border-border pt-4">
        <div className="flex flex-wrap items-center gap-3">
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={() => void openEditor()}
            disabled={!projectPath || opening}
          >
            {opening ? <Loader2 className="animate-spin" /> : <ExternalLink />}
            Open in Godot editor
          </Button>
          <span className="text-sm text-muted-foreground">
            {projectPath
              ? "Opens this project in Godot's own editor, for when you want to look around yourself."
              : "Open a project first."}
          </span>
        </div>
        {editorNotice && (
          <p className={editorNotice.tone === "error" ? "text-sm text-destructive" : "text-sm text-muted-foreground"}>
            {editorNotice.text}
          </p>
        )}
      </div>
    </section>
  );
}
