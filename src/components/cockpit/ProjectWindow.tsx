import { useState } from "react";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { OverviewTab } from "@/components/cockpit/OverviewTab";
import { ChangesTab } from "@/components/cockpit/ChangesTab";
import { FilesTab } from "@/components/cockpit/FilesTab";

interface ProjectWindowProps {
  projectPath: string | null;
}

export function ProjectWindow({ projectPath }: ProjectWindowProps) {
  const [tab, setTab] = useState("overview");

  return (
    <div className="flex h-full min-w-0 flex-1 flex-col border border-border bg-card">
      <Tabs
        value={tab}
        onValueChange={setTab}
        className="flex min-h-0 flex-1 flex-col gap-0"
      >
        <TabsList
          variant="line"
          className="h-8 w-full shrink-0 justify-start rounded-none border-b border-border px-2"
        >
          <TabsTrigger value="overview">Overview</TabsTrigger>
          <TabsTrigger value="changes">Changes</TabsTrigger>
          <TabsTrigger value="files">Files</TabsTrigger>
        </TabsList>
        <TabsContent value="overview" className="flex min-h-0 flex-1 flex-col">
          <OverviewTab projectPath={projectPath} />
        </TabsContent>
        <TabsContent value="changes" className="flex min-h-0 flex-1 flex-col">
          <ChangesTab projectPath={projectPath} />
        </TabsContent>
        <TabsContent value="files" className="flex min-h-0 flex-1">
          <FilesTab projectPath={projectPath} />
        </TabsContent>
      </Tabs>
    </div>
  );
}
