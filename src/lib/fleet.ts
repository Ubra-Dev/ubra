/** First-run fleet: pure planning for the multi-CLI launch screen. */

import type { DetectedCli } from "./agentClis";
import { leavesByAreaDesc, newId, tilingForCount, type Tab } from "./layout.ts";

/**
 * Initial fleet picks: the preferred CLI first, then detection order.
 * There is no launch cap; users can pick as many agents as they like.
 */
export const FLEET_DEFAULT_PICKS = 4;

/**
 * Agent commands for the fleet tab, in pane order (primary CLI first).
 * Blank picks are dropped, duplicates collapse, and a custom command
 * fills an otherwise empty fleet.
 */
export function planFleet(
  selected: readonly string[],
  customCommand: string | null | undefined,
): string[] {
  const commands = selected.map((entry) => entry.trim()).filter(Boolean);
  const custom = customCommand?.trim() ?? "";
  if (commands.length === 0 && custom) commands.push(custom);
  return [...new Set(commands)];
}

/** Stems of the detected entries, in detection order, for fleet planning. */
export function detectedStems(entries: readonly DetectedCli[]): string[] {
  return entries.map((entry) => entry.cli);
}

export interface FleetLaunchPlan {
  tab: Tab;
  /** Pane ids largest-area-first; `commands[i]` lands on `order[i]`. */
  order: string[];
}

/**
 * Launch plan for `count` panes: the canonical tiling shared with the
 * workspace layout menu, so the preview always matches the launch.
 */
export function planFleetLayout(count: number): FleetLaunchPlan {
  const tab: Tab = { id: newId("tab"), name: "Tab 1", root: tilingForCount(count) };
  return { tab, order: leavesByAreaDesc(tab.root) };
}
