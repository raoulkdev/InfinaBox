// Recently opened projects, for the Home dashboard's project list. There's
// no backend concept of "known projects" (the Rust side only ever sees one
// `projectPath` at a time) — this is purely a frontend convenience, so
// localStorage is enough: it's per-machine, per-user data with no need to
// round-trip through Tauri, and it survives app restarts the same way a
// real "recent files" list would.

export interface RecentProject {
  path: string;
  lastOpened: number;
}

const STORAGE_KEY = "infinabox.recentProjects";

function readAll(): RecentProject[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(
      (entry): entry is RecentProject =>
        typeof entry === "object" &&
        entry !== null &&
        typeof (entry as RecentProject).path === "string" &&
        typeof (entry as RecentProject).lastOpened === "number",
    );
  } catch {
    // Corrupted JSON, or localStorage unavailable (private browsing, quota,
    // disabled site data) — an empty recents list is a safe fallback either
    // way, not something worth surfacing as an error.
    return [];
  }
}

function writeAll(projects: RecentProject[]): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(projects));
  } catch {
    // See readAll — losing the recents list isn't worth an error UI.
  }
}

/** Newest-first. */
export function loadRecentProjects(): RecentProject[] {
  return readAll().sort((a, b) => b.lastOpened - a.lastOpened);
}

export function recordProjectOpened(path: string): RecentProject[] {
  const rest = readAll().filter((p) => p.path !== path);
  const updated = [...rest, { path, lastOpened: Date.now() }];
  writeAll(updated);
  return loadRecentProjects();
}

export function removeRecentProject(path: string): RecentProject[] {
  const updated = readAll().filter((p) => p.path !== path);
  writeAll(updated);
  return loadRecentProjects();
}
