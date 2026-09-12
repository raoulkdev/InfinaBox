import { FileText, ShieldCheck, Megaphone, Radio } from "lucide-react";
import type { Section } from "@/components/cockpit/TopBar";
import { DesignSection } from "@/components/cockpit/DesignSection";
import { QaSection } from "@/components/cockpit/QaSection";
import { BusinessSection } from "@/components/cockpit/BusinessSection";
import { LiveOpsSection } from "@/components/cockpit/LiveOpsSection";

const SECTION_META: Record<Section, { label: string; icon: typeof FileText }> = {
  design: { label: "Design", icon: FileText },
  qa: { label: "QA", icon: ShieldCheck },
  business: { label: "Business", icon: Megaphone },
  liveops: { label: "Live Ops", icon: Radio },
};

interface SectionOverlayProps {
  section: Section;
  projectPath: string | null;
}

/** Whatever's active in the "Project" menu — always sits in the same slot
 * Build normally occupies, never both at once. File browser, agent panel,
 * and console stay visible regardless of which section is showing. */
export function SectionOverlay({ section, projectPath }: SectionOverlayProps) {
  const meta = SECTION_META[section];

  return (
    <div className="flex h-full min-w-0 flex-1 flex-col border border-border bg-card">
      <div className="flex h-8 shrink-0 items-center gap-2 border-b border-border px-3">
        <meta.icon className="size-3.5 text-muted-foreground" />
        <span className="text-xs font-medium tracking-wide text-muted-foreground uppercase">
          {meta.label}
        </span>
      </div>
      {section === "design" && <DesignSection projectPath={projectPath} />}
      {section === "qa" && <QaSection />}
      {section === "business" && <BusinessSection />}
      {section === "liveops" && <LiveOpsSection />}
    </div>
  );
}
