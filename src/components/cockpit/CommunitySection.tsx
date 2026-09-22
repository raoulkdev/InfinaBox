import { FileBrowser } from "@/components/cockpit/FileBrowser";

interface CommunitySectionProps {
  projectPath: string | null;
}

// Community guidelines, event-planning notes — same hidden-dotfolder-docs convention as Design (see DesignSection.tsx).
const DOCS_DIR = ".ibproject/community";

export function CommunitySection({ projectPath }: CommunitySectionProps) {
  const rootPath = projectPath ? `${projectPath}/${DOCS_DIR}` : null;
  return <FileBrowser label="Community" rootPath={rootPath} forceMdExtension variant="list" />;
}
