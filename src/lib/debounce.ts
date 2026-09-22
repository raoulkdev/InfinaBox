// A leading+trailing debounce for resize-driven work: an ancestor's CSS/JS
// transition (the Sidebar's width animation, in particular) fires a
// `ResizeObserver` on every intermediate frame, not just once at the end —
// see the callers of this in TerminalPanel and ResizablePanelGroup for what
// that costs. Plain debounce (trailing-only) would make the very first
// resize signal feel delayed; plain throttle would still do the expensive
// work many times over one transition. Leading+trailing gives exactly two
// calls per burst — one immediately (so a one-off resize, e.g. switching
// to a tab for the first time, still applies instantly) and one once
// movement actually settles (so the final, correct size is never skipped) —
// regardless of how many resize signals land in between.
export function debounce<Args extends unknown[]>(
  fn: (...args: Args) => void,
  delay: number,
  options: { leading?: boolean } = {},
): { (...args: Args): void; cancel: () => void } {
  const { leading = false } = options;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let trailingPending = false;
  let lastArgs: Args;

  function debounced(...args: Args): void {
    lastArgs = args;
    const isFirstInBurst = timer === null;
    if (timer !== null) clearTimeout(timer);
    if (leading && isFirstInBurst) {
      fn(...args);
      trailingPending = false;
    } else {
      trailingPending = true;
    }
    timer = setTimeout(() => {
      timer = null;
      if (trailingPending) {
        trailingPending = false;
        fn(...lastArgs);
      }
    }, delay);
  }

  debounced.cancel = () => {
    if (timer !== null) clearTimeout(timer);
    timer = null;
    trailingPending = false;
  };

  return debounced;
}
