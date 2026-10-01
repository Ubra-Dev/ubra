/** First-run fleet: pure planning for the multi-CLI launch screen. */

import type { DetectedCli } from "./agentClis";

/**
 * Max panes in a first-run fleet tab. Three CLIs stay readable at the
 * default window size; a fourth would shrink every terminal past comfort.
 */
export const FLEET_MAX_PANES = 3;

/**
 * How long to wait for a fleet CLI to boot before flagging that pane for
 * attention instead of sending the starter prompt.
 */
export const FLEET_PROMPT_TIMEOUT_MS = 30_000;

/** localStorage flag so the first-fleet celebration fires once ever. */
export const FLEET_CELEBRATED_KEY = "ubra.fleetCelebrated";

/**
 * Settle delay between observing a booted fleet CLI and sending the
 * starter prompt, so the keystrokes land in a ready prompt line.
 */
export const FLEET_PROMPT_SETTLE_MS = 1500;

export interface FleetStarter {
  id: string;
  title: string;
  blurb: string;
  prompt: string;
}

/**
 * One-click first tasks for the fleet. Every prompt is read-only by
 * construction ("don't change anything") so a first run can impress
 * without ever mutating the user's project.
 */
export const FLEET_STARTERS: FleetStarter[] = [
  {
    id: "explain",
    title: "Explain this codebase",
    blurb: "A guided tour: what it does and where to start",
    prompt:
      "Explain this codebase: what it does, how it's organized, and where a newcomer should start. Be concise. Don't change anything.",
  },
  {
    id: "review",
    title: "Review uncommitted changes",
    blurb: "Flag anything risky before you commit",
    prompt:
      "Review my uncommitted changes with git status and git diff. Flag anything risky, sloppy, or worth a second look. Don't change anything.",
  },
  {
    id: "bugs",
    title: "Hunt for bugs",
    blurb: "Up to 3 real issues, ranked by severity",
    prompt:
      "Find up to 3 real bugs or sharp edges in this code. For each: the file, what's wrong, and a suggested fix. Don't change anything.",
  },
];

export const DEFAULT_FLEET_STARTER_ID = "explain";

export function fleetStarterById(id: string): FleetStarter {
  return FLEET_STARTERS.find((starter) => starter.id === id) ?? FLEET_STARTERS[0];
}

/**
 * Agent commands for the fleet tab, in pane order (primary CLI first).
 * Blank picks are dropped, duplicates collapse, the list caps at
 * FLEET_MAX_PANES, and a custom command fills an otherwise empty fleet.
 */
export function planFleet(
  selected: readonly string[],
  customCommand: string | null | undefined,
): string[] {
  const commands = selected.map((entry) => entry.trim()).filter(Boolean);
  const custom = customCommand?.trim() ?? "";
  if (commands.length === 0 && custom) commands.push(custom);
  return [...new Set(commands)].slice(0, FLEET_MAX_PANES);
}

/** Stems of the detected entries, in detection order, for fleet planning. */
export function detectedStems(entries: readonly DetectedCli[]): string[] {
  return entries.map((entry) => entry.cli);
}
