import { useEffect, useRef } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import { debounce } from "@/lib/debounce";
import { connectResize, connectRun, connectWrite, onConnectExit, onConnectOutput } from "@/lib/studio-api";
import type { ConnectAction, ConnectExitPayload, ProviderId } from "@/lib/studio-types";

// Same settle delay as TerminalPanel's — see its comment on
// `RESIZE_SETTLE_MS` for why a burst of ResizeObserver firings is collapsed.
const RESIZE_SETTLE_MS = 120;

interface ConnectTerminalProps {
  provider: ProviderId;
  action: ConnectAction;
  /** The real `connect-exit` event for this run. */
  onExit: (exit: ConnectExitPayload) => void;
  /** `connect_run` itself was refused (e.g. another run is going). */
  onStartError: (message: string) => void;
}

/** A visible terminal running a provider's own installer or sign-in (the
 * backend runs it in a real PTY — spec §8.2's "the user watches it
 * happen"). Set up like TerminalPanel's xterm (same theme, fit and
 * debounced resize), except the backend process is started exactly once per
 * mount and ends by itself: keyboard input goes to it (a sign-in may ask to
 * press Enter), and its end arrives as `connect-exit`. */
export function ConnectTerminal({ provider, action, onExit, onStartError }: ConnectTerminalProps) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  // Survives StrictMode's mount→cleanup→mount (refs are kept across that
  // cycle), so the installer is never started twice for one click.
  const startedRef = useRef(false);
  // Latest callbacks, read from inside the mount-only effect.
  const onExitRef = useRef(onExit);
  const onStartErrorRef = useRef(onStartError);
  onExitRef.current = onExit;
  onStartErrorRef.current = onStartError;

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;

    const terminal = new Terminal({
      cursorBlink: true,
      convertEol: false,
      fontFamily:
        "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, 'Liberation Mono', 'Courier New', monospace",
      fontSize: 12,
      theme: {
        // TerminalPanel's palette, but on --background: this terminal sits
        // inside a card, so it reads as an inset box.
        background: "#0f0f0f",
        foreground: "#F2F2F2",
        cursor: "#F2F2F2",
        cursorAccent: "#0f0f0f",
        selectionBackground: "#2a2a2a",
      },
    });
    const fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);
    terminal.open(host);

    const fit = () => {
      try {
        fitAddon.fit();
        return true;
      } catch {
        // Zero-sized for a moment during layout; the next resize fits.
        return false;
      }
    };

    const unlistenOutput = onConnectOutput(({ data }) => terminal.write(data));
    const unlistenExit = onConnectExit((exit) => {
      if (exit.provider === provider && exit.action === action) onExitRef.current(exit);
    });
    const dataDisposable = terminal.onData((data) => {
      void connectWrite(data).catch(() => {
        // The process may have just ended; keystrokes after that go nowhere.
      });
    });

    fit();
    if (!startedRef.current) {
      startedRef.current = true;
      terminal.focus();
      connectRun(provider, action, terminal.rows, terminal.cols).catch((err: unknown) => {
        onStartErrorRef.current(err instanceof Error ? err.message : String(err));
      });
    }

    const debouncedFit = debounce(
      () => {
        if (fit()) void connectResize(terminal.rows, terminal.cols).catch(() => {});
      },
      RESIZE_SETTLE_MS,
      { leading: true },
    );
    const resizeObserver = new ResizeObserver(() => debouncedFit());
    resizeObserver.observe(host);

    return () => {
      debouncedFit.cancel();
      resizeObserver.disconnect();
      dataDisposable.dispose();
      unlistenOutput();
      unlistenExit();
      terminal.dispose();
    };
    // Mount-only: one terminal per run; a new run remounts with a new key.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div
      data-testid="connect-terminal"
      className="h-56 overflow-hidden rounded-lg border border-border bg-[#0f0f0f] p-2"
    >
      <div ref={hostRef} className="h-full w-full" />
    </div>
  );
}
