import { FileBrowser } from "@/components/cockpit/FileBrowser";

interface ReleaseSectionProps {
  projectPath: string | null;
}

// Changelog drafts, release checklists — same hidden-dotfolder-docs convention as Design (see DesignSection.tsx). Real build/release tooling (engine + CLI detection, ship-state dashboard) is a later addition; this is just the docs half for now.
const DOCS_DIR = ".ibproject/release";

export function ReleaseSection({ projectPath }: ReleaseSectionProps) {
  const rootPath = projectPath ? `${projectPath}/${DOCS_DIR}` : null;
  return <FileBrowser label="Release" rootPath={rootPath} forceMdExtension variant="list" />;
}
