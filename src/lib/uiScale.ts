export const DEFAULT_UI_SCALE = 100;
export const MIN_UI_SCALE = 75;
export const MAX_UI_SCALE = 150;
export const UI_SCALE_STEP = 10;

export function clampUiScale(value: number): number {
  if (!Number.isFinite(value)) return DEFAULT_UI_SCALE;
  return Math.min(
    MAX_UI_SCALE,
    Math.max(MIN_UI_SCALE, Math.round(value)),
  );
}

export function stepUiScale(current: number, delta: 1 | -1): number {
  return clampUiScale(clampUiScale(current) + delta * UI_SCALE_STEP);
}

export function uiZoomStyle(scale: number): string {
  return `zoom:${clampUiScale(scale)}%`;
}
