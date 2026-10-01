import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { playbackPayload, routeNotification } from "./notify";
import { store } from "./store.svelte";
import { captureAgentEnded, captureAgentStarted, type AgentEndOutcome } from "./telemetry";
import { toasts } from "./toasts.svelte.ts";
import { baseName, collectPaneIds, findPane, findTabByPane, type Tab, type Workspace } from "./layout";
import {
  acknowledgeAgent, agentStatusLabel, applyAgentUpdate, effectiveAgentStatus,
  emptyAgentModel, paneIsVisible, registerAgentPane, rollupStatuses,
  type AgentStatus, type AgentTransition, type AgentUpdate, type Rollup,
} from "./agentStatus";
export type { Rollup } from "./agentStatus";

export interface ActiveAgent {
  nodeId: string;
  agent: string;
  cli?: string;
  dir: string;
  dirPath?: string;
  status: Rollup;
  statusTitle: string;
  tabName: string;
  paneTitle?: string;
}
export interface WorkspaceAgents {
  wsId: string;
  wsName: string;
  agents: ActiveAgent[];
}
class AgentStore {
  private model = $state(emptyAgentModel());
  private liveToNode = $state<Record<string, string>>({});
  private lastAgent = $state<Record<string, { agent: string; cli?: string; cwd?: string }>>({});
  private pending = new Map<number, AgentTransition>();
  private disposed = new Set<number>();
  /** Observed agent-session starts by node, for telemetry durations. */
  private sessionStart = new Map<string, number>();
  /** Ignore the first snapshot, which may describe a resumed session. */
  private telemetryBaselineReady = false;
  private foreground = false;
  private started = false;
  // TEMP perf instrumentation (removed after the lag audit).
  private perfLog = false;
  private perfCount = 0;
  private perfWindowStart = 0;
  get states(): Record<string, AgentStatus> { return this.model.states; }
  get attention(): string[] { return this.unreadNodes("attention"); }
  get done(): string[] { return this.unreadNodes("done"); }
  private unreadNodes(kind: "attention" | "done"): string[] {
    return Object.entries(this.model.unread)
      .filter(([, unread]) => unread.kind === kind)
      .map(([live]) => this.liveToNode[live]).filter((node): node is string => !!node);
  }
  register(liveId: number, nodeId: string): void {
    const registered = registerAgentPane(this.liveToNode, liveId, nodeId);
    for (const oldLive of registered.replacedLiveIds) this.unregister(oldLive);
    this.disposed.delete(liveId);
    this.liveToNode = registered.mapping;
    this.rememberAgents();
    const pending = this.pending.get(liveId);
    this.pending.delete(liveId);
    if (pending && this.model.unread[liveId]?.eventId === pending.eventId) this.notify(pending);
    this.acknowledge(nodeId);
  }
  unregister(liveId: number): void {
    const node = this.liveToNode[liveId];
    this.disposed.add(liveId);
    delete this.liveToNode[liveId];
    this.pending.delete(liveId);
    this.model = acknowledgeAgent(this.model, liveId, true);
    if (node) {
      const last = this.lastAgent[node];
      if (last?.cli) this.captureEnded(node, last.cli, "closed");
      delete this.lastAgent[node];
    }
  }
  start(): void {
    if (this.started) return;
    this.started = true;
    try {
      this.perfLog = window.localStorage.getItem("ubra.perfLog") === "1";
    } catch {
      this.perfLog = false;
    }
    void this.startSubscriptions().catch(console.error);
  }

  /** TEMP perf instrumentation (removed after the lag audit). */
  private perfNoteUpdate(): void {
    if (!this.perfLog) return;
    const now = Date.now();
    if (this.perfWindowStart === 0) this.perfWindowStart = now;
    this.perfCount += 1;
    if (now - this.perfWindowStart >= 30_000) {
      console.log(`ubra-perf: ${this.perfCount} agent updates in 30s`);
      this.perfCount = 0;
      this.perfWindowStart = now;
    }
  }
  private async startSubscriptions(): Promise<void> {
    const win = getCurrentWindow();
    await win.onFocusChanged(({ payload }) => {
      this.foreground = payload;
      if (payload) this.acknowledgeFocused();
    });
    this.foreground = await win.isFocused();
    // Subscribe before fetching: startup snapshots cannot overwrite newer events.
    await listen<AgentUpdate>("agent-state-update", (event) => this.onUpdate(event.payload));
    document.addEventListener("visibilitychange", () => this.acknowledgeFocused());
    const snapshot = await invoke<AgentUpdate>("agent_snapshot");
    this.onUpdate({ ...snapshot, transitions: [] });
    this.telemetryBaselineReady = true;
    this.acknowledgeFocused();
  }
  paneState(nodeId: string): AgentStatus | null {
    const live = this.liveForNode(nodeId);
    return live === undefined ? null : this.states[live] ?? null;
  }
  paneAgentLabel(nodeId: string): string | undefined {
    return this.paneState(nodeId)?.agent ?? this.lastAgent[nodeId]?.agent;
  }
  private liveForNode(nodeId: string): number | undefined {
    const live = Object.entries(this.liveToNode).find(([, node]) => node === nodeId)?.[0];
    return live === undefined ? undefined : Number(live);
  }
  paneStatus(nodeId: string): Rollup {
    const live = this.liveForNode(nodeId);
    return effectiveAgentStatus(this.paneState(nodeId), live === undefined ? undefined : this.model.unread[live]);
  }
  paneStatusTitle(nodeId: string): string {
    const state = this.paneState(nodeId);
    const label = agentStatusLabel(this.paneStatus(nodeId));
    return state?.reason ? `${label} — ${state.reason}` : label;
  }
  tabRollup(tab: Tab): Rollup { return this.rollupFor(collectPaneIds(tab.root)); }
  workspaceRollup(ws: Workspace): Rollup { return this.rollupFor(ws.tabs.flatMap((t) => collectPaneIds(t.root))); }
  private rollupFor(nodes: string[]): Rollup { return rollupStatuses(nodes.map((node) => this.paneStatus(node))); }
  jumpToReview(): void {
    const target = this.attention[0] ?? this.done[0];
    if (target) this.jumpToPane(target);
  }
  jumpToPane(nodeId: string): void {
    store.revealPane(nodeId);
    const found = store.layout && findTabByPane(store.layout, nodeId);
    if (found?.tab.zoomedPaneId && found.tab.zoomedPaneId !== nodeId) store.toggleZoomPane(found.tab.zoomedPaneId);
    store.paneFocusTarget = nodeId;
  }
  private focusedNode(): string | null {
    return document.activeElement?.closest<HTMLElement>(".pane-view[data-pane-id]")?.dataset.paneId ?? null;
  }
  private isSeen(nodeId: string): boolean {
    const layout = store.layout;
    const found = layout && findTabByPane(layout, nodeId);
    if (!layout || !found || store.settingsOpen || store.pendingClose) return false;
    if (store.firstRun) return false;
    return paneIsVisible({
      activeWorkspaceId: layout.activeWorkspaceId, workspaceId: found.ws.id,
      activeTabId: found.ws.activeTabId, tabId: found.tab.id,
      zoomedPaneId: found.tab.zoomedPaneId, nodeId,
      foreground: this.foreground && document.visibilityState !== "hidden",
      focusedNodeId: this.focusedNode(),
    });
  }
  acknowledge(nodeId: string): void {
    const live = this.liveForNode(nodeId);
    if (live !== undefined) this.model = acknowledgeAgent(this.model, live, this.isSeen(nodeId));
  }
  private acknowledgeFocused(): void {
    const node = this.focusedNode();
    if (node) this.acknowledge(node);
  }
  knownClis(): { cli: string; label: string }[] {
    const labels = new Map<string, string>();
    for (const st of [...Object.values(this.states), ...Object.values(this.lastAgent)]) {
      if (st.cli) labels.set(st.cli.toLowerCase(), st.agent ?? st.cli);
    }
    for (const muted of store.mutedAgents) {
      const lower = muted.toLowerCase();
      if (!labels.has(lower)) labels.set(lower, muted);
    }
    return [...labels].map(([cli, label]) => ({ cli, label })).sort((a, b) => a.label.localeCompare(b.label));
  }
  activeAgents(): WorkspaceAgents[] {
    const layout = store.layout;
    if (!layout) return [];
    const rows: { wsId: string; row: ActiveAgent }[] = [];
    for (const node of Object.values(this.liveToNode)) {
      const st = this.paneState(node);
      const last = this.lastAgent[node];
      const label = st?.agent ?? last?.agent;
      if (!label) continue;
      const found = findTabByPane(layout, node);
      if (!found) continue;
      const pane = findPane(found.tab.root, node);
      const cwd = st?.cwd ?? last?.cwd ?? pane?.cwd;
      rows.push({ wsId: found.ws.id, row: {
        nodeId: node, agent: label, cli: st?.cli ?? last?.cli,
        dir: cwd ? baseName(cwd) : label, dirPath: cwd ?? undefined,
        status: this.paneStatus(node), statusTitle: this.paneStatusTitle(node),
        tabName: found.tab.name, paneTitle: pane?.title,
      } });
    }
    const order: Rollup[] = ["blocked", "working", "attention", "done", "unknown", "idle"];
    return layout.workspaces.map((ws) => ({
      wsId: ws.id, wsName: ws.name,
      agents: rows.filter((row) => row.wsId === ws.id).map((row) => row.row)
        .sort((a, b) => order.indexOf(a.status) - order.indexOf(b.status)),
    })).filter((group) => group.agents.length > 0);
  }
  private rememberAgents(): void {
    for (const [live, node] of Object.entries(this.liveToNode)) {
      const state = this.states[live];
      if (!state?.agent) continue;
      const previous = this.lastAgent[node];
      if (this.telemetryBaselineReady && previous?.agent !== state.agent) {
        if (previous?.agent && previous.cli) this.captureEnded(node, previous.cli, "closed");
        this.sessionStart.set(node, Date.now());
        if (state.cli) {
          captureAgentStarted(state.cli).catch((error: unknown) => {
            console.error("ubra: agent-started capture failed", error);
          });
        }
      }
      this.lastAgent[node] = {
        agent: state.agent, cli: state.cli, cwd: state.cwd ?? this.lastAgent[node]?.cwd,
      };
    }
  }
  /** Emit an agent-ended event when a session start was observed. */
  private captureEnded(node: string, cli: string, outcome: AgentEndOutcome): void {
    const started = this.sessionStart.get(node);
    this.sessionStart.delete(node);
    if (started === undefined || !cli) return;
    captureAgentEnded(cli, Date.now() - started, outcome).catch((error: unknown) => {
      console.error("ubra: agent-ended capture failed", error);
    });
  }
  private onUpdate(update: AgentUpdate): void {
    this.perfNoteUpdate();
    const result = applyAgentUpdate(this.model, update);
    this.model = result.model;
    for (const live of this.disposed) this.model = acknowledgeAgent(this.model, live, true);
    this.rememberAgents();
    for (const transition of result.transitions) {
      const node = this.liveToNode[transition.paneId];
      if (node) {
        this.notify(transition);
        const cli = transition.cli ?? this.lastAgent[node]?.cli;
        if (cli) {
          const outcome = transition.kind === "task-completed" ? "completed" : "stopped";
          this.captureEnded(node, cli, outcome);
        }
      } else if (!this.disposed.has(transition.paneId)) {
        this.pending.set(transition.paneId, transition);
      }
    }
    this.acknowledgeFocused();
  }
  private notify(transition: AgentTransition): void {
    const nodeId = this.liveToNode[transition.paneId];
    if (!nodeId) return;
    const last = this.lastAgent[nodeId];
    if (transition.agent) this.lastAgent[nodeId] = {
      agent: transition.agent, cli: transition.cli, cwd: transition.cwd ?? last?.cwd,
    };
    if (this.isSeen(nodeId)) return;
    const label = transition.agent ?? last?.agent ?? "Agent";
    const kind = transition.kind === "task-completed" ? "done" : "request";
    const title = kind === "done" ? `${label} finished` : `${label} needs attention`;
    const route = routeNotification({ delivery: store.notifyDelivery,
      soundEnabled: store.soundEnabled, mutedClis: store.mutedAgents,
      cli: transition.cli ?? last?.cli });
    if (route.toast) toasts.push(title, "Click to review", nodeId);
    if (route.system) invoke("notify_agent", { title, body: "Open Ubra to review", kind }).catch(console.error);
    if (route.sound) invoke("play_sound", playbackPayload(kind, store.soundStyle, store.soundFile)).catch(console.error);
  }
}
export const agent = new AgentStore();
