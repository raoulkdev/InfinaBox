import { FileBrowser } from "@/components/cockpit/FileBrowser";

interface GraphsSectionProps {
  projectPath: string | null;
}

// Graph-based documents (flowcharts, dependency maps, relationship
// diagrams) — same hidden-dotfolder-docs convention as Documents (see
// DesignSection.tsx), but rooted at its own folder and opening `.graph.json`
// files in the ReactFlow-backed GraphEditor instead of the markdown editor.
const GRAPHS_DIR = ".ibproject/graphs";

export function GraphsSection({ projectPath }: GraphsSectionProps) {
  const rootPath = projectPath ? `${projectPath}/${GRAPHS_DIR}` : null;
  return <FileBrowser label="Graphs" rootPath={rootPath} graphExtension variant="list" />;
}
