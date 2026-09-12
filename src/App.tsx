import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { TopBar } from "@/components/cockpit/TopBar";
import { FileBrowserPanel } from "@/components/cockpit/FileBrowserPanel";
import { ViewportPanel } from "@/components/cockpit/ViewportPanel";
import { AgentPanel } from "@/components/cockpit/AgentPanel";
import { ConsoleDock } from "@/components/cockpit/ConsoleDock";
import type { FileEntry } from "@/types/fs";

function App() {
  // Lifted here (rather than context) because exactly two panels need it
  // and the app has no state library — see FileTree -> FileBrowserPanel
  // (sets it on file click) and ViewportPanel (reads it to load/edit).
  const [selectedPath, setSelectedPath] = useState<string | null>(null);

  // The currently open project folder — shared by FileBrowserPanel and
  // AgentPanel (and shown in TopBar). Seeded from get_default_project_path
  // on mount so the app still opens showing the familiar test project by
  // default; from then on it's real, user-driven state set via the
  // TopBar's "Open Project" folder picker.
  const [projectPath, setProjectPath] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    async function loadDefaultProject() {
      const defaultPath = await invoke<string>("get_default_project_path");
      if (!cancelled) {
        setProjectPath(defaultPath);
      }
    }

    void loadDefaultProject();

    return () => {
      cancelled = true;
    };
  }, []);

  function openProject(path: string) {
    setProjectPath(path);
    // A selected file belongs to the previous project — showing it against
    // a new project's file tree would be wrong, so drop it.
    setSelectedPath(null);
  }

  return (
    <div className="flex h-screen w-screen flex-col gap-2 bg-background p-2 text-foreground">
      <TopBar projectPath={projectPath} onOpenProject={openProject} />
      <div className="flex min-h-0 flex-1 gap-2">
        <FileBrowserPanel
          projectPath={projectPath}
          selectedPath={selectedPath}
          onSelectFile={(entry: FileEntry) => setSelectedPath(entry.path)}
        />
        <ViewportPanel selectedPath={selectedPath} />
        <AgentPanel projectPath={projectPath} />
      </div>
      <ConsoleDock />
    </div>
  );
}

export default App;
