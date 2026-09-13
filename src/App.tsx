import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { TopBar } from "@/components/cockpit/TopBar";
import { ProjectWindow } from "@/components/cockpit/ProjectWindow";
import { TerminalPanel } from "@/components/cockpit/TerminalPanel";

function App() {
  // The currently open project folder — shared by ProjectWindow (all three
  // of its tabs) and the terminal's starting directory. Seeded from
  // get_default_project_path on mount so the app still opens showing the
  // familiar test project by default; from then on it's real, user-driven
  // state set via the TopBar's "Open Project" picker.
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

  return (
    <div className="flex h-screen w-screen flex-col gap-2 bg-background p-2 text-foreground">
      <TopBar projectPath={projectPath} onOpenProject={setProjectPath} />
      <div className="flex min-h-0 flex-1 gap-2">
        <ProjectWindow projectPath={projectPath} />
        <TerminalPanel projectPath={projectPath} />
      </div>
    </div>
  );
}

export default App;
