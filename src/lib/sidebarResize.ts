export const DEFAULT_SIDEBAR_WIDTH = 190;
export const DEFAULT_RIGHT_PANEL_WIDTH = 250;
export const MIN_SIDEBAR_WIDTH = 140;
export const MAX_SIDEBAR_WIDTH = 400;
export const SIDEBAR_WIDTH_KEY_STEP = 10;

export const DEFAULT_SPLIT_RATIO = 0.5;
export const MIN_SPLIT_PANE_PX = 80;
export const SPLIT_RATIO_KEY_STEP = 0.05;

export function clampSidebarWidth(
  value: number,
  fallback: number = DEFAULT_SIDEBAR_WIDTH,
): number {
  if (!Number.isFinite(value)) return fallback;
  return Math.min(
    MAX_SIDEBAR_WIDTH,
    Math.max(MIN_SIDEBAR_WIDTH, Math.round(value)),
  );
}

export function stepSidebarWidth(current: number, delta: 1 | -1): number {
  return clampSidebarWidth(
    clampSidebarWidth(current) + delta * SIDEBAR_WIDTH_KEY_STEP,
  );
}

export function clampSplitRatio(value: number): number {
  if (!Number.isFinite(value)) return DEFAULT_SPLIT_RATIO;
  return Math.round(Math.min(1, Math.max(0, value)) * 10000) / 10000;
}

export function stepSplitRatio(current: number, delta: 1 | -1): number {
  return clampSplitRatio(
    clampSplitRatio(current) + delta * SPLIT_RATIO_KEY_STEP,
  );
}

/** Fraction of pointerY between top and top+height; default when degenerate. */
export function ratioFromPointer(
  pointerY: number,
  top: number,
  height: number,
): number {
  if (!(height > 0)) return DEFAULT_SPLIT_RATIO;
  return (pointerY - top) / height;
}

/**
 * Constrain a split ratio so each pane keeps at least minPanePx.
 * Falls back to the default when the container cannot fit the minimums.
 */
export function effectiveSplitRatio(
  ratio: number,
  containerPx: number,
  minPanePx: number = MIN_SPLIT_PANE_PX,
): number {
  const clamped = clampSplitRatio(ratio);
  if (!(containerPx > 0) || !(minPanePx > 0)) return clamped;
  const lo = minPanePx / containerPx;
  const hi = 1 - lo;
  if (lo > hi) return DEFAULT_SPLIT_RATIO;
  return Math.min(hi, Math.max(lo, clamped));
}
