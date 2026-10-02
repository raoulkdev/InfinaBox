import { useEffect, useState } from "react";
import { assetsReadBase64, contextRead } from "@/lib/studio-api";

// Pictures on a board come from project files; they are read once and kept.

const images = new Map<string, Promise<string | null>>();

function loadImage(projectPath: string, src: string): Promise<string | null> {
  const key = `${projectPath}\n${src}`;
  let p = images.get(key);
  if (!p) {
    p = assetsReadBase64(projectPath, src, 30 * 1024 * 1024).then(
      (f) => (f.truncated ? null : `data:${f.mime};base64,${f.base64}`),
      () => null,
    );
    images.set(key, p);
  }
  return p;
}

/** A picture's data URL (null while loading or if it can't be read). */
export function useImageSrc(projectPath: string, src: string): string | null {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    setUrl(null);
    if (!src) return;
    void loadImage(projectPath, src).then((u) => !cancelled && setUrl(u));
    return () => {
      cancelled = true;
    };
  }, [projectPath, src]);
  return url;
}

/** The size of a picture file, to give an image block its shape. */
export function imageSize(dataUrl: string): Promise<{ w: number; h: number }> {
  return new Promise((resolve) => {
    const img = new Image();
    img.onload = () => resolve({ w: img.naturalWidth || 1, h: img.naturalHeight || 1 });
    img.onerror = () => resolve({ w: 4, h: 3 });
    img.src = dataUrl;
  });
}

/** The first lines of a document's text, for its card on the board. */
export function useDocPreview(projectPath: string, path: string, tick: number): string | null {
  const [text, setText] = useState<string | null>(null);
  useEffect(() => {
    let cancelled = false;
    contextRead(projectPath, path).then(
      (card) => !cancelled && setText(previewOf(card.body)),
      () => !cancelled && setText(null),
    );
    return () => {
      cancelled = true;
    };
  }, [projectPath, path, tick]);
  return text;
}

export function previewOf(body: string): string {
  return body
    .split("\n")
    .map((l) => l.replace(/^\s*(#{1,6}|[-*+]|\d+\.|>)\s+/, "").replace(/[*_`~]/g, "").trim())
    .filter(Boolean)
    .slice(0, 6)
    .join("\n")
    .slice(0, 260);
}
