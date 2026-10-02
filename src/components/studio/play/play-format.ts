import type { GameError, GameOutputLine } from "@/lib/studio-types";

// Plain formatting helpers for the Play panel — no React, so they stay
// trivially testable if a frontend test runner ever lands.

/** "12.4 MB" — binary units, one decimal past KB. Real byte counts only;
 * callers never pass an estimate. */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(1)} ${units[unit]}`;
}

/** Godot's real `--version` output is e.g. "4.7.2.stable.official.ed1daf0bf"
 * (see crates/core/tests/fixtures/godot/version.txt) — show just the
 * leading "4.7.2", keeping the full string for a tooltip. Falls back to the
 * whole string if it doesn't start with a number. */
export function shortGodotVersion(version: string): string {
  const match = version.match(/^\d+(\.\d+)*/);
  return match ? match[0] : version;
}

/** "res://main.gd, line 7" / "res://main.gd" / null. */
export function errorLocation(error: GameError): string | null {
  if (!error.file) return null;
  return error.line !== null ? `${error.file}, line ${error.line}` : error.file;
}

/** The `res://` script named by Godot's follow-up to a script that didn't
 * compile — `ERROR: Failed to load script "res://player.gd" with error
 * "Parse error".` (see crates/core/tests/fixtures/godot/parse_error.stderr.txt).
 * That block's own location is engine C++ source, so it arrives with no
 * `file`; the script is only named in the message. */
export function scriptLoadFailurePath(error: GameError): string | null {
  const match = error.message.match(/^Failed to load script "(res:\/\/[^"]+)"/);
  return match ? match[1] : null;
}

/** A GDScript compile error with its location (`SCRIPT ERROR: Parse Error:
 * ...` at `res://file.gd:line`). */
export function isParseError(error: GameError): boolean {
  return error.file !== null && error.message.startsWith("Parse Error:");
}

/** The file in the user's own project an error is about, if any: its
 * location, or else a `res://` path its message names (a script that
 * failed to load, a missing resource). `null` means Godot didn't tie it to
 * anything in the project (e.g. an engine or sound-driver problem), so
 * there's nothing specific to ask the AI to fix. */
export function projectFile(error: GameError): string | null {
  if (error.file?.startsWith("res://")) return error.file;
  return scriptLoadFailurePath(error) ?? error.message.match(/res:\/\/[^\s"'(),]+/)?.[0] ?? null;
}

/** The message "Ask AI to fix" puts in the chat. Plain enough for the user
 * to read in their own chat history, specific enough (file, line, Godot's
 * full raw output, including follow-up messages grouped under this error)
 * for the agent to act on without asking. */
export function fixRequestMessage(error: GameError, related: GameError[] = []): string {
  const location = errorLocation(error);
  const lines = [
    "My game showed this error when I ran it. Please find the cause and fix it.",
    "",
    `Error: ${error.message}`,
  ];
  if (location) lines.push(`Where: ${location}`);
  const raw = [error, ...related]
    .map((e) => e.raw.trim() || e.message.trim())
    .filter(Boolean)
    .join("\n");
  if (raw && raw !== error.message.trim()) {
    lines.push("", "Godot's full error output:", "```", raw, "```");
  }
  return lines.join("\n");
}

/** Identity for collapsing repeats: a game erroring every frame prints the
 * same error thousands of times, which is one problem, not thousands. */
export function errorKey(error: GameError): string {
  return `${error.message}\u0000${error.file ?? ""}\u0000${error.line ?? ""}`;
}

export type OutputLineKind = "error" | "error-detail" | "warning" | "normal";

/** How a log line gets highlighted. Godot prints error blocks on stderr as a
 * `SCRIPT ERROR:` / `ERROR:` headline followed by indented `at:` and
 * backtrace lines (see crates/core/tests/fixtures/README.md), and exits 0
 * even when scripts fail — so the text, not the exit code, is the signal. */
export function classifyOutputLine(line: GameOutputLine): OutputLineKind {
  const text = line.text.trimStart();
  if (/^(SCRIPT )?ERROR:/.test(text)) return "error";
  if (/^(USER )?WARNING:/.test(text)) return "warning";
  if (line.stream === "stderr") return "error-detail";
  return "normal";
}
