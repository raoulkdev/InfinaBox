import { useContextMenu } from "@/lib/context-menu";
import { useCallback, useEffect, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { Package } from "lucide-react";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { assetsHealth, assetsScan } from "@/lib/studio-api";
import { fadeTransition } from "@/lib/motion";
import type { AssetInfo, HealthReport } from "@/lib/studio-types";
import { errorText } from "./assets-format";
import { AssetPreview } from "./AssetPreview";
import { GenerateTab } from "./GenerateTab";
import { LibraryTab } from "./LibraryTab";
import { ProjectTab } from "./ProjectTab";

// The Assets section (product spec §7.3): the game's pictures, sounds, 3D
// models and fonts, free libraries and the person's own files to add more,
// and generation with their own provider accounts. Every license is kept
// with the file so the credits stay honest. The three tabs stay mounted
// (stacked, like Context's) so a half-filled form survives a look at
// another tab.

export interface AssetsSectionProps {
  projectPath: string | null;
}

type AssetsTab = "project" | "library" | "generate";

const TABS: { id: AssetsTab; label: string }[] = [
  { id: "project", label: "Project" },
  { id: "library", label: "Library" },
  { id: "generate", label: "Generate" },
];

export function AssetsSection({ projectPath }: AssetsSectionProps) {
  const [tab, setTab] = useState<AssetsTab>("project");
  const [assets, setAssets] = useState<AssetInfo[] | null>(null);
  const [health, setHealth] = useState<HealthReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [previewPath, setPreviewPath] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!projectPath) return;
    setLoading(true);
    const [scan, report] = await Promise.allSettled([assetsScan(projectPath), assetsHealth(projectPath)]);
    if (scan.status === "fulfilled") {
      setAssets(scan.value);
      setError(null);
    } else {
      setAssets((cur) => cur ?? []);
      setError(errorText(scan.reason));
    }
    // A failed health check just hides the strip; the scan error above already says what's wrong.
    setHealth(report.status === "fulfilled" ? report.value : null);
    setLoading(false);
  }, [projectPath]);

  useEffect(() => {
    setAssets(null);
    setHealth(null);
    setError(null);
    setNotice(null);
    setPreviewPath(null);
    void refresh();
  }, [refresh]);

  if (!projectPath) {
    return (
      <div className="flex h-full min-h-0 flex-1 flex-col items-center justify-center gap-2 rounded-xl border border-border bg-card text-center">
        <div className="flex size-10 items-center justify-center rounded-lg border border-border">
          <Package className="size-5 text-muted-foreground" />
        </div>
        <h2 className="text-lg font-medium tracking-tight">No project open</h2>
      </div>
    );
  }

  const menu = useContextMenu();
  const previewAsset = previewPath ? (assets?.find((a) => a.path === previewPath) ?? null) : null;

  return (
    <Tabs
      value={tab}
      onValueChange={(value) => setTab(value as AssetsTab)}
      data-testid="assets-section"
      onContextMenu={(e) =>
        menu(e, [
          ...TABS.map((t) => ({ label: t.label, disabled: t.id === tab, onSelect: () => setTab(t.id) })),
          "separator",
          { label: "Refresh assets", disabled: !projectPath || loading, onSelect: () => void refresh() },
        ])
      }
      className="flex h-full min-h-0 min-w-0 flex-1 flex-col gap-2"
    >
      <div
        data-tauri-drag-region
        className="flex shrink-0 items-center gap-3 rounded-xl border border-border bg-card px-3 py-2"
      >
        <div data-tauri-drag-region className="flex min-w-0 flex-1 flex-col">
          <span className="text-xs font-medium tracking-wide text-muted-foreground">Assets</span>
        </div>
        <TabsList>
          {TABS.map((t) => (
            <TabsTrigger key={t.id} value={t.id} className="px-3">
              {t.label}
            </TabsTrigger>
          ))}
        </TabsList>
      </div>

      <div className="relative min-h-0 flex-1 overflow-hidden rounded-xl border border-border bg-card">
        {TABS.map((t) => (
          <TabsContent key={t.id} value={t.id} forceMount asChild>
            <motion.div
              className="absolute inset-0 flex min-h-0 min-w-0 flex-col"
              animate={{ opacity: tab === t.id ? 1 : 0 }}
              style={{ pointerEvents: tab === t.id ? "auto" : "none" }}
              transition={fadeTransition}
              inert={tab !== t.id}
            >
              {t.id === "project" && (
                <ProjectTab
                  projectPath={projectPath}
                  assets={assets}
                  health={health}
                  loading={loading}
                  error={error}
                  notice={notice}
                  onRefresh={() => void refresh()}
                  onImported={(a) => {
                    setNotice(`Added ${a.path} to your game.`);
                    void refresh();
                  }}
                  onOpenAsset={(a) => setPreviewPath(a.path)}
                  onGoTo={setTab}
                />
              )}
              {t.id === "library" && (
                <LibraryTab
                  projectPath={projectPath}
                  onAdded={(added) => {
                    if (added.length > 0) setNotice(`Added ${added.map((a) => a.path).join(", ")} from the Library.`);
                    void refresh();
                  }}
                />
              )}
              {t.id === "generate" && (
                <GenerateTab
                  projectPath={projectPath}
                  onAccepted={(a) => {
                    setNotice(`Added ${a.path} (generated).`);
                    void refresh();
                  }}
                />
              )}
            </motion.div>
          </TabsContent>
        ))}

        <AnimatePresence>
          {previewAsset && tab === "project" && (
            <AssetPreview
              key={previewAsset.path}
              projectPath={projectPath}
              asset={previewAsset}
              onClose={() => setPreviewPath(null)}
            />
          )}
        </AnimatePresence>
      </div>
    </Tabs>
  );
}
