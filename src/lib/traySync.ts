import type { WorkspaceAgents } from "./agent.svelte.ts";
import type { Rollup } from "./agentStatus.ts";
import { trailingDebouncer, type TimerClock } from "./schedule.ts";

/** Rows per menu; the backend clamps higher as a backstop. */
export const TRAY_MAX_ROWS = 15;
export const TRAY_SYNC_DEBOUNCE_MS = 300;

export interface TrayAgentRow {
  nodeId: string;
  agent: string;
  context: string;
  status: Rollup;
}

/** Wire shape of the `tray_update` command; field names are camelCase. */
export interface TrayPayload {
  title: string | null;
  tooltip: string;
  header: string;
  agents: TrayAgentRow[];
  overflow: number;
  showAgents: boolean;
}

export interface TrayOptions {
  showTitle: boolean;
  showAgents: boolean;
}

type CountedState = "working" | "blocked" | "attention" | "done" | "idle" | "unknown";

const STATE_ORDER: CountedState[] = ["working", "blocked", "attention", "done", "idle", "unknown"];

function headerFor(counts: Record<CountedState, number>, total: number): string {
  if (total === 0) return "No agents running";
  const parts = STATE_ORDER.filter((state) => counts[state] > 0).map(
    (state) => `${counts[state]} ${state}`,
  );
  if (parts.length === 1) return parts[0];
  const active = (["working", "blocked", "attention", "done"] as const)
    .filter((state) => counts[state] > 0)
    .map((state) => `${counts[state]} ${state}`);
  const detail = active.length > 0 ? active : ["idle"];
  return `${total} agents · ${detail.join(" · ")}`;
}

function titleFor(counts: Record<CountedState, number>, showTitle: boolean): string | null {
  if (!showTitle) return null;
  const attention = counts.blocked + counts.attention;
  if (attention > 0) return `! ${attention}`;
  if (counts.working > 0) return `● ${counts.working}`;
  return null;
}

/**
 * Build the tray status payload from the sidebar agent groups. Context is
 * `tab · pane` within one workspace, prefixed with the workspace name when
 * several workspaces have agents.
 */
export function buildTrayPayload(groups: WorkspaceAgents[], options: TrayOptions): TrayPayload {
  const counts: Record<CountedState, number> = {
    working: 0, blocked: 0, attention: 0, done: 0, idle: 0, unknown: 0,
  };
  const rows: TrayAgentRow[] = [];
  const multiWorkspace = groups.length > 1;
  for (const group of groups) {
    for (const row of group.agents) {
      counts[row.status] += 1;
      const where = row.paneTitle ?? row.dir;
      const context = multiWorkspace
        ? `${group.wsName} · ${row.tabName} · ${where}`
        : `${row.tabName} · ${where}`;
      rows.push({ nodeId: row.nodeId, agent: row.agent, context, status: row.status });
    }
  }
  const header = headerFor(counts, rows.length);
  return {
    title: titleFor(counts, options.showTitle),
    tooltip: header,
    header,
    agents: rows.slice(0, TRAY_MAX_ROWS),
    overflow: Math.max(0, rows.length - TRAY_MAX_ROWS),
    showAgents: options.showAgents,
  };
}

export interface TraySync {
  /** Debounced latest-wins push; identical payloads never resend. */
  queue(payload: TrayPayload): void;
  /** Deliver the pending payload now, if any. */
  flush(): void;
}

/**
 * Coalesce rapid status pushes into one backend call per quiet window.
 * A payload matching the last delivered one cancels any pending push.
 */
export function createTraySync(
  send: (payload: TrayPayload) => void,
  waitMs: number = TRAY_SYNC_DEBOUNCE_MS,
  clock?: TimerClock,
): TraySync {
  const debouncer = clock ? trailingDebouncer(waitMs, clock) : trailingDebouncer(waitMs);
  let lastJson = "";
  return {
    queue(payload: TrayPayload): void {
      const json = JSON.stringify(payload);
      if (json === lastJson) {
        debouncer.cancel();
        return;
      }
      debouncer.schedule(() => {
        lastJson = json;
        send(payload);
      });
    },
    flush(): void {
      debouncer.flush();
    },
  };
}
