import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

/** Resolves to the chosen folder, or `null` if the user cancelled — a
 * normal outcome, not an error. Used both to pick a folder to open as a
 * project and, from the "New Project" dialog, to pick where a new one
 * should be created — neither of those cases wants project validation, so
 * it stays a plain folder picker. */
export async function pickProjectFolder(): Promise<string | null> {
  const folder = await open({ directory: true, multiple: false });
  return typeof folder === "string" ? folder : null;
}

export function projectFolderName(path: string): string {
  return path.split("/").filter(Boolean).pop() ?? path;
}

interface ProjectMetadata {
  version: number;
  name: string;
  createdAt: string;
}

function metadataPath(projectPath: string): string {
  return `${projectPath}/.ibproject/.ibx`;
}

/** Written once, at project creation, into the `.ibproject` folder that
 * "New Project" already creates. Its only real job is to exist — reading
 * it back is how `isInfinaBoxProject` tells a real InfinaBox project apart
 * from an arbitrary folder someone picked in the "Open Project" dialog. */
export async function writeProjectMetadata(projectPath: string, name: string): Promise<void> {
  const metadata: ProjectMetadata = { version: 1, name, createdAt: new Date().toISOString() };
  await invoke("write_file", { path: metadataPath(projectPath), contents: JSON.stringify(metadata, null, 2) });
}

/** A folder only counts as an InfinaBox project if it has this file —
 * unreadable (missing, permissions, whatever) just means "no", not an
 * error worth surfacing differently. */
export async function isInfinaBoxProject(path: string): Promise<boolean> {
  try {
    await invoke<string>("read_file", { path: metadataPath(path) });
    return true;
  } catch {
    return false;
  }
}

export type OpenProjectResult =
  | { status: "opened"; path: string }
  | { status: "invalid"; path: string }
  | { status: "cancelled" };

/** The shared "Open Project" flow for the Sidebar and the Home dashboard:
 * pick a folder, then refuse it unless it's a real InfinaBox project — the
 * whole point of `.ibx` is that InfinaBox can't be pointed at a random
 * folder and treated as a project. */
export async function pickAndOpenExistingProject(): Promise<OpenProjectResult> {
  const folder = await pickProjectFolder();
  if (!folder) return { status: "cancelled" };
  const valid = await isInfinaBoxProject(folder);
  return { status: valid ? "opened" : "invalid", path: folder };
}
