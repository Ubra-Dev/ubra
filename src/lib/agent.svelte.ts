import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  chimeStyleParam,
  routeNotification,
  type SoundKind,
} from "./notify";
import { store } from "./store.svelte";
import { toasts } from "./toasts.svelte.ts";
import {
  baseName,
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

export type Rollup =
  | "working"
  | "blocked"
  | "unknown"
  | "attention"
  | "done"
  | "idle";

export interface ActiveAgent {
  nodeId: string;
  agent: string;
  cli?: string;
  /** Display name: the agent's working directory (basename). */
  dir: string;
  /** Full working directory for tooltips, when known. */
  dirPath?: string;
  status: "working" | "blocked" | "unknown" | "attention" | "done" | "idle";
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
  /** Panes whose agent stopped unexpectedly (vanished) and is unseen. */
  attention = $state<string[]>([]);
  /** Panes whose agent cleanly finished a task and is unseen. */
  done = $state<string[]>([]);
  private liveToNode = new Map<number, string>();
  /** Last-known agent name + CLI + cwd per pane node, for ended rows. */
  private lastAgent = new Map<
    string,
    { agent: string; cli?: string; cwd?: string }
  >();
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

  paneState(
    nodeId: string,
  ): { state: string; agent?: string; cli?: string; cwd?: string } | null {
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

  /** Precedence: blocked > working > attention > done > unknown > idle. */
  private rollupFor(nodeIds: string[]): Rollup {
    let seenAttention = false;
    let seenDone = false;
    let seenUnknown = false;
    for (const id of nodeIds) {
      const state = this.paneState(id)?.state;
      if (state === "blocked") return "blocked";
      if (state === "working") return "working";
      if (this.attention.includes(id)) seenAttention = true;
      else if (this.done.includes(id)) seenDone = true;
      else if (state === "done") seenDone = true;
      else if (state === "unknown") seenUnknown = true;
    }
    if (seenAttention) return "attention";
    if (seenDone) return "done";
    return seenUnknown ? "unknown" : "idle";
  }

  /** Jump to the first pane needing review (unexpected stops first). */
  jumpToReview(): void {
    const target = this.attention[0] ?? this.done[0];
    if (target) this.jumpToPane(target);
  }

  /** Reveal a pane and mark it seen. */
  jumpToPane(nodeId: string): void {
    store.revealPane(nodeId);
    this.attention = this.attention.filter((id) => id !== nodeId);
    this.done = this.done.filter((id) => id !== nodeId);
  }

  /**
   * Every agent cli seen this session plus every muted cli (so muted agents
   * show in Settings even before they run), with display labels.
   */
  knownClis(): { cli: string; label: string }[] {
    const labels = new Map<string, string>();
    for (const live of this.liveToNode.keys()) {
      const st = this.states[live];
      if (st?.cli) labels.set(st.cli.toLowerCase(), st.agent ?? st.cli);
    }
    for (const last of this.lastAgent.values()) {
      if (last.cli) labels.set(last.cli.toLowerCase(), last.agent);
    }
    for (const muted of store.mutedAgents) {
      const lower = muted.toLowerCase();
      if (!labels.has(lower)) labels.set(lower, muted);
    }
    return [...labels.entries()]
      .map(([cli, label]) => ({ cli, label }))
      .sort((a, b) => a.label.localeCompare(b.label));
  }

  /**
   * Working panes, blocked/starting panes, live-done panes (agent at prompt),
   * unseen stops, and idle known agents, grouped per workspace in workspace
   * order (working first, idle last). Clean finishes show done; unexpected
   * stops show attention; exit-based rows become idle once seen while
   * live-done rows keep their check until the agent works again. Workspaces
   * without agents are omitted; an empty list means none anywhere.
   */
  activeAgents(): WorkspaceAgents[] {
    const layout = store.layout;
    if (!layout) return [];
    const seen = new Set<string>();
    const rows: { wsId: string; row: ActiveAgent }[] = [];
    const push = (
      node: string,
      label: string,
      cli: string | undefined,
      cwd: string | undefined,
      status: ActiveAgent["status"],
    ) => {
      if (seen.has(node)) return;
      const found = findTabByPane(layout, node);
      if (!found) return;
      seen.add(node);
      const pane = findPane(found.tab.root, node);
      const full = cwd ?? pane?.cwd ?? null;
      rows.push({
        wsId: found.ws.id,
        row: {
          nodeId: node,
          agent: label,
          cli,
          dir: full ? baseName(full) : label,
          dirPath: full ?? undefined,
          status,
          tabName: found.tab.name,
          paneTitle: pane?.title,
        },
      });
    };
    for (const status of ["working", "blocked", "unknown", "done"] as const) {
      for (const [live, node] of this.liveToNode) {
        const st = this.states[live];
        if (st?.state === status) {
          push(node, st.agent ?? "Agent", st.cli, st.cwd, status);
        }
      }
    }
    for (const node of this.attention) {
      const last = this.lastAgent.get(node);
      push(node, last?.agent ?? "Agent", last?.cli, last?.cwd, "attention");
    }
    for (const node of this.done) {
      const last = this.lastAgent.get(node);
      push(node, last?.agent ?? "Agent", last?.cli, last?.cwd, "done");
    }
    for (const [node, last] of this.lastAgent) {
      push(node, last.agent, last.cli, last.cwd, "idle");
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
      const unseen = (nodeId: string): boolean =>
        !(this.isNodeVisible(layout, nodeId) && winVisible);
      const flag = (
        liveStr: string,
        list: string[],
        title: (label: string) => string,
        kind: SoundKind,
      ): void => {
        const nodeId = this.liveToNode.get(Number(liveStr));
        if (!nodeId || !unseen(nodeId) || list.includes(nodeId)) return;
        list.push(nodeId);
        const label = this.states[liveStr]?.agent ?? "Agent";
        const cli = this.states[liveStr]?.cli;
        const route = routeNotification({
          delivery: store.notifyDelivery,
          soundEnabled: store.soundEnabled,
          mutedClis: store.mutedAgents,
          cli,
        });
        if (route.toast) {
          toasts.push(title(label), "Click to review", nodeId);
        }
        if (route.system) {
          invoke("notify_agent", {
            title: title(label),
            body: "Open Ubra to review",
            kind,
          }).catch((e) => console.error("ubra: agent notification failed", e));
        }
        if (route.sound) {
          invoke("play_sound", {
            kind,
            style: chimeStyleParam(store.soundStyle),
            file: store.soundFile.trim() === "" ? null : store.soundFile,
          }).catch((e) => console.error("ubra: agent sound failed", e));
        }
      };
      // Clean exits are done; vanished ids with a live mapping died
      // unexpectedly (deliberate closes unregister first).
      for (const liveStr of detectFinished(this.states, next)) {
        flag(liveStr, this.done, (label) => `${label} finished`, "done");
      }
      for (const liveStr of detectVanished(this.states, next)) {
        flag(
          liveStr,
          this.attention,
          (label) => `${label} needs attention`,
          "request",
        );
      }
      const stillUnseen = (id: string): boolean =>
        findTabByPane(layout, id) !== null && unseen(id);
      this.attention = this.attention.filter(stillUnseen);
      this.done = this.done.filter(stillUnseen);
      for (const [live, node] of this.liveToNode) {
        const st = next[live];
        if (st?.agent) {
          this.lastAgent.set(node, {
            agent: st.agent,
            cli: st.cli,
            cwd: st.cwd ?? this.lastAgent.get(node)?.cwd,
          });
        }
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
