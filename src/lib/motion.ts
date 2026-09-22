import type { Transition } from "motion/react";

// One shared spring for anything that moves (width, position, a sliding
// highlight) — snappy enough to feel immediate, damped enough not to
// wobble. Reused everywhere instead of tuning stiffness/damping per
// component, so different parts of the app don't end up subtly
// disagreeing about how "fast" a transition feels.
export const springTransition: Transition = { type: "spring", stiffness: 420, damping: 38, mass: 0.9 };

// For content that's appearing/disappearing (a fade, a fade+rise) rather
// than moving — a plain duration reads cleaner than a spring for opacity.
export const fadeTransition: Transition = { duration: 0.16, ease: "easeOut" };

// For animating a layout-affecting property — `width`/`height`, not
// `transform`/`opacity` — where the browser has to synchronously recompute
// layout on every tick, unlike a compositor-only property. A spring's
// physics-driven ticks don't run on a fixed frame budget and can wobble
// past the target before settling, which means more of those expensive
// ticks than a plain tween needs, for longer, on top of anything downstream
// that reacts to the resize (see the Sidebar's width animation and its
// `ResizeObserver`-driven neighbors, which is exactly the case this exists
// for). A short, fixed-duration tween is both cheaper and more predictable
// here: it's the one deliberate exception to `springTransition` being the
// app's default "anything that moves" transition.
export const widthTransition: Transition = { duration: 0.22, ease: [0.4, 0, 0.2, 1] };

// The one animation most of the app's section/panel swaps use: fade in
// while rising slightly from below, fade out the same way in reverse.
export const fadeRise = {
  initial: { opacity: 0, y: 6 },
  animate: { opacity: 1, y: 0 },
  exit: { opacity: 0, y: -6 },
};
