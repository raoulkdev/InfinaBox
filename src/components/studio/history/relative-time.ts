// "2 minutes ago" from a real unix-seconds timestamp (a snapshot's commit
// time). No React, so it's testable on its own.

const MINUTE = 60;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

function plural(n: number, unit: string): string {
  return `${n} ${unit}${n === 1 ? "" : "s"} ago`;
}

/** `now` is unix seconds too; passed in so a caller can re-render on a
 * ticking clock instead of each row reading `Date.now()` independently. */
export function relativeTime(timestamp: number, now: number): string {
  const diff = Math.max(0, now - timestamp);
  if (diff < 45) return "just now";
  if (diff < HOUR) return plural(Math.max(1, Math.floor(diff / MINUTE)), "minute");
  if (diff < DAY) return plural(Math.floor(diff / HOUR), "hour");
  const days = Math.floor(diff / DAY);
  if (days === 1) return "yesterday";
  if (days < 7) return plural(days, "day");
  return new Date(timestamp * 1000).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: new Date(timestamp * 1000).getFullYear() === new Date(now * 1000).getFullYear() ? undefined : "numeric",
  });
}

/** Full local date and time, for a tooltip beside the relative time. */
export function absoluteTime(timestamp: number): string {
  return new Date(timestamp * 1000).toLocaleString(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  });
}
