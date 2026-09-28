import { FileBrowser } from "@/components/cockpit/FileBrowser";

interface GraphsSectionProps {
  projectPath: string | null;
}

// Graph-based documents (story and dialogue flows, level connections,
// state machines) — Context's Graphs tab (see ContextSection.tsx). Same
// hidden-dotfolder convention as Context's cards, but rooted at its own
// folder and opening `.graph.json` files in the ReactFlow-backed
// GraphEditor instead of the markdown editor.
const GRAPHS_DIR = ".ibproject/graphs";

export function GraphsSection({ projectPath }: GraphsSectionProps) {
  const rootPath = projectPath ? `${projectPath}/${GRAPHS_DIR}` : null;
  return <FileBrowser label="Graphs" rootPath={rootPath} graphExtension variant="list" />;
}
