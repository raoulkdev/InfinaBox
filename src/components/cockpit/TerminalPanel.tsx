interface TerminalPanelProps {
  projectPath: string | null;
}

// TODO: real embedded terminal (xterm.js + a Rust-side PTY via
// portable-pty), so the user can run their own Claude Code / Codex / etc
// CLI directly against the open project. Should split evenly (flex-1) with
// whatever's in the Build/Section slot next to it.
export function TerminalPanel({ projectPath }: TerminalPanelProps) {
  void projectPath;
  return (
    <div className="flex h-full min-w-0 flex-1 flex-col border border-border bg-card">
      <div className="flex h-8 shrink-0 items-center border-b border-border px-3">
        <span className="text-xs font-medium tracking-wide text-muted-foreground uppercase">
          Terminal
        </span>
      </div>
      <div className="flex flex-1 items-center justify-center">
        <p className="text-sm text-muted-foreground">Terminal — coming next.</p>
      </div>
    </div>
  );
}
