import { FileBrowser } from "@/components/cockpit/FileBrowser";

interface MarketingSectionProps {
  projectPath: string | null;
}

// Campaign briefs, press-kit drafts — same hidden-dotfolder-docs convention as Design (see DesignSection.tsx).
const DOCS_DIR = ".ibproject/marketing";

export function MarketingSection({ projectPath }: MarketingSectionProps) {
  const rootPath = projectPath ? `${projectPath}/${DOCS_DIR}` : null;
  return <FileBrowser label="Marketing" rootPath={rootPath} forceMdExtension variant="list" />;
}
