import { useEffect, useMemo, useRef, useState, type PointerEvent } from "react";
import { Pause, Play, Repeat } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { base64ToBytes, formatSeconds } from "./assets-format";

// Audio preview: decode the bytes with the Web Audio API to draw the
// waveform, and play them through an <audio> element (play/pause, seek,
// loop). The waveform doubles as the seek bar.

interface AudioPreviewProps {
  mime: string;
  base64: string;
}

const PEAK_COLUMNS = 400;

function peaksOf(buffer: AudioBuffer, columns: number): Float32Array {
  const out = new Float32Array(columns);
  const per = Math.max(1, Math.floor(buffer.length / columns));
  for (let ch = 0; ch < buffer.numberOfChannels; ch++) {
    const data = buffer.getChannelData(ch);
    for (let c = 0; c < columns; c++) {
      let peak = 0;
      const start = c * per;
      const end = Math.min(buffer.length, start + per);
      for (let i = start; i < end; i++) {
        const v = Math.abs(data[i]);
        if (v > peak) peak = v;
      }
      if (peak > out[c]) out[c] = peak;
    }
  }
  return out;
}

export function AudioPreview({ mime, base64 }: AudioPreviewProps) {
  const audioRef = useRef<HTMLAudioElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [peaks, setPeaks] = useState<Float32Array | null>(null);
  const [decodeError, setDecodeError] = useState<string | null>(null);
  const [duration, setDuration] = useState(0);
  const [time, setTime] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [loop, setLoop] = useState(false);

  const url = useMemo(() => URL.createObjectURL(new Blob([base64ToBytes(base64)], { type: mime })), [mime, base64]);
  useEffect(() => () => URL.revokeObjectURL(url), [url]);

  useEffect(() => {
    let cancelled = false;
    setPeaks(null);
    setDecodeError(null);
    const Ctx: typeof AudioContext | undefined =
      window.AudioContext ?? (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
    if (!Ctx) {
      setDecodeError("This computer can't draw a sound wave for this file.");
      return;
    }
    const ctx = new Ctx();
    const bytes = base64ToBytes(base64);
    ctx
      .decodeAudioData(bytes.buffer)
      .then((buffer) => {
        if (cancelled) return;
        setDuration(buffer.duration);
        setPeaks(peaksOf(buffer, PEAK_COLUMNS));
      })
      .catch(() => {
        if (!cancelled) setDecodeError("This sound file couldn't be read, so there's no wave to show.");
      })
      .finally(() => {
        void ctx.close().catch(() => undefined);
      });
    return () => {
      cancelled = true;
    };
  }, [base64]);

  // Draw the waveform and the play position.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const rect = canvas.getBoundingClientRect();
    const dpr = window.devicePixelRatio || 1;
    const w = Math.max(1, Math.floor(rect.width * dpr));
    const h = Math.max(1, Math.floor(rect.height * dpr));
    if (canvas.width !== w) canvas.width = w;
    if (canvas.height !== h) canvas.height = h;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const style = getComputedStyle(canvas);
    const fg = style.color;
    ctx.clearRect(0, 0, w, h);
    if (!peaks) return;
    const played = duration > 0 ? time / duration : 0;
    const bar = w / peaks.length;
    for (let i = 0; i < peaks.length; i++) {
      const amp = Math.max(1, peaks[i] * (h / 2 - 2));
      ctx.fillStyle = i / peaks.length <= played ? "#3b82f6" : fg;
      ctx.globalAlpha = i / peaks.length <= played ? 1 : 0.45;
      ctx.fillRect(i * bar, h / 2 - amp, Math.max(1, bar - 1), amp * 2);
    }
    ctx.globalAlpha = 1;
  }, [peaks, time, duration]);

  const toggle = () => {
    const el = audioRef.current;
    if (!el) return;
    if (el.paused) void el.play().catch(() => setPlaying(false));
    else el.pause();
  };

  const seek = (e: PointerEvent<HTMLCanvasElement>) => {
    const el = audioRef.current;
    if (!el || !duration) return;
    const rect = e.currentTarget.getBoundingClientRect();
    el.currentTime = Math.min(1, Math.max(0, (e.clientX - rect.left) / rect.width)) * duration;
    setTime(el.currentTime);
  };

  return (
    <div className="flex flex-col gap-3">
      <div className="rounded-lg border border-border bg-muted/30 p-3">
        {decodeError ? (
          <p className="py-6 text-center text-sm text-muted-foreground">{decodeError}</p>
        ) : (
          <canvas
            ref={canvasRef}
            data-testid="waveform"
            onPointerDown={seek}
            className="h-24 w-full cursor-pointer text-foreground"
            aria-label="Sound wave. Click to jump to a spot."
          />
        )}
        {!decodeError && !peaks && <p className="text-center text-xs text-muted-foreground">Reading the sound…</p>}
      </div>
      <audio
        ref={audioRef}
        src={url}
        loop={loop}
        onPlay={() => setPlaying(true)}
        onPause={() => setPlaying(false)}
        onEnded={() => setPlaying(false)}
        onTimeUpdate={(e) => setTime(e.currentTarget.currentTime)}
        onLoadedMetadata={(e) => {
          if (Number.isFinite(e.currentTarget.duration)) setDuration((d) => d || e.currentTarget.duration);
        }}
      />
      <div className="flex items-center gap-2">
        <Button variant="outline" size="sm" onClick={toggle}>
          {playing ? <Pause /> : <Play />}
          {playing ? "Pause" : "Play"}
        </Button>
        <Button
          variant="outline"
          size="sm"
          aria-pressed={loop}
          onClick={() => setLoop((l) => !l)}
          className={cn(loop && "border-foreground")}
        >
          <Repeat />
          Loop
        </Button>
        <span className="ml-auto text-xs tabular-nums text-muted-foreground">
          {formatSeconds(time)} / {formatSeconds(duration)}
        </span>
      </div>
    </div>
  );
}
