import { assetsReadBase64 } from "@/lib/studio-api";
import type { AssetInfo } from "@/lib/studio-types";
import { dataUrl } from "./assets-format";

// Thumbnails for the grid: only small images are read (a big picture would
// cost a large IPC round-trip per card), a few at a time, and each is
// remembered for the session so scrolling back doesn't re-read it.

export const THUMB_MAX_BYTES = 400 * 1024;
const MAX_PARALLEL = 4;

const cache = new Map<string, Promise<string | null>>();
let running = 0;
const waiting: (() => void)[] = [];

function acquire(): Promise<void> {
  if (running < MAX_PARALLEL) {
    running++;
    return Promise.resolve();
  }
  return new Promise((resolve) => waiting.push(resolve));
}

function release() {
  const next = waiting.shift();
  if (next) next();
  else running--;
}

export function canThumbnail(asset: AssetInfo): boolean {
  return asset.kind === "image" && asset.size_bytes <= THUMB_MAX_BYTES;
}

/** A `data:` URL for the asset, or `null` when it can't be shown. */
export function loadThumbnail(projectPath: string, asset: AssetInfo): Promise<string | null> {
  const key = `${projectPath}\0${asset.path}\0${asset.size_bytes}`;
  let hit = cache.get(key);
  if (!hit) {
    hit = (async () => {
      await acquire();
      try {
        const file = await assetsReadBase64(projectPath, asset.path, THUMB_MAX_BYTES);
        return file.truncated || !file.base64 ? null : dataUrl(file.mime, file.base64);
      } catch {
        return null;
      } finally {
        release();
      }
    })();
    cache.set(key, hit);
  }
  return hit;
}
