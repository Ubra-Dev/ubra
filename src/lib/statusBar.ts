// Status bar labels: pure module (no Svelte/Tauri imports) so formatting can
// be unit-tested with node:test. StatusBar.svelte renders these strings.
import { countPanes, type Layout } from "./layout.ts";
import type { NotifyDelivery } from "./notify.ts";

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

/** "2 workspaces · 3 tabs · 9 panes"; empty without a layout. */
export function layoutTotalsLabel(layout: Layout | null): string {
  if (!layout) return "";
  const plural = (n: number, one: string, many: string): string =>
    `${n} ${n === 1 ? one : many}`;
  const workspaces = layout.workspaces.length;
  let tabs = 0;
  let panes = 0;
  for (const ws of layout.workspaces) {
    tabs += ws.tabs.length;
    for (const tab of ws.tabs) panes += countPanes(tab.root);
  }
  return (
    `${plural(workspaces, "workspace", "workspaces")} · ` +
    `${plural(tabs, "tab", "tabs")} · ` +
    `${plural(panes, "pane", "panes")}`
  );
}

/** Display OS for a navigator.platform value; null when unrecognized. */
export function platformLabel(platform: string): string | null {
  const value = platform.toLowerCase();
  if (value.includes("mac")) return "macOS";
  if (value.includes("win")) return "Windows";
  if (value.includes("linux")) return "Linux";
  return null;
}

export function deliveryLabel(delivery: NotifyDelivery): string {
  return delivery === "off" ? "Off" : delivery === "inapp" ? "In-app" : "System";
}

export function soundLabel(enabled: boolean): string {
  return enabled ? "On" : "Off";
}
