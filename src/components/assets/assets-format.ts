import type { AssetInfo, AssetKind, GenKind, LicenseInfo } from "@/lib/studio-types";

// Small pure helpers shared by the Assets screens.

export const KIND_LABEL: Record<AssetKind, string> = {
  image: "Image",
  audio: "Audio",
  model3d: "3D model",
  font: "Font",
  other: "File",
};

/** The kinds a person can filter or search by (not "other"). */
export const FILTER_KINDS: AssetKind[] = ["image", "audio", "model3d", "font"];

export const GEN_KIND_LABEL: Record<GenKind, string> = {
  image: "Image",
  voice: "Voice",
  sfx: "Sound effect",
  music: "Music",
};

export function fileName(path: string): string {
  return path.split("/").filter(Boolean).pop() ?? path;
}

export function fileExtension(path: string): string {
  const name = fileName(path);
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(bytes < 10 * 1024 ? 1 : 0)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function formatDimensions(a: AssetInfo): string | null {
  return a.width && a.height ? `${a.width} × ${a.height}` : null;
}

export function formatSeconds(s: number): string {
  if (!Number.isFinite(s)) return "0:00";
  const whole = Math.max(0, Math.floor(s));
  const m = Math.floor(whole / 60);
  const rest = String(whole % 60).padStart(2, "0");
  return `${m}:${rest}`;
}

/** An asset counts as licensed when it has a license name or says it was generated. */
export function hasLicense(license: LicenseInfo | null): boolean {
  return !!license && !!(license.name?.trim() || license.generated_by?.trim());
}

export function licenseLabel(license: LicenseInfo | null): string {
  if (license?.name?.trim()) return license.name.trim();
  if (license?.generated_by?.trim()) return "Generated";
  return "License unknown";
}

export function base64ToBytes(b64: string): Uint8Array<ArrayBuffer> {
  const bin = atob(b64);
  const bytes = new Uint8Array(new ArrayBuffer(bin.length));
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  return bytes;
}

export function dataUrl(mime: string, base64: string): string {
  return `data:${mime};base64,${base64}`;
}

/** The message of whatever a Tauri command rejected with, as plain text. */
export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  try {
    return JSON.stringify(e);
  } catch {
    return String(e);
  }
}

export const LICENSE_CHOICES = ["CC0-1.0", "CC-BY-4.0", "CC-BY-SA-4.0", "MIT", "I made it", "Other…"] as const;
