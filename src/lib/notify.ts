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

/** The only notification chime; there is no picker for it. */
export const FIXED_CHIME_FILE = "/Users/nemoryoliver/Downloads/notification.mp3";

/** One selection contract for preview, test, and real notification playback. */
export function playbackPayload(kind: SoundKind) {
  return {
    kind,
    file: FIXED_CHIME_FILE,
  };
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

/** Authorization state from the backend `notification_permission` command. */
export type NotifyPermission = "granted" | "denied" | "prompt" | "unknown";

export function parseNotifyPermission(value: unknown): NotifyPermission {
  return value === "granted" ||
    value === "denied" ||
    value === "prompt" ||
    value === "unknown"
    ? value
    : "unknown";
}

/** Delivery report from the backend `notify_agent` command. */
export type NotifyOutcomeStatus =
  | "delivered"
  | "denied"
  | "attempted"
  | "unavailable";
export interface NotifyOutcome {
  status: NotifyOutcomeStatus;
  reason?: string;
}

export function parseNotifyOutcome(value: unknown): NotifyOutcome {
  if (typeof value !== "object" || value === null) {
    return { status: "unavailable", reason: "bad-outcome" };
  }
  const record = value as { status?: unknown; reason?: unknown };
  if (
    record.status === "delivered" ||
    record.status === "denied" ||
    record.status === "attempted"
  ) {
    return { status: record.status };
  }
  if (record.status === "unavailable") {
    return {
      status: "unavailable",
      reason: typeof record.reason === "string" ? record.reason : "unknown",
    };
  }
  return { status: "unavailable", reason: "bad-outcome" };
}

/**
 * One actionable reading of an outcome. `message` is the exact line to show
 * (a success note or an error); null means stay silent. The Settings Test
 * button reports everything inline, while the real agent-finish path only
 * surfaces actionable states — transient failures stay in the console so a
 * flaky backend can't toast-spam every finish.
 */
export interface NotifyReport {
  ok: boolean;
  message: string | null;
}

export function describeNotifyOutcome(
  outcome: NotifyOutcome,
  context: "test" | "agent",
): NotifyReport {
  switch (outcome.status) {
    case "delivered":
      return {
        ok: true,
        message: context === "test" ? "System notification sent." : null,
      };
    case "attempted":
      return {
        ok: true,
        message:
          context === "test"
            ? "Sent, but this build can't confirm delivery. Verify in the packaged app."
            : null,
      };
    case "denied":
      return {
        ok: false,
        message:
          "System notifications are blocked. Enable them in System Settings → Notifications, then try again.",
      };
    case "unavailable":
      if (outcome.reason === "not-determined") {
        return {
          ok: false,
          message:
            context === "test"
              ? "Permission isn't decided yet — answer the system prompt, then press Test again."
              : "Notification permission isn't decided yet — open Settings → Alerts and press Test.",
        };
      }
      if (context === "agent") return { ok: false, message: null };
      return {
        ok: false,
        message: `Couldn't show the system notification (${outcome.reason ?? "unknown"}).`,
      };
  }
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
