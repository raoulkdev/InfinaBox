import type { AgentErrorKind, ConnectAction, ProviderId, ProviderInfo } from "@/lib/studio-types";

// Plain wording for the connect panel — no React, so it stays trivially
// testable if a frontend test runner ever lands (same idea as the Play
// panel's `play-format.ts`).

/** The answer to "Signed in?", straight from the CLI's own status check.
 * `null` means the CLI couldn't say, which is shown as exactly that rather
 * than guessed either way. */
export function signedInLabel(loggedIn: boolean | null): string {
  if (loggedIn === null) return "Couldn't tell";
  return loggedIn ? "Yes" : "No";
}

/** An API key or a local model: cards built from a fixed list rather than
 * from CLI detection. */
export function isApiStyle(id: ProviderId): boolean {
  return id === "anthropic-api" || id === "openai-api" || id === "local-model";
}

/** A short, plain heading for a failed test. The real error message from
 * the backend is always shown underneath it. Pass the provider so a key or
 * a local server gets wording that fits ("That key was refused", not
 * "You're not signed in"). */
export function testErrorHeading(kind: AgentErrorKind | null, provider?: ProviderId): string {
  if (provider === "anthropic-api" || provider === "openai-api") {
    switch (kind) {
      case "not_installed":
        return "No key or model is set up yet";
      case "not_authenticated":
        return "That key was refused";
      case "rate_limited":
        return "The provider says you've hit a limit or are out of credit";
      case "process_failed":
        return "Couldn't reach the provider";
      case "cancelled":
        return "The test was stopped";
      default:
        return "Something went wrong";
    }
  }
  if (provider === "local-model") {
    switch (kind) {
      case "not_installed":
        return "No model is set up yet";
      case "not_authenticated":
        return "The model server refused the request";
      case "rate_limited":
        return "The model is busy right now";
      case "process_failed":
        return "Couldn't reach the model";
      case "cancelled":
        return "The test was stopped";
      default:
        return "Something went wrong";
    }
  }
  switch (kind) {
    case "not_installed":
      return "It isn't installed on this computer";
    case "not_authenticated":
      return "You're not signed in";
    case "rate_limited":
      return "You've reached your plan's limit for now";
    case "process_failed":
      return "It didn't start properly";
    case "cancelled":
      return "The test was stopped";
    default:
      return "Something went wrong";
  }
}

/** "0.8 s" / "12 s" — a real measured duration, never an estimate. */
export function formatDuration(ms: number): string {
  const seconds = ms / 1000;
  return seconds < 10 ? `${seconds.toFixed(1)} s` : `${Math.round(seconds)} s`;
}

/** What the terminal's finished run means, in plain words. */
export function runOutcome(
  action: ConnectAction,
  name: string,
  success: boolean,
  code: number | null,
): string {
  if (success) {
    return action === "install" ? `${name} finished installing.` : `The sign-in finished.`;
  }
  // The installer said it was fine, but the backend's check afterwards
  // didn't find the CLI (a `curl … | sh` whose download failed still exits 0).
  if (action === "install" && code === 0) {
    return `The installer finished, but ${name} still isn't on this computer. Its output above says what went wrong.`;
  }
  const what = action === "install" ? "The installer" : "The sign-in";
  return code === null ? `${what} stopped before it finished.` : `${what} stopped with an error (code ${code}).`;
}

/** Ready to use: installed, and not known to be signed out. `null`
 * ("couldn't tell") still counts — the "Say hello" test is how to find out
 * for sure. */
export function isUsable(p: ProviderInfo): boolean {
  return p.installed && p.logged_in !== false;
}
