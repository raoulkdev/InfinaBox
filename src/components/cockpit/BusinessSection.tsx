import { FileBrowser } from "@/components/cockpit/FileBrowser";

interface BusinessSectionProps {
  projectPath: string | null;
}

// Store-page copy, pricing notes, publishing checklists — same
// hidden-dotfolder-docs convention as Design (see DesignSection.tsx).
const DOCS_DIR = ".ibproject/business";

export function BusinessSection({ projectPath }: BusinessSectionProps) {
  const rootPath = projectPath ? `${projectPath}/${DOCS_DIR}` : null;
  return <FileBrowser label="Business" rootPath={rootPath} forceMdExtension variant="list" />;
}
