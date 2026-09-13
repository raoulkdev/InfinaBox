import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { TopBar, type Section } from "@/components/cockpit/TopBar";
import { FileBrowserPanel } from "@/components/cockpit/FileBrowserPanel";
import { ViewportPanel } from "@/components/cockpit/ViewportPanel";
import { SectionOverlay } from "@/components/cockpit/SectionOverlay";
import { TerminalPanel } from "@/components/cockpit/TerminalPanel";
import { ConsoleDock } from "@/components/cockpit/ConsoleDock";

function App() {
  // null = Build (the persistent, default view). Any other value is a
  // Project-menu section overlaid on top of it — Design, QA, Business, or
  // Live Ops — matching the original design: Build is always underneath,
  // sections are what you switch via the "Project" dropdown.
  const [activeSection, setActiveSection] = useState<Section | null>(null);

  // The currently open project folder — shared by FileBrowserPanel, the
  // Design section, and the terminal's starting directory (shown in
  // TopBar). Seeded from get_default_project_path on mount so the app
  // still opens showing the familiar test project by default; from then on
  // it's real, user-driven state set via the TopBar's project picker.
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
    setActiveSection(null);
  }

  return (
    <div className="flex h-screen w-screen flex-col gap-2 bg-background p-2 text-foreground">
      <TopBar
        projectPath={projectPath}
        onOpenProject={openProject}
        activeSection={activeSection}
        onSelectSection={setActiveSection}
      />
      <div className="flex min-h-0 flex-1 gap-2">
        <FileBrowserPanel projectPath={projectPath} />
        {activeSection ? (
          <SectionOverlay section={activeSection} projectPath={projectPath} />
        ) : (
          <ViewportPanel />
        )}
        <TerminalPanel projectPath={projectPath} />
      </div>
      <ConsoleDock />
    </div>
  );
}

export default App;
