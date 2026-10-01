/** A file name from what a person typed: no slashes or other characters the disk dislikes. */
export function safeName(text: string): string {
  const cleaned = text
    .normalize("NFKD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 60)
    .replace(/-+$/g, "");
  return cleaned || "untitled";
}

/** `name` (with `ext`) made unique among the `taken` paths. */
export function uniquePath(name: string, ext: string, taken: Set<string>): string {
  let candidate = `${name}${ext}`;
  for (let n = 2; taken.has(candidate); n += 1) candidate = `${name}-${n}${ext}`;
  return candidate;
}

export function hostOf(url: string): string {
  try {
    return new URL(url).host.replace(/^www\./, "");
  } catch {
    return url;
  }
}

/** Only web addresses are ever opened from a board. */
export function isWebUrl(url: string): boolean {
  try {
    const u = new URL(url);
    return u.protocol === "http:" || u.protocol === "https:";
  } catch {
    return false;
  }
}

/** A typed address, with `https://` added when it has none. */
export function normalizeUrl(text: string): string {
  const t = text.trim();
  if (!t) return "";
  return /^[a-z][a-z0-9+.-]*:/i.test(t) ? t : `https://${t}`;
}

export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function fileToBase64(file: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const text = String(reader.result ?? "");
      resolve(text.slice(text.indexOf(",") + 1));
    };
    reader.onerror = () => reject(reader.error ?? new Error("That file couldn't be read."));
    reader.readAsDataURL(file);
  });
}

export const IMAGE_EXT = /\.(png|jpe?g|gif|webp|bmp|svg|avif)$/i;
