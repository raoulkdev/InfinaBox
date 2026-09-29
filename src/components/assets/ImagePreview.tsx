import { useCallback, useEffect, useMemo, useRef, useState, type PointerEvent, type WheelEvent } from "react";
import { Minus, Pause, Play, Plus, RotateCcw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import { Field, checkerStyle, selectClass } from "./shared";

// Image preview: the picture on a checkerboard with zoom and pan, and a
// sprite-sheet mode that slices it into frames (grid overlay + a small
// animation preview drawn on a canvas).

interface ImagePreviewProps {
  src: string;
}

type SliceMode = "size" | "grid";

const clamp = (n: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, n));

export function ImagePreview({ src }: ImagePreviewProps) {
  const [natural, setNatural] = useState<{ w: number; h: number } | null>(null);
  const [loadError, setLoadError] = useState(false);
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState({ x: 0, y: 0 });
  const stageRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<{ x: number; y: number; px: number; py: number } | null>(null);
  const imgRef = useRef<HTMLImageElement>(null);

  const [sheet, setSheet] = useState(false);
  const [mode, setMode] = useState<SliceMode>("size");
  const [a, setA] = useState("32"); // frame width, or columns
  const [b, setB] = useState("32"); // frame height, or rows

  const fit = useCallback(() => {
    const stage = stageRef.current;
    if (!stage || !natural) return;
    const f = Math.min(stage.clientWidth / natural.w, stage.clientHeight / natural.h, 8);
    // Small pixel art is shown at a whole-number scale so it stays crisp.
    setZoom(f >= 1 ? Math.max(1, Math.floor(f)) : f);
    setPan({ x: 0, y: 0 });
  }, [natural]);

  useEffect(fit, [fit]);

  const onWheel = (e: WheelEvent) => {
    setZoom((z) => clamp(z * (e.deltaY < 0 ? 1.15 : 1 / 1.15), 0.05, 64));
  };
  const onPointerDown = (e: PointerEvent) => {
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    dragRef.current = { x: e.clientX, y: e.clientY, px: pan.x, py: pan.y };
  };
  const onPointerMove = (e: PointerEvent) => {
    const d = dragRef.current;
    if (d) setPan({ x: d.px + e.clientX - d.x, y: d.py + e.clientY - d.y });
  };
  const onPointerUp = () => {
    dragRef.current = null;
  };

  // Frame size from whichever pair of numbers the person typed.
  const frame = useMemo(() => {
    if (!natural) return null;
    const x = Math.floor(Number(a));
    const y = Math.floor(Number(b));
    if (!(x > 0) || !(y > 0)) return null;
    const fw = mode === "size" ? x : Math.floor(natural.w / x);
    const fh = mode === "size" ? y : Math.floor(natural.h / y);
    if (fw < 1 || fh < 1) return null;
    const cols = mode === "size" ? Math.floor(natural.w / fw) : x;
    const rows = mode === "size" ? Math.floor(natural.h / fh) : y;
    if (cols < 1 || rows < 1) return null;
    return { fw, fh, cols, rows, count: cols * rows };
  }, [natural, mode, a, b]);

  return (
    <div className="flex flex-col gap-3">
      <div className="relative">
        <div
          ref={stageRef}
          style={checkerStyle}
          className="relative h-72 cursor-grab touch-none overflow-hidden rounded-lg border border-border active:cursor-grabbing"
          onWheel={onWheel}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerCancel={onPointerUp}
          data-testid="image-stage"
        >
          {loadError ? (
            <p className="absolute inset-0 flex items-center justify-center p-4 text-center text-sm text-muted-foreground">
              This picture couldn't be shown.
            </p>
          ) : (
            <div
              className="absolute top-1/2 left-1/2"
              style={{
                width: natural?.w ?? 0,
                height: natural?.h ?? 0,
                transform: `translate(-50%, -50%) translate(${pan.x}px, ${pan.y}px) scale(${zoom})`,
                transformOrigin: "center",
              }}
            >
              <img
                ref={imgRef}
                src={src}
                alt="Preview"
                draggable={false}
                onLoad={(e) =>
                  setNatural({ w: e.currentTarget.naturalWidth, h: e.currentTarget.naturalHeight })
                }
                onError={() => setLoadError(true)}
                className="block size-full max-w-none [image-rendering:pixelated]"
              />
              {sheet && frame && natural && (
                <div
                  className="pointer-events-none absolute top-0 left-0"
                  style={{
                    width: frame.cols * frame.fw,
                    height: frame.rows * frame.fh,
                    backgroundImage:
                      "linear-gradient(to right, rgba(59,130,246,0.9) 0, rgba(59,130,246,0.9) 1px, transparent 1px), linear-gradient(to bottom, rgba(59,130,246,0.9) 0, rgba(59,130,246,0.9) 1px, transparent 1px)",
                    backgroundSize: `${frame.fw}px ${frame.fh}px`,
                    boxShadow: "inset -1px -1px 0 rgba(59,130,246,0.9)",
                  }}
                />
              )}
            </div>
          )}
        </div>
        <div className="absolute right-2 bottom-2 flex items-center gap-1 rounded-lg border border-border bg-popover/90 p-0.5 shadow-sm backdrop-blur">
          <Button variant="ghost" size="icon-xs" aria-label="Zoom out" onClick={() => setZoom((z) => clamp(z / 1.5, 0.05, 64))}>
            <Minus />
          </Button>
          <span className="w-10 text-center text-xs tabular-nums text-muted-foreground">{Math.round(zoom * 100)}%</span>
          <Button variant="ghost" size="icon-xs" aria-label="Zoom in" onClick={() => setZoom((z) => clamp(z * 1.5, 0.05, 64))}>
            <Plus />
          </Button>
          <Button variant="ghost" size="icon-xs" aria-label="Fit to view" onClick={fit}>
            <RotateCcw />
          </Button>
        </div>
      </div>
      {natural && (
        <p className="text-xs text-muted-foreground">
          {natural.w} × {natural.h} pixels. Scroll to zoom, drag to move.
        </p>
      )}

      <div className="rounded-lg border border-border p-3">
        <label className="flex items-center gap-2 text-sm font-medium">
          <input type="checkbox" checked={sheet} onChange={(e) => setSheet(e.target.checked)} />
          This is a sprite sheet (a picture made of animation frames)
        </label>
        {sheet && (
          <div className="mt-3 flex flex-col gap-3">
            <div className="grid grid-cols-3 gap-2">
              <Field label="Slice by">
                <select className={selectClass} value={mode} onChange={(e) => setMode(e.target.value as SliceMode)}>
                  <option value="size">Frame size</option>
                  <option value="grid">Columns × rows</option>
                </select>
              </Field>
              <Field label={mode === "size" ? "Frame width" : "Columns"}>
                <Input inputMode="numeric" value={a} onChange={(e) => setA(e.target.value)} />
              </Field>
              <Field label={mode === "size" ? "Frame height" : "Rows"}>
                <Input inputMode="numeric" value={b} onChange={(e) => setB(e.target.value)} />
              </Field>
            </div>
            {frame ? (
              <>
                <p className="text-xs text-muted-foreground">
                  {frame.count} {frame.count === 1 ? "frame" : "frames"}, each {frame.fw} × {frame.fh}.
                </p>
                <FramePlayer src={src} frame={frame} />
              </>
            ) : (
              <p className="text-xs text-muted-foreground">
                Type whole numbers that fit inside the picture to see the frames.
              </p>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

function FramePlayer({
  src,
  frame,
}: {
  src: string;
  frame: { fw: number; fh: number; cols: number; rows: number; count: number };
}) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const imgRef = useRef<HTMLImageElement | null>(null);
  const [ready, setReady] = useState(false);
  const [playing, setPlaying] = useState(true);
  const [fps, setFps] = useState("8");
  const [index, setIndex] = useState(0);

  useEffect(() => {
    const img = new Image();
    img.onload = () => {
      imgRef.current = img;
      setReady(true);
    };
    img.src = src;
    return () => {
      img.onload = null;
    };
  }, [src]);

  const rate = clamp(Number(fps) || 1, 1, 60);

  useEffect(() => {
    if (!playing || frame.count < 2) return;
    const id = window.setInterval(() => setIndex((i) => (i + 1) % frame.count), 1000 / rate);
    return () => window.clearInterval(id);
  }, [playing, rate, frame.count]);

  const shown = index % frame.count;

  useEffect(() => {
    const canvas = canvasRef.current;
    const img = imgRef.current;
    if (!canvas || !img || !ready) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.imageSmoothingEnabled = false;
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    const sx = (shown % frame.cols) * frame.fw;
    const sy = Math.floor(shown / frame.cols) * frame.fh;
    ctx.drawImage(img, sx, sy, frame.fw, frame.fh, 0, 0, canvas.width, canvas.height);
  }, [ready, shown, frame]);

  const scale = Math.max(1, Math.floor(96 / Math.max(frame.fw, frame.fh)));
  return (
    <div className="flex items-center gap-3">
      <div style={checkerStyle} className="flex size-28 items-center justify-center rounded-lg border border-border">
        <canvas
          ref={canvasRef}
          width={frame.fw}
          height={frame.fh}
          style={{ width: frame.fw * scale, height: frame.fh * scale, maxWidth: "100%", maxHeight: "100%" }}
          className="[image-rendering:pixelated]"
          aria-label="Animation preview"
        />
      </div>
      <div className="flex flex-col gap-2">
        <div className="flex items-center gap-2">
          <Button variant="outline" size="sm" onClick={() => setPlaying((p) => !p)}>
            {playing ? <Pause /> : <Play />}
            {playing ? "Pause" : "Play"}
          </Button>
          <span className="text-xs tabular-nums text-muted-foreground">
            Frame {shown + 1} of {frame.count}
          </span>
        </div>
        <label className={cn("flex items-center gap-2 text-xs text-muted-foreground")}>
          Speed
          <Input className="h-7 w-16" inputMode="numeric" value={fps} onChange={(e) => setFps(e.target.value)} />
          frames per second
        </label>
      </div>
    </div>
  );
}
