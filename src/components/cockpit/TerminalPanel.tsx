import { useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";

interface TerminalPanelProps {
  projectPath: string | null;
}

// A real embedded terminal: xterm.js in the browser, backed by a real PTY
// shell process on the Rust side (see src-tauri/src/commands/terminal.rs).
// The point is that the user's own `claude`/`codex`/etc CLI — already
// authenticated on their machine — just works here, because this is a real
// shell, not something InfinaBox has to broker.
export function TerminalPanel({ projectPath }: TerminalPanelProps) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);

  // Spawn is keyed on mount only, intentionally — projectPath changing
  // later (a different project opened) must NOT respawn or re-cwd an
  // already-running session, which would kill whatever the user is doing
  // in their shell. We read projectPath's mount-time value via a ref-like
  // capture below rather than putting it in the dependency array.
  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;

    const terminal = new Terminal({
      cursorBlink: true,
      fontFamily:
        "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, 'Liberation Mono', 'Courier New', monospace",
      fontSize: 13,
      theme: {
        background: "#0A0A0A",
        foreground: "#F2F2F2",
        cursor: "#F2F2F2",
        cursorAccent: "#0A0A0A",
        selectionBackground: "#1D1D1D",
      },
    });
    const fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);
    terminal.open(host);
    terminalRef.current = terminal;
    fitAddonRef.current = fitAddon;

    const fitAndResizeBackend = () => {
      try {
        fitAddon.fit();
      } catch {
        // The host can be zero-sized for a moment during layout/unmount
        // churn; fit() throws in that case, but there's nothing useful to
        // do — just skip this pass, the next resize will fit fine.
        return;
      }
      void invoke("resize_terminal", { rows: terminal.rows, cols: terminal.cols });
    };

    // Initial fit + spawn. The spawn command is idempotent on the backend
    // (safe under React 18/19 StrictMode's double-invoke of effects), so
    // there's no need to guard against calling it twice here.
    fitAndResizeBackend();
    void invoke("spawn_terminal", { cwd: projectPath ?? undefined }).then(() => {
      // Re-fit after spawn in case the host's size settled differently
      // once content is present, and make sure the backend PTY's size
      // matches whatever we ended up with.
      fitAndResizeBackend();
    });

    const dataDisposable = terminal.onData((data) => {
      void invoke("write_to_terminal", { data });
    });

    let unlisten: (() => void) | undefined;
    void listen<string>("terminal-output", (event) => {
      terminal.write(event.payload);
    }).then((fn) => {
      unlisten = fn;
    });

    const resizeObserver = new ResizeObserver(() => {
      fitAndResizeBackend();
    });
    resizeObserver.observe(host);

    return () => {
      resizeObserver.disconnect();
      dataDisposable.dispose();
      unlisten?.();
      terminal.dispose();
      terminalRef.current = null;
      fitAddonRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- spawn is keyed on mount only; projectPath changing later must not respawn/re-cwd the running session.
  }, []);

  return (
    <div className="flex h-full min-w-0 flex-1 flex-col border border-border bg-card">
      <div className="flex h-8 shrink-0 items-center border-b border-border px-3">
        <span className="text-xs font-medium tracking-wide text-muted-foreground uppercase">
          Terminal
        </span>
      </div>
      <div className="min-h-0 flex-1 overflow-hidden p-1">
        <div ref={hostRef} className="h-full w-full" />
      </div>
    </div>
  );
}
