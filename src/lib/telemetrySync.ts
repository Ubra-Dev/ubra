import posthog from "posthog-js";
import { setDistinctId, setTelemetryConsent, telemetryStatus } from "./telemetry";

/**
 * Align the posthog-js client with Rust-owned consent. Every SDK call is
 * guarded: telemetry must never break boot (missing env, blocked network,
 * uninitialized client).
 */
export async function syncTelemetryToConsent(): Promise<void> {
  let status;
  try {
    status = await telemetryStatus();
  } catch (error) {
    console.error("ubra: telemetry status failed", error);
    return;
  }
  if (!status.supported) return;
  if (status.consented) {
    safeOptIn();
    pushDistinctId();
  } else {
    safeOptOut();
  }
}

/**
 * Persist a consent change and mirror it into the JS client. Throws when
 * the Rust write fails so callers can revert their UI; the JS mirroring
 * itself never throws.
 */
export async function applyTelemetryConsent(granted: boolean): Promise<void> {
  try {
    await setTelemetryConsent(granted);
  } catch (error) {
    console.error("ubra: telemetry consent failed", error);
    throw error;
  }
  if (granted) {
    safeOptIn();
    pushDistinctId();
  } else {
    safeOptOut();
  }
}

function safeOptIn(): void {
  try {
    posthog.opt_in_capturing();
  } catch (error) {
    console.error("ubra: posthog opt-in failed", error);
  }
}

function safeOptOut(): void {
  try {
    posthog.opt_out_capturing();
  } catch (error) {
    console.error("ubra: posthog opt-out failed", error);
  }
}

function pushDistinctId(): void {
  let id: string | undefined;
  try {
    id = posthog.get_distinct_id() ?? undefined;
  } catch {
    return;
  }
  if (!id) return;
  setDistinctId(id).catch((error: unknown) => {
    console.error("ubra: distinct id sync failed", error);
  });
}
