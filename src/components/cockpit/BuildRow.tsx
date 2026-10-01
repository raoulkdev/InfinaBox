import { useCallback, useRef, useState } from "react";
import { FileBrowser } from "@/components/cockpit/FileBrowser";
import { TerminalPanel } from "@/components/cockpit/TerminalPanel";
import { getLayout, setLayout } from "@/lib/layout-store";

interface BuildRowProps {
  projectPath: string | null;
  showTerminal: boolean;
}

const SPLIT_KEY = "build.split";
const MIN_PERCENT = 20;

/** The Code page: the project's files and editor (`FileBrowser`'s
 * `codeEditor` mode: a file list beside a CodeMirror editor) with the
 * terminal under them. The divider between the two is dragged and the
 * position is saved in the app settings. */
export function BuildRow({ projectPath, showTerminal }: BuildRowProps) {
  const [percent, setPercent] = useState(() => getLayout<number>(SPLIT_KEY) ?? 62);
  const box = useRef<HTMLDivElement | null>(null);

  const startDrag = useCallback((e: React.PointerEvent) => {
    e.preventDefault();
    window.getSelection()?.removeAllRanges();
    document.documentElement.classList.add("is-dragging");
    const move = (ev: PointerEvent) => {
      const r = box.current?.getBoundingClientRect();
      if (!r || r.height === 0) return;
      const next = Math.min(100 - MIN_PERCENT, Math.max(MIN_PERCENT, ((ev.clientY - r.top) / r.height) * 100));
      setPercent(next);
      setLayout(SPLIT_KEY, next);
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", up);
      document.documentElement.classList.remove("is-dragging");
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", up);
  }, []);

  return (
    <div ref={box} className="flex h-full min-h-0 w-full min-w-0 flex-1 flex-col">
      <div className="min-h-0 overflow-hidden" style={{ flex: `0 0 calc(${percent}% - 4px)` }}>
        <FileBrowser label="Code" rootPath={projectPath} variant="list" codeEditor />
      </div>
      <div
        role="separator"
        aria-orientation="horizontal"
        data-testid="code-split-handle"
        onPointerDown={startDrag}
        className="h-2 shrink-0 cursor-row-resize touch-none"
      />
      {/* TerminalPanel spawns its shell once, on mount, using whatever
          projectPath it was given at that instant (see its own comment on
          why it never re-cwds later) — so it must not mount at all until a
          real project is chosen, or it'd spawn in the user's home directory
          instead. */}
      <div className="min-h-0 flex-1 overflow-hidden">{showTerminal ? <TerminalPanel projectPath={projectPath} /> : null}</div>
    </div>
  );
}
