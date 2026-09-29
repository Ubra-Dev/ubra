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

/** Built-in chime ids (backend ChimeStyle) plus the custom-file pseudo-id. */
export const CHIME_STYLE_IDS = ["default", "bright", "soft", "pop"] as const;
export type ChimeStyleId = (typeof CHIME_STYLE_IDS)[number];
export const CUSTOM_CHIME_ID = "custom";
export const DEFAULT_CHIME_STYLE = "default";

/** Persisted chime selection: a built-in id, "custom", or "default". */
export function parseChimeStyle(value: unknown): string {
  if (value === CUSTOM_CHIME_ID) return CUSTOM_CHIME_ID;
  return (CHIME_STYLE_IDS as readonly string[]).includes(value as string)
    ? (value as string)
    : DEFAULT_CHIME_STYLE;
}

/**
 * Style param for play_sound: built-ins pass through, "custom" becomes null
 * so the custom file wins, and anything unexpected becomes "default".
 * Never sends "custom" itself: the backend enum would reject it.
 */
export function chimeStyleParam(style: string): string | null {
  if (style === CUSTOM_CHIME_ID) return null;
  return (CHIME_STYLE_IDS as readonly string[]).includes(style)
    ? style
    : DEFAULT_CHIME_STYLE;
}

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

/** Sample payload for the Settings "send test" button. */
export interface TestNotification {
  title: string;
  body: string;
  /** Sample agent cli, so muting demonstrates silence. */
  cli: string;
  kind: SoundKind;
  /** Empty: clicking the test toast dismisses without jumping anywhere. */
  nodeId: "";
}

/**
 * The Settings test button fires this through the live delivery, sound, and
 * mute settings, exactly like a real agent finish.
 */
export function testNotificationPayload(): TestNotification {
  return {
    title: "Codex finished (test)",
    body: "This is what an agent finish looks like",
    cli: "codex",
    kind: "done",
    nodeId: "",
  };
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
