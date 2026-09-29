import { useEffect, useRef, useState } from "react";
import { Loader2, RotateCcw, Sparkles, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { generateAccept, generateDiscard, generateRun } from "@/lib/studio-api";
import { cn } from "@/lib/utils";
import type { AssetInfo, GenKind, GenOptions, GenPreview, GenRequest } from "@/lib/studio-types";
import { GEN_KIND_LABEL, dataUrl, errorText } from "./assets-format";
import { Chips, Field, Note, checkerStyle } from "./shared";

const SIZE_PRESETS = [64, 128, 256, 512, 1024] as const;

const baseOptions = (): GenOptions => ({
  model: null,
  width: null,
  height: null,
  seed: null,
  pixel_art: false,
  transparent: false,
  duration_seconds: null,
  voice_id: null,
  looping: false,
});

const PROMPT_LABEL: Record<GenKind, string> = {
  image: "What should the picture show?",
  voice: "What should the voice say?",
  sfx: "What should the sound effect sound like?",
  music: "What should the music feel like?",
};

const PROMPT_HINT: Record<GenKind, string> = {
  image: "A small green slime with big eyes, facing right",
  voice: "Welcome, traveler. The village needs your help.",
  sfx: "A wooden door creaking open",
  music: "Calm village theme with soft flute and guitar",
};

const toNumber = (s: string): number | null => {
  const n = Number(s);
  return s.trim() !== "" && Number.isFinite(n) ? n : null;
};

interface GenerateFormProps {
  kind: GenKind;
  projectPath: string;
  styleGuide: string | null;
  onAccepted: (asset: AssetInfo) => void;
}

/** One generator: the settings for a kind, the running state, and the result card. */
export function GenerateForm({ kind, projectPath, styleGuide, onAccepted }: GenerateFormProps) {
  const [prompt, setPrompt] = useState("");
  const [size, setSize] = useState<number | "custom">(512);
  const [customW, setCustomW] = useState("512");
  const [customH, setCustomH] = useState("512");
  const [pixelArt, setPixelArt] = useState(false);
  const [transparent, setTransparent] = useState(false);
  const [seed, setSeed] = useState("");
  const [voiceId, setVoiceId] = useState("");
  const [sfxSeconds, setSfxSeconds] = useState("2");
  const [musicSeconds, setMusicSeconds] = useState("30");
  const [looping, setLooping] = useState(false);

  const [running, setRunning] = useState(false);
  const [elapsed, setElapsed] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<GenPreview | null>(null);
  const [accepting, setAccepting] = useState(false);
  const [accepted, setAccepted] = useState<string | null>(null);
  const resultRef = useRef<GenPreview | null>(null);
  resultRef.current = result;

  useEffect(() => {
    if (!running) return;
    const start = Date.now();
    setElapsed(0);
    const id = window.setInterval(() => setElapsed(Math.floor((Date.now() - start) / 1000)), 500);
    return () => window.clearInterval(id);
  }, [running]);

  // Leaving with an un-accepted preview would strand its temp file.
  useEffect(
    () => () => {
      if (resultRef.current) void generateDiscard(resultRef.current.temp_id).catch(() => undefined);
    },
    [],
  );

  const sfxNum = toNumber(sfxSeconds);
  const musicNum = toNumber(musicSeconds);
  const w = size === "custom" ? toNumber(customW) : size;
  const h = size === "custom" ? toNumber(customH) : size;

  const settingsError: string | null =
    kind === "image" && (!w || !h || w < 1 || h < 1)
      ? "Enter a width and height in pixels."
      : kind === "sfx" && (sfxNum === null || sfxNum < 0.5 || sfxNum > 22)
        ? "A sound effect can be between 0.5 and 22 seconds long."
        : kind === "music" && (musicNum === null || musicNum < 1)
          ? "Enter how long the music should be, in seconds."
          : null;

  const buildRequest = (): GenRequest => {
    const o = baseOptions();
    if (kind === "image") {
      o.width = w;
      o.height = h;
      o.pixel_art = pixelArt;
      o.transparent = transparent;
      o.seed = toNumber(seed);
    }
    if (kind === "voice") o.voice_id = voiceId.trim() || null;
    if (kind === "sfx") o.duration_seconds = sfxNum;
    if (kind === "music") {
      o.duration_seconds = musicNum;
      o.looping = looping;
    }
    return { kind, prompt: prompt.trim(), options: o, style_guide: styleGuide };
  };

  const discardCurrent = async () => {
    const cur = resultRef.current;
    setResult(null);
    if (cur) {
      try {
        await generateDiscard(cur.temp_id);
      } catch (e) {
        setError(errorText(e));
      }
    }
  };

  const run = async () => {
    setError(null);
    setAccepted(null);
    setRunning(true);
    const previous = resultRef.current;
    try {
      const preview = await generateRun(projectPath, buildRequest());
      if (previous) void generateDiscard(previous.temp_id).catch(() => undefined);
      setResult(preview);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setRunning(false);
    }
  };

  const ready = prompt.trim().length > 0 && !settingsError && !running;

  return (
    <div className="flex flex-col gap-4">
      <form
        className="flex flex-col gap-3 rounded-xl border border-border bg-card p-4"
        onSubmit={(e) => {
          e.preventDefault();
          if (ready) void run();
        }}
      >
        <fieldset disabled={running} className="flex min-w-0 flex-col gap-3 disabled:opacity-70">
          <Field label={PROMPT_LABEL[kind]}>
            <textarea
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              rows={3}
              placeholder={PROMPT_HINT[kind]}
              className="w-full resize-y rounded-lg border border-input bg-transparent px-2.5 py-2 text-sm font-normal text-foreground outline-none placeholder:text-muted-foreground focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 dark:bg-input/30"
            />
          </Field>

          {kind === "image" && (
            <>
              <div className="flex flex-col gap-1">
                <span className="text-xs font-medium text-muted-foreground">Size (pixels)</span>
                <div className="flex flex-wrap items-center gap-2">
                  <Chips
                    label="Size"
                    value={size === "custom" ? "custom" : String(size)}
                    onChange={(id) => id && setSize(id === "custom" ? "custom" : Number(id))}
                    options={[
                      ...SIZE_PRESETS.map((s) => ({ id: String(s), label: `${s}` })),
                      { id: "custom", label: "Custom" },
                    ]}
                  />
                  {size === "custom" && (
                    <div className="flex items-center gap-1.5 text-sm">
                      <Input className="h-7 w-20" inputMode="numeric" aria-label="Width" value={customW} onChange={(e) => setCustomW(e.target.value)} />
                      ×
                      <Input className="h-7 w-20" inputMode="numeric" aria-label="Height" value={customH} onChange={(e) => setCustomH(e.target.value)} />
                    </div>
                  )}
                </div>
              </div>
              <div className="flex flex-wrap items-center gap-x-5 gap-y-2 text-sm">
                <label className="flex items-center gap-2">
                  <input type="checkbox" checked={pixelArt} onChange={(e) => setPixelArt(e.target.checked)} />
                  Pixel art
                </label>
                <label className="flex items-center gap-2">
                  <input type="checkbox" checked={transparent} onChange={(e) => setTransparent(e.target.checked)} />
                  Transparent background
                </label>
                <label className="flex items-center gap-2 text-muted-foreground">
                  Seed (optional)
                  <Input className="h-7 w-28" inputMode="numeric" value={seed} onChange={(e) => setSeed(e.target.value)} />
                </label>
              </div>
            </>
          )}

          {kind === "voice" && (
            <Field label="Voice ID (optional)" hint="Leave empty to use the provider's default voice.">
              <Input value={voiceId} onChange={(e) => setVoiceId(e.target.value)} className="max-w-72" />
            </Field>
          )}

          {kind === "sfx" && (
            <Field label="Length in seconds (0.5 to 22)">
              <Input className="w-28" inputMode="decimal" value={sfxSeconds} onChange={(e) => setSfxSeconds(e.target.value)} />
            </Field>
          )}

          {kind === "music" && (
            <div className="flex flex-wrap items-end gap-4">
              <Field label="Length in seconds">
                <Input className="w-28" inputMode="numeric" value={musicSeconds} onChange={(e) => setMusicSeconds(e.target.value)} />
              </Field>
              <label className="flex h-8 items-center gap-2 text-sm">
                <input type="checkbox" checked={looping} onChange={(e) => setLooping(e.target.checked)} />
                Make it loop
              </label>
            </div>
          )}
        </fieldset>

        {settingsError && prompt.trim() && <p className="text-xs text-destructive">{settingsError}</p>}

        <div className="flex items-center gap-3">
          <Button type="submit" disabled={!ready}>
            {running ? <Loader2 className="animate-spin" /> : <Sparkles />}
            Generate
          </Button>
          {running && (
            <span role="status" className="text-sm text-muted-foreground">
              Generating your {GEN_KIND_LABEL[kind].toLowerCase()}… {elapsed}s. This can take a little while.
            </span>
          )}
        </div>
        {error && <Note tone="error">{error}</Note>}
        {accepted && <Note tone="success">{accepted}</Note>}
      </form>

      {result && (
        <ResultCard
          result={result}
          busy={running}
          onTryAgain={() => void run()}
          onDiscard={() => void discardCurrent()}
          onAccept={() => setAccepting(true)}
        />
      )}

      <AcceptDialog
        open={accepting && result !== null}
        result={result}
        kind={kind}
        prompt={prompt}
        projectPath={projectPath}
        onClose={() => setAccepting(false)}
        onAccepted={(asset) => {
          setAccepting(false);
          setResult(null);
          setAccepted(`Added ${asset.path} to your game. You can find it in Project.`);
          onAccepted(asset);
        }}
      />
    </div>
  );
}

function ResultCard({
  result,
  busy,
  onTryAgain,
  onDiscard,
  onAccept,
}: {
  result: GenPreview;
  busy: boolean;
  onTryAgain: () => void;
  onDiscard: () => void;
  onAccept: () => void;
}) {
  const src = dataUrl(result.mime, result.base64);
  const isImage = result.mime.startsWith("image/");
  return (
    <div className="flex flex-col gap-3 rounded-xl border border-border bg-card p-4" data-testid="gen-result">
      <div className="flex items-center justify-between gap-2">
        <h3 className="text-sm font-medium">Here's what came back</h3>
        <span className="text-xs text-muted-foreground">
          {result.provider} · {result.model} · {(result.duration_ms / 1000).toFixed(1)}s
        </span>
      </div>
      {isImage ? (
        <div style={checkerStyle} className="flex max-h-80 items-center justify-center overflow-hidden rounded-lg border border-border p-3">
          <img src={src} alt="Generated result" className="max-h-72 max-w-full object-contain [image-rendering:pixelated]" />
        </div>
      ) : (
        <audio controls src={src} className="w-full" />
      )}
      <div className="flex flex-col gap-1">
        <span className="text-xs font-medium text-muted-foreground">What was actually sent</span>
        <p className={cn("rounded-lg bg-muted/50 p-2.5 text-xs whitespace-pre-wrap break-words select-text")}>
          {result.prompt_used}
        </p>
      </div>
      <div className="flex flex-wrap gap-2">
        <Button onClick={onAccept} disabled={busy}>
          Accept
        </Button>
        <Button variant="outline" onClick={onTryAgain} disabled={busy}>
          {busy ? <Loader2 className="animate-spin" /> : <RotateCcw />}
          Try again
        </Button>
        <Button variant="ghost" onClick={onDiscard} disabled={busy}>
          <Trash2 />
          Discard
        </Button>
      </div>
    </div>
  );
}

function AcceptDialog({
  open,
  result,
  kind,
  prompt,
  projectPath,
  onClose,
  onAccepted,
}: {
  open: boolean;
  result: GenPreview | null;
  kind: GenKind;
  prompt: string;
  projectPath: string;
  onClose: () => void;
  onAccepted: (asset: AssetInfo) => void;
}) {
  const [title, setTitle] = useState("");
  const [folder, setFolder] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    setTitle(
      prompt
        .trim()
        .split(/\s+/)
        .slice(0, 4)
        .join(" ")
        .replace(/[^\p{L}\p{N} _-]/gu, "")
        .trim() || GEN_KIND_LABEL[kind],
    );
    setFolder("");
    setError(null);
    setBusy(false);
  }, [open, kind, prompt]);

  const accept = async () => {
    if (!result) return;
    setBusy(true);
    setError(null);
    try {
      onAccepted(await generateAccept(projectPath, result.temp_id, title.trim(), folder.trim() || undefined));
    } catch (e) {
      setError(errorText(e));
      setBusy(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={(o) => !o && !busy && onClose()}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Add it to your game</DialogTitle>
          <DialogDescription>It will be recorded as generated by {result?.provider}, so your credits stay honest.</DialogDescription>
        </DialogHeader>
        <div className="flex flex-col gap-3">
          <Field label="Name">
            <Input value={title} onChange={(e) => setTitle(e.target.value)} autoFocus />
          </Field>
          <Field label="Folder inside your game (optional)">
            <Input value={folder} onChange={(e) => setFolder(e.target.value)} placeholder="Leave empty to use the default" />
          </Field>
          {error && <Note tone="error">{error}</Note>}
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button onClick={() => void accept()} disabled={busy || !title.trim()}>
            {busy && <Loader2 className="animate-spin" />}
            Add to game
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
