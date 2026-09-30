/**
 * Plan-usage types mirrored from the `cli_usage` command, plus formatting
 * helpers.
 *
 * This module is deliberately pure: no Tauri imports, so unit tests can
 * import it under plain node. Live fetching lives in `./usage.svelte`.
 */

/** Backend `UsageStatus`, serialized camelCase. */
export type UsageStatus =
  | "ready"
  | "notLoggedIn"
  | "expired"
  | "offline"
  | "unsupported"
  | "parseError";

export interface UsageWindow {
  label: string;
  percentUsed?: number;
  resetsAt?: number;
}

export interface UsageSnapshot {
  cli: string;
  source: string;
  plan?: string;
  windows: UsageWindow[];
  /** Capture time as unix epoch seconds. */
  fetchedAt: number;
}

export interface CliUsage {
  cli: string;
  status: UsageStatus;
  snapshot?: UsageSnapshot;
  message?: string;
}

/** "in 2h 14m" style countdown for a reset epoch (seconds). */
export function formatResetCountdown(
  resetsAt: number,
  nowSec: number = Math.floor(Date.now() / 1000),
): string {
  const diff = Math.floor(resetsAt - nowSec);
  if (diff <= 0) return "resetting…";
  const hours = Math.floor(diff / 3600);
  const minutes = Math.floor((diff % 3600) / 60);
  if (hours >= 48) return `in ${Math.floor(hours / 24)}d ${hours % 24}h`;
  if (hours > 0) return `in ${hours}h ${minutes}m`;
  if (minutes > 0) return `in ${minutes}m`;
  return `in ${diff}s`;
}

/** "just now" / "5m ago" / "2h ago" for a fetch epoch (seconds). */
export function formatUpdatedAgo(
  fetchedAt: number,
  nowSec: number = Math.floor(Date.now() / 1000),
): string {
  const diff = Math.max(0, Math.floor(nowSec - fetchedAt));
  if (diff < 60) return "just now";
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  return `${Math.floor(diff / 3600)}h ago`;
}
