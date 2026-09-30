/** Runtime evidence and unread review are independent. */
export type RuntimeState = "working" | "blocked" | "unknown" | "done" | "idle";
export type Rollup = RuntimeState | "attention";
export interface AgentStatus {
  state: RuntimeState;
  agent?: string;
  cli?: string;
  cwd?: string;
  agentInstanceId?: string;
  reason?: string;
}
export interface AgentTransition {
  eventId: string;
  paneId: number;
  agentInstanceId: string;
  kind: "task-completed" | "agent-stopped";
  agent?: string;
  cli?: string;
  cwd?: string;
}
export interface AgentUpdate {
  revision: number;
  states: Record<string, AgentStatus>;
  transitions: AgentTransition[];
}
export interface UnreadStatus {
  kind: "done" | "attention";
  agentInstanceId: string;
  eventId?: string;
}
export interface AgentModel {
  revision: number;
  states: Record<string, AgentStatus>;
  unread: Record<string, UnreadStatus>;
  seenEvents: ReadonlySet<string>;
}
export function emptyAgentModel(): AgentModel {
  return { revision: -1, states: {}, unread: {}, seenEvents: new Set() };
}
/** Upper bound for retained dedup/ack identities; oldest evicted first. */
export const MAX_RETAINED_AGENT_IDS = 512;
/** Keep at most `max` identities, preferring the most recently added. */
export function retainRecent<T>(ids: Iterable<T>, max: number = MAX_RETAINED_AGENT_IDS): Set<T> {
  const unique = [...new Set(ids)];
  return new Set(unique.slice(Math.max(0, unique.length - max)));
}
export function applyAgentUpdate(model: AgentModel, update: AgentUpdate): {
  model: AgentModel;
  transitions: AgentTransition[];
} {
  if (update.revision < model.revision) return { model, transitions: [] };
  // A snapshot can arrive before the event at the same revision. Its live
  // transition still needs delivery, while its state cannot regress.
  const states = update.revision === model.revision ? model.states : update.states;
  const unread = { ...model.unread };
  for (const [id, status] of Object.entries(states)) {
    if (status.state === "working" ||
        (status.agentInstanceId && unread[id]?.agentInstanceId !== status.agentInstanceId)) {
      delete unread[id];
    }
  }
  const seenEvents = new Set(model.seenEvents);
  const transitions: AgentTransition[] = [];
  for (const transition of update.transitions) {
    if (seenEvents.has(transition.eventId)) continue;
    seenEvents.add(transition.eventId);
    const status = states[transition.paneId];
    // A delayed notification for an old instance/task must not replace a live task.
    if (status?.state === "working" ||
        (status?.agentInstanceId && status.agentInstanceId !== transition.agentInstanceId)) continue;
    unread[transition.paneId] = {
      kind: transition.kind === "task-completed" ? "done" : "attention",
      agentInstanceId: transition.agentInstanceId,
      eventId: transition.eventId,
    };
    transitions.push(transition);
  }
  return {
    model: { revision: update.revision, states, unread, seenEvents: retainRecent(seenEvents) },
    transitions,
  };
}
export function acknowledgeAgent(model: AgentModel, liveId: number, seen: boolean): AgentModel {
  if (!seen || !model.unread[liveId]) return model;
  const unread = { ...model.unread };
  delete unread[liveId];
  return { ...model, unread };
}
const rank: Record<Rollup, number> = {
  idle: 0, unknown: 1, done: 2, attention: 3, working: 4, blocked: 5,
};
export function rollupStatuses(statuses: Iterable<Rollup>): Rollup {
  let result: Rollup = "idle";
  for (const status of statuses) if (rank[status] > rank[result]) result = status;
  return result;
}
export function effectiveAgentStatus(status?: AgentStatus | null, unread?: UnreadStatus): Rollup {
  return rollupStatuses([status?.state ?? "idle", unread?.kind ?? "idle"]);
}
export function paneIsVisible(context: {
  activeWorkspaceId: string;
  workspaceId: string;
  activeTabId: string;
  tabId: string;
  zoomedPaneId?: string;
  nodeId: string;
  foreground: boolean;
  focusedNodeId: string | null;
}): boolean {
  return context.foreground && context.focusedNodeId === context.nodeId &&
    context.activeWorkspaceId === context.workspaceId && context.activeTabId === context.tabId &&
    (!context.zoomedPaneId || context.zoomedPaneId === context.nodeId);
}
export function agentStatusLabel(status: Rollup): string {
  return { working: "Working", blocked: "Blocked", unknown: "Unknown", done: "Done",
    idle: "Idle", attention: "Needs review" }[status];
}

/** A respawn replaces a pane's old live session; moves retain the same ID. */
export function registerAgentPane(mapping: Record<string, string>, liveId: number, nodeId: string): {
  mapping: Record<string, string>;
  replacedLiveIds: number[];
} {
  const next = { ...mapping };
  const replacedLiveIds: number[] = [];
  for (const [live, node] of Object.entries(next)) {
    if (node === nodeId && Number(live) !== liveId) {
      delete next[live];
      replacedLiveIds.push(Number(live));
    }
  }
  next[liveId] = nodeId;
  return { mapping: next, replacedLiveIds };
}
