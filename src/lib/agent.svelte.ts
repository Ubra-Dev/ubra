import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { store } from "./store.svelte";
import {
  collectPaneIds,
  detectFinished,
  detectVanished,
  findPane,
  findTabByPane,
  type AgentSnapshot,
  type Layout,
  type Tab,
  type Workspace,
} from "./layout";

export type Rollup = "working" | "attention" | "idle";

export interface ActiveAgent {
  nodeId: string;
  agent: string;
  status: "working" | "attention";
  tabName: string;
  paneTitle?: string;
}

export interface WorkspaceAgents {
  wsId: string;
  wsName: string;
  agents: ActiveAgent[];
}

class AgentStore {
  states = $state<AgentSnapshot>({});
  attention = $state<string[]>([]);
  private liveToNode = new Map<number, string>();
  /** Last-known agent name per pane node, for finished (attention) rows. */
  private lastAgent = new Map<string, string>();
  private started = false;

  register(liveId: number, nodeId: string): void {
    this.liveToNode.set(liveId, nodeId);
  }

  unregister(liveId: number): void {
    this.liveToNode.delete(liveId);
  }

  start(): void {
    if (this.started) return;
    this.started = true;
    listen<AgentSnapshot>("agent-states", (event) =>
      this.onStates(event.payload),
    ).catch(console.error);
  }

  paneState(nodeId: string): { state: string; agent?: string } | null {
    for (const [live, node] of this.liveToNode) {
      if (node === nodeId) return this.states[live] ?? null;
    }
    return null;
  }

  tabRollup(tab: Tab): Rollup {
    return this.rollupFor(collectPaneIds(tab.root));
  }

  workspaceRollup(ws: Workspace): Rollup {
    return this.rollupFor(ws.tabs.flatMap((t) => collectPaneIds(t.root)));
  }

  private rollupFor(nodeIds: string[]): Rollup {
    let seenAttention = false;
    for (const id of nodeIds) {
      if (this.paneState(id)?.state === "working") return "working";
      if (this.attention.includes(id)) seenAttention = true;
    }
    return seenAttention ? "attention" : "idle";
  }

  jumpToAttention(): void {
    const target = this.attention[0];
    if (target) this.jumpToPane(target);
  }

  /** Reveal a pane and mark it seen. */
  jumpToPane(nodeId: string): void {
    store.revealPane(nodeId);
    this.attention = this.attention.filter((id) => id !== nodeId);
  }

  /**
   * Working panes plus unfinished attention, grouped per workspace in
   * workspace order (working rows before attention rows). Workspaces without
   * agents are omitted; an empty list means none anywhere.
   */
  activeAgents(): WorkspaceAgents[] {
    const layout = store.layout;
    if (!layout) return [];
    const seen = new Set<string>();
    const rows: { wsId: string; row: ActiveAgent }[] = [];
    const push = (node: string, label: string, status: ActiveAgent["status"]) => {
      if (seen.has(node)) return;
      const found = findTabByPane(layout, node);
      if (!found) return;
      seen.add(node);
      rows.push({
        wsId: found.ws.id,
        row: {
          nodeId: node,
          agent: label,
          status,
          tabName: found.tab.name,
          paneTitle: findPane(found.tab.root, node)?.title,
        },
      });
    };
    for (const [live, node] of this.liveToNode) {
      const st = this.states[live];
      if (st?.state === "working") push(node, st.agent ?? "Agent", "working");
    }
    for (const node of this.attention) {
      push(node, this.lastAgent.get(node) ?? "Agent", "attention");
    }
    const groups: WorkspaceAgents[] = [];
    for (const ws of layout.workspaces) {
      const mine = rows.filter((r) => r.wsId === ws.id).map((r) => r.row);
      if (mine.length > 0) {
        groups.push({ wsId: ws.id, wsName: ws.name, agents: mine });
      }
    }
    return groups;
  }

  private isNodeVisible(layout: Layout, nodeId: string): boolean {
    const found = findTabByPane(layout, nodeId);
    return (
      found !== null &&
      layout.activeWorkspaceId === found.ws.id &&
      found.ws.activeTabId === found.tab.id
    );
  }

  private windowVisible(): boolean {
    return (
      typeof document === "undefined" || document.visibilityState !== "hidden"
    );
  }

  private onStates(next: AgentSnapshot): void {
    const layout = store.layout;
    if (layout) {
      const winVisible = this.windowVisible();
      // Vanished ids with a live mapping died unexpectedly (deliberate closes
      // unregister first); treat them like finished panes.
      const finished = [
        ...detectFinished(this.states, next),
        ...detectVanished(this.states, next),
      ];
      for (const liveStr of finished) {
        const nodeId = this.liveToNode.get(Number(liveStr));
        if (!nodeId) continue;
        if (this.isNodeVisible(layout, nodeId) && winVisible) continue;
        if (!this.attention.includes(nodeId)) {
          this.attention.push(nodeId);
          const label = this.states[liveStr]?.agent ?? "Agent";
          invoke("notify_agent", {
            title: `${label} needs attention`,
            body: "Open Ubra to review",
          }).catch(() => {});
        }
      }
      this.attention = this.attention.filter(
        (id) =>
          findTabByPane(layout, id) !== null &&
          !(this.isNodeVisible(layout, id) && winVisible),
      );
      for (const [live, node] of this.liveToNode) {
        const name = next[live]?.agent;
        if (name) this.lastAgent.set(node, name);
      }
      const live = new Set(
        layout.workspaces.flatMap((ws) =>
          ws.tabs.flatMap((t) => collectPaneIds(t.root)),
        ),
      );
      for (const node of this.lastAgent.keys()) {
        if (!live.has(node)) this.lastAgent.delete(node);
      }
    }
    this.states = next;
  }
}

export const agent = new AgentStore();
