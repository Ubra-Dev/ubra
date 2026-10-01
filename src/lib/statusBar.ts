// Status bar labels: pure module (no Svelte/Tauri imports) so formatting can
// be unit-tested with node:test. StatusBar.svelte renders these strings.
import type { Rollup } from "./agentStatus.ts";
import type { NotifyDelivery } from "./notify.ts";
import type { UpdatePhase } from "./updater.svelte.ts";
import { formatResetCountdown, type CliUsage } from "./usage.ts";

export interface FleetCounts {
  working: number;
  blocked: number;
  /** Unread agent finishes + attention requests across all workspaces. */
  review: number;
}

/** "2 working · 1 blocked · 3 need review"; null when idle and unreviewed. */
export function fleetSummary(counts: FleetCounts): string | null {
  const parts: string[] = [];
  if (counts.working > 0) parts.push(`${counts.working} working`);
  if (counts.blocked > 0) parts.push(`${counts.blocked} blocked`);
  if (counts.review > 0) {
    parts.push(counts.review === 1 ? "1 needs review" : `${counts.review} need review`);
  }
  return parts.length > 0 ? parts.join(" · ") : null;
}

export type SaveState = "saved" | "saving" | "error";

/** A failed save keeps reporting until the next save succeeds. */
export function saveState(opts: { saving: boolean; error: string | null }): SaveState {
  if (opts.error) return "error";
  return opts.saving ? "saving" : "saved";
}

export type UpdateAction = "check" | "download" | "relaunch";

export interface UpdateSegment {
  text: string;
  title: string;
  action: UpdateAction | null;
}

/**
 * Download percent capped below 100 (100 only counts once installed);
 * null when the total is unknown.
 */
export function downloadPercent(
  downloadedBytes: number,
  totalBytes: number | null,
): number | null {
  if (!totalBytes || totalBytes <= 0) return null;
  return Math.min(99, Math.floor((downloadedBytes / totalBytes) * 100));
}

/** Plain-language update failure; falls back to the raw backend message. */
export function friendlyUpdateError(raw: string | null): string {
  const detail = (raw ?? "").trim();
  if (/404|not found/i.test(detail)) {
    return "Couldn't reach the update server (not found).";
  }
  if (
    /network|fetch failed|connection|connrefused|econn|enotfound|etimedout|timed?\s?out|offline|dns/i.test(
      detail,
    )
  ) {
    return "Couldn't reach the update server. Check your connection and retry.";
  }
  if (/sign|verif|tamper|integrity/i.test(detail)) {
    return "The update couldn't be verified and was blocked for safety.";
  }
  return detail || "Couldn't check for updates.";
}

export interface SidebarUpdateState {
  icon: "download" | "refresh" | "alert";
  /** Accessible name and tooltip. */
  label: string;
  /** Render the button only while an update is actionable. */
  visible: boolean;
  /** Spinner replaces the icon. */
  busy: boolean;
  /** Download percent 0-99; null when unknown or not downloading. */
  progress: number | null;
}

export interface SidebarUpdateInput {
  phase: UpdatePhase;
  checked: boolean;
  version: string | null;
  downloadedBytes: number;
  totalBytes: number | null;
  error: string | null;
}

/**
 * Sidebar update button for the updater phase. The button only renders while
 * an update is actionable (available, downloading, ready). It opens
 * Settings → App; one-click install/restart lives in Settings and the
 * status bar.
 */
export function sidebarUpdateState(opts: SidebarUpdateInput): SidebarUpdateState {
  switch (opts.phase) {
    case "idle":
      return opts.checked
        ? {
            icon: "download",
            label: "You're up to date — open Updates",
            visible: false,
            busy: false,
            progress: null,
          }
        : {
            icon: "download",
            label: "Check for updates",
            visible: false,
            busy: false,
            progress: null,
          };
    case "checking":
      return {
        icon: "download",
        label: "Checking for updates…",
        visible: false,
        busy: true,
        progress: null,
      };
    case "available":
      return {
        icon: "download",
        label: opts.version
          ? `Ubra ${opts.version} available — open Updates to install`
          : "Update available — open Updates to install",
        visible: true,
        busy: false,
        progress: null,
      };
    case "downloading": {
      const pct = downloadPercent(opts.downloadedBytes, opts.totalBytes);
      return {
        icon: "download",
        label:
          pct === null
            ? "Downloading update…"
            : `Downloading update ${pct}% — open Updates for details`,
        visible: true,
        busy: true,
        progress: pct,
      };
    }
    case "ready":
      return {
        icon: "refresh",
        label: "Update installed — open Updates to restart",
        visible: true,
        busy: false,
        progress: null,
      };
    case "error":
      return {
        icon: "alert",
        label: `${friendlyUpdateError(opts.error)} — open Updates to retry`,
        visible: false,
        busy: false,
        progress: null,
      };
  }
}

/**
 * Update segment for the updater phase; null when there is nothing to show
 * (up to date after a check). Checks run in the background; the bar surfaces
 * results and one-click actions.
 */
export function updateSegment(opts: {
  phase: UpdatePhase;
  checked: boolean;
  version: string | null;
  downloadedBytes: number;
  totalBytes: number | null;
  error: string | null;
}): UpdateSegment | null {
  switch (opts.phase) {
    case "idle":
      if (opts.checked) return null;
      return {
        text: "Check for updates",
        title: "Never checked — click to check for updates",
        action: "check",
      };
    case "checking":
      return {
        text: "Checking for updates…",
        title: "Checking for updates…",
        action: null,
      };
    case "available":
      return {
        text: opts.version ? `Update to ${opts.version}` : "Update available",
        title: `Version ${opts.version ?? "unknown"} available — click to download and install`,
        action: "download",
      };
    case "downloading": {
      const pct = downloadPercent(opts.downloadedBytes, opts.totalBytes);
      return {
        text: pct === null ? "Downloading update…" : `Downloading update ${pct}%`,
        title: "Downloading update…",
        action: null,
      };
    }
    case "ready":
      return {
        text: "Restart to update",
        title: "Update installed — click to restart",
        action: "relaunch",
      };
    case "error":
      return {
        text: "Update failed",
        title: `${opts.error ?? "Update failed"} — click to retry`,
        action: "check",
      };
  }
}

/** Show the usage segment once any plan window reaches this percent. */
export const USAGE_WARN_THRESHOLD = 80;

export interface UsageWarning {
  text: string;
  title: string;
}

/**
 * Worst-first plan-usage warning across ready entries; null when every
 * known window is below the threshold. Unknown statuses and windows
 * without a percent never warn.
 */
export function usageWarning(
  entries: Record<string, CliUsage>,
  labels: Record<string, string>,
  nowSec: number = Math.floor(Date.now() / 1000),
): UsageWarning | null {
  const hits: { cli: string; percent: number; resetsAt?: number }[] = [];
  for (const entry of Object.values(entries)) {
    if (entry.status !== "ready" || !entry.snapshot) continue;
    let best: { percent: number; resetsAt?: number } | null = null;
    for (const window of entry.snapshot.windows) {
      if (window.percentUsed === undefined) continue;
      if (!best || window.percentUsed > best.percent) {
        best = { percent: window.percentUsed, resetsAt: window.resetsAt };
      }
    }
    if (best && best.percent >= USAGE_WARN_THRESHOLD) {
      hits.push({ cli: entry.cli, ...best });
    }
  }
  if (hits.length === 0) return null;
  hits.sort((a, b) => b.percent - a.percent);
  const [worst] = hits;
  const label = labels[worst.cli] ?? worst.cli;
  const text =
    hits.length === 1 ? `${label} ${worst.percent}%` : `${label} ${worst.percent}% +${hits.length - 1}`;
  const parts = hits.map((hit) => {
    const name = labels[hit.cli] ?? hit.cli;
    const reset =
      hit.resetsAt === undefined ? "" : ` (resets ${formatResetCountdown(hit.resetsAt, nowSec)})`;
    return `${name} ${hit.percent}%${reset}`;
  });
  return { text, title: `${parts.join(" · ")} — open Usage` };
}

/** Dot class for the focused pane's agent rollup; null hides the dot. */
export function crumbDotClass(
  status: Rollup,
  hasAgent: boolean,
): "blocked" | "attention" | "working" | null {
  if (!hasAgent) return null;
  if (status === "blocked") return "blocked";
  if (status === "attention") return "attention";
  if (status === "working") return "working";
  return null;
}

/** Branch segment tooltip; null changes means the status is unknown. */
export function branchTooltip(opts: { branch: string; changes: number | null }): string {
  const state =
    opts.changes === null
      ? "status unknown"
      : opts.changes === 0
        ? "clean"
        : opts.changes === 1
          ? "1 change"
          : `${opts.changes} changes`;
  return `${opts.branch} · ${state} — open Source Control`;
}

export function deliveryLabel(delivery: NotifyDelivery): string {
  return delivery === "off" ? "Off" : delivery === "inapp" ? "In-app" : "System";
}

export function soundLabel(enabled: boolean): string {
  return enabled ? "On" : "Off";
}

/** Status-bar notification button tooltip; the button toggles sound. */
export function notifyTooltip(opts: {
  delivery: NotifyDelivery;
  soundEnabled: boolean;
  mutedCount: number;
}): string {
  return (
    `Notifications: ${deliveryLabel(opts.delivery)} · ` +
    `Sound ${soundLabel(opts.soundEnabled)}` +
    (opts.mutedCount > 0
      ? ` · ${opts.mutedCount} muted agent${opts.mutedCount === 1 ? "" : "s"}`
      : "") +
    " — click to toggle sound"
  );
}
