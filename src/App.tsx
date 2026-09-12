import { useState } from "react";
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

  return (
    <div className="flex h-screen w-screen flex-col gap-2 bg-background p-2 text-foreground">
      <TopBar />
      <div className="flex min-h-0 flex-1 gap-2">
        <FileBrowserPanel
          selectedPath={selectedPath}
          onSelectFile={(entry: FileEntry) => setSelectedPath(entry.path)}
        />
        <ViewportPanel selectedPath={selectedPath} />
        <AgentPanel />
      </div>
      <ConsoleDock />
    </div>
  );
}

export default App;
