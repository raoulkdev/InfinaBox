import { TopBar } from "@/components/cockpit/TopBar";
import { FileBrowserPanel } from "@/components/cockpit/FileBrowserPanel";
import { ViewportPanel } from "@/components/cockpit/ViewportPanel";
import { AgentPanel } from "@/components/cockpit/AgentPanel";
import { ConsoleDock } from "@/components/cockpit/ConsoleDock";

function App() {
  return (
    <div className="flex h-screen w-screen flex-col gap-2 bg-background p-2 text-foreground">
      <TopBar />
      <div className="flex min-h-0 flex-1 gap-2">
        <FileBrowserPanel />
        <ViewportPanel />
        <AgentPanel />
      </div>
      <ConsoleDock />
    </div>
  );
}

export default App;
