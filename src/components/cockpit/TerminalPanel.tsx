import { useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import { debounce } from "@/lib/debounce";

// How long to wait, after the host's size stops changing, before trusting
// it enough to fit()+resize the backend PTY again. Ancestor animations
// (the Sidebar's width transition in particular) fire this observer on
// every intermediate frame of the transition, not just once at the end —
// without this, one sidebar toggle turns into a real IPC round-trip
// (`resize_terminal`) plus an xterm re-layout on every single one of those
// frames. See `debounce`'s own comment for why leading+trailing.
const RESIZE_SETTLE_MS = 120;

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
        // Matches --card in src/index.css — the same card background every
        // other block renders on, so the terminal reads as one of them
        // rather than a visually distinct, darker box.
        background: "#161616",
        foreground: "#F2F2F2",
        cursor: "#F2F2F2",
        cursorAccent: "#161616",
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

    // Leading call so a genuine one-off resize (switching to this tab,
    // resizing the window once) still fits instantly; trailing call so the
    // final settled size is never skipped once a burst of intermediate
    // firings (an animated ancestor, a live divider drag) stops. Without
    // this, every one of those intermediate firings would do a real fit()
    // plus a `resize_terminal` IPC round-trip on its own — see the module
    // comment on `RESIZE_SETTLE_MS`.
    const debouncedFit = debounce(fitAndResizeBackend, RESIZE_SETTLE_MS, { leading: true });
    const resizeObserver = new ResizeObserver(() => {
      debouncedFit();
    });
    resizeObserver.observe(host);

    return () => {
      debouncedFit.cancel();
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
    <div className="flex h-full min-w-0 flex-1 flex-col overflow-hidden rounded-xl border border-border bg-card">
      <div data-tauri-drag-region className="flex h-9 shrink-0 items-center px-3">
        <span className="text-xs font-medium tracking-wide text-muted-foreground">Agent</span>
      </div>
      <div className="min-h-0 flex-1 overflow-hidden bg-card p-2">
        <div ref={hostRef} className="h-full w-full" />
      </div>
    </div>
  );
}
