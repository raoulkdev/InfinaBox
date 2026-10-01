import type { Transition } from "motion/react";

// The app has no decorative motion: panels, sections and the sidebar change
// instantly. These stay as the single place components get their transition
// from, all of them instant, so one change here would bring motion back.
const instant: Transition = { duration: 0 };

export const springTransition: Transition = instant;
export const fadeTransition: Transition = instant;
export const widthTransition: Transition = instant;

/** Section swaps: no fade or rise. */
export const fadeRise = {
  initial: { opacity: 1, y: 0 },
  animate: { opacity: 1, y: 0 },
  exit: { opacity: 1, y: 0 },
};
