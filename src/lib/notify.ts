/**
 * Notification + sound routing, Herdr-style.
 *
 * Herdr's `[ui.toast] delivery` is a single channel (`off | herdr | terminal
 * | system`); Ubra mirrors that minus `terminal` (no outer terminal exists).
 * Sounds are an independent channel like Herdr's `[ui.sound]`.
 */

export type NotifyDelivery = "off" | "inapp" | "system";
export type ToastPosition =
  | "top-left"
  | "top-right"
  | "bottom-left"
  | "bottom-right";
export type SoundKind = "done" | "request";

export const DEFAULT_DELIVERY: NotifyDelivery = "system";
export const DEFAULT_TOAST_POSITION: ToastPosition = "bottom-right";

export function parseDelivery(value: unknown): NotifyDelivery {
  return value === "off" || value === "inapp" || value === "system"
    ? value
    : DEFAULT_DELIVERY;
}

export function parseToastPosition(value: unknown): ToastPosition {
  return value === "top-left" ||
    value === "top-right" ||
    value === "bottom-left" ||
    value === "bottom-right"
    ? value
    : DEFAULT_TOAST_POSITION;
}

export interface NotifyRoute {
  toast: boolean;
  system: boolean;
  sound: boolean;
}

/**
 * Route one finished-agent event. Delivery picks at most one visual channel;
 * sound is independent (plays even with delivery `off`, like Herdr) unless
 * disabled or the agent cli is muted. Matching is case-insensitive.
 */
export function routeNotification(opts: {
  delivery: NotifyDelivery;
  soundEnabled: boolean;
  mutedClis: string[];
  cli?: string;
}): NotifyRoute {
  const muted = (opts.cli ?? "").toLowerCase();
  const sound =
    opts.soundEnabled &&
    !(muted !== "" && opts.mutedClis.some((m) => m.toLowerCase() === muted));
  return {
    toast: opts.delivery === "inapp",
    system: opts.delivery === "system",
    sound,
  };
}
