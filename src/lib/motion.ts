import { cubicOut } from "svelte/easing";
import type { TransitionConfig } from "svelte/transition";

// Shared motion language: short durations, ease-out entrances, opacity and
// transform only (no layout-affecting properties).
//
// These mirror the --motion-* tokens on .root in +page.svelte; keep the two
// in sync. CSS-side transitions (hover states) read the tokens;
// svelte/transition params use these constants.

export const MOTION_FAST_MS = 120;
export const MOTION_MED_MS = 180;
export const MOTION_SLOW_MS = 280;

/** True when the OS asks for reduced motion. Read fresh on every call. */
export function prefersReducedMotion(): boolean {
  return (
    typeof window !== "undefined" &&
    typeof window.matchMedia === "function" &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}

/** Zeroes a transition duration when the user prefers reduced motion. */
export function motionMs(ms: number): number {
  return prefersReducedMotion() ? 0 : ms;
}

export interface FadeUnlessParams {
  duration?: number;
  /**
   * Read when the transition starts; true completes it instantly. Pass a
   * closure over the state that was current at render to detect a swap.
   */
  skip?: () => boolean;
}

/**
 * Opacity fade that can bail out at start. Used where an outro must not
 * hold layout — e.g. tab-strip buttons, whose outro would otherwise overlap
 * the incoming workspace's buttons and shove the row sideways before
 * snapping back. Honors reduced motion (instant).
 */
export function fadeUnless(
  _node: Element,
  { duration = MOTION_FAST_MS, skip }: FadeUnlessParams = {},
): TransitionConfig {
  const instant = prefersReducedMotion() || skip?.() === true;
  return {
    duration: instant ? 0 : duration,
    css: (t) => `opacity: ${t};`,
  };
}

export interface RiseParams {
  duration?: number;
  /** Vertical offset (px) the panel rises from. */
  y?: number;
  /** Starting scale; 1 disables scaling. */
  scaleFrom?: number;
}

/**
 * Dialog/panel entrance: fade with a slight rise and settle. Runs symmetric
 * on exit through the same transition. Instant under reduced motion.
 */
export function rise(
  _node: Element,
  { duration = MOTION_MED_MS, y = 6, scaleFrom = 0.98 }: RiseParams = {},
): TransitionConfig {
  return {
    duration: motionMs(duration),
    easing: cubicOut,
    css: (t) => {
      const dy = (1 - t) * y;
      const s = scaleFrom + (1 - scaleFrom) * t;
      return `opacity: ${t}; transform: translateY(${dy}px) scale(${s});`;
    },
  };
}
