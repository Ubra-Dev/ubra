import { invoke } from "@tauri-apps/api/core";

/** Rust-owned telemetry state. `supported` is false in builds without a key. */
export interface TelemetryStatus {
  supported: boolean;
  consented: boolean;
  active: boolean;
}

export async function telemetryStatus(): Promise<TelemetryStatus> {
  return invoke<TelemetryStatus>("telemetry_status");
}

/** Persist consent Rust-side; returns whether capture is now active. */
export async function setTelemetryConsent(consented: boolean): Promise<boolean> {
  return invoke<boolean>("telemetry_set_consent", { consented });
}

/** Push the posthog-js distinct id so Rust exceptions correlate. */
export async function setDistinctId(id: string): Promise<void> {
  await invoke("telemetry_set_distinct_id", { id });
}

/** Evaluate a Rust-side feature flag (fails open when unavailable). */
export async function telemetryFlag(name: string): Promise<boolean> {
  return invoke<boolean>("telemetry_flag", { name });
}

export type AgentEndOutcome = "completed" | "stopped" | "closed";
export type DurationBucket = "under_minute" | "under_5m" | "under_30m" | "over_30m";

/** Coarsen session lengths so durations can't fingerprint users. */
export function bucketizeDuration(ms: number): DurationBucket {
  if (Number.isNaN(ms) || ms < 60_000) return "under_minute";
  if (ms < 5 * 60_000) return "under_5m";
  if (ms < 30 * 60_000) return "under_30m";
  return "over_30m";
}

/**
 * Capture an agent session start. The Rust side allowlists the event,
 * validates `cli` against the known agent table, and honors the
 * agent-events kill switch. Returns true when queued.
 */
export async function captureAgentStarted(cli: string): Promise<boolean> {
  return invoke<boolean>("telemetry_capture", {
    event: "agent_cli_started",
    properties: { cli },
  });
}

export async function captureAgentEnded(
  cli: string,
  durationMs: number,
  outcome: AgentEndOutcome,
): Promise<boolean> {
  return invoke<boolean>("telemetry_capture", {
    event: "agent_cli_ended",
    properties: { cli, duration: bucketizeDuration(durationMs), outcome },
  });
}
