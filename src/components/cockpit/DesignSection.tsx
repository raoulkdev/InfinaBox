import { FileBrowser } from "@/components/cockpit/FileBrowser";

interface DesignSectionProps {
  projectPath: string | null;
}

// GDD docs (and whatever else the user wants to organize alongside them —
// a "gdd" folder, "lore", "characters", anything) live inside a hidden
// dotfolder. `list_directory`'s general dotfile filter (see
// src-tauri/src/commands/fs.rs) keeps `.ibproject` out of the Files
// panel's browser entirely, while this section browses it directly by
// path, unaffected by that filter (it only applies to entries *discovered*
// while listing a directory, not to a path given directly).
const DOCS_DIR = ".ibproject/docs";

/** The Design tab used to hardcode a flat `docs/gdd/*.md` listing. It's
 * now just a `FileBrowser` rooted at `.ibproject/docs` — full folder
 * navigation, create/delete included, so the user organizes their own docs
 * however they want instead of being locked into one fixed structure.
 * Displayed as "Documents" — this is the one central place for project
 * docs, not GDD-specific (internal naming stays `Design`/`.ibproject/docs`
 * for continuity, only the user-facing label changed). */
export function DesignSection({ projectPath }: DesignSectionProps) {
  const rootPath = projectPath ? `${projectPath}/${DOCS_DIR}` : null;
  return <FileBrowser label="Documents" rootPath={rootPath} forceMdExtension variant="list" />;
}
