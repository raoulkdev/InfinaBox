import { FileBrowser } from "@/components/cockpit/FileBrowser";
import { ResizablePanelGroup } from "@/components/cockpit/ResizablePanelGroup";
import { TerminalPanel } from "@/components/cockpit/TerminalPanel";

interface BuildRowProps {
  projectPath: string | null;
  showTerminal: boolean;
}

/** Agent / Code, laid out via the shared `ResizablePanelGroup` system —
 * resizable against each other and dragged into either order, persisted
 * under the "build" storage key. Overview/Changes (git branch, commits,
 * diffs) is removed from the Build tab for now — see the project
 * discussion; `ProjectWindow`/`OverviewTab`/`ChangesTab` still exist for
 * whenever it comes back, just unused here. The Files panel and its
 * Finder-style grid are replaced by the same "mini VS Code" window
 * (`FileBrowser`'s `codeEditor` mode): a list-style file browser paired
 * with a CodeMirror editor that opens anything text-based. */
export function BuildRow({ projectPath, showTerminal }: BuildRowProps) {
  return (
    <ResizablePanelGroup
      storageKey="build"
      panels={[
        {
          id: "agent",
          // Agent starts a little narrower than the code window — the
          // terminal is a single-purpose panel, while the mini VS Code
          // window carries both a file list and an editor, and wants the
          // room.
          defaultPercent: 40,
          minPercent: 20,
          // TerminalPanel spawns its shell once, on mount, using whatever
          // projectPath it was given at that instant (see its own comment
          // on why it never re-cwds later) — so it must not mount at all
          // until a real project is chosen, or it'd spawn in the user's
          // home directory instead.
          content: showTerminal ? <TerminalPanel projectPath={projectPath} /> : null,
        },
        {
          id: "code",
          defaultPercent: 60,
          minPercent: 20,
          content: <FileBrowser label="Code" rootPath={projectPath} variant="list" codeEditor />,
        },
      ]}
    />
  );
}
