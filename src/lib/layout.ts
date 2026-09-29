// Layout model: workspaces -> tabs -> binary split tree of panes.
// Pure module (no Svelte/Tauri imports) so it can be unit-tested with node:test.

export const LAYOUT_VERSION = 1;

export interface PaneNode {
  kind: "pane";
  id: string;
  cwd?: string;
  title?: string;
  /** Startup command: [program, ...args]. Defaults to the user's shell. */
  cmd?: string[];
}

export interface SplitNode {
  kind: "split";
  id: string;
  dir: "row" | "col";
  sizes: [number, number];
  first: LayoutNode;
  second: LayoutNode;
}

export type LayoutNode = PaneNode | SplitNode;

export interface Tab {
  id: string;
  name: string;
  root: LayoutNode;
  /** Pane node id shown full-tab when set (zoom). Cleared when it no longer exists. */
  zoomedPaneId?: string;
}

export interface Workspace {
  id: string;
  name: string;
  tabs: Tab[];
  activeTabId: string;
}

export interface Layout {
  version: number;
  workspaces: Workspace[];
  activeWorkspaceId: string;
}

export function newId(prefix: string): string {
  const uuid =
    typeof crypto !== "undefined" && "randomUUID" in crypto
      ? crypto.randomUUID().slice(0, 8)
      : Math.floor(Math.random() * 0xffffffff).toString(16);
  return `${prefix}-${uuid}`;
}

export function defaultPane(): PaneNode {
  return { kind: "pane", id: newId("pane") };
}

export function defaultTab(name = "Tab 1"): Tab {
  return { id: newId("tab"), name, root: defaultPane() };
}

export function defaultWorkspace(name = "Workspace 1"): Workspace {
  const tab = defaultTab();
  return { id: newId("ws"), name, tabs: [tab], activeTabId: tab.id };
}

export function defaultLayout(): Layout {
  const ws = defaultWorkspace();
  return { version: LAYOUT_VERSION, workspaces: [ws], activeWorkspaceId: ws.id };
}

export function activeWorkspace(layout: Layout): Workspace {
  return (
    layout.workspaces.find((w) => w.id === layout.activeWorkspaceId) ??
    layout.workspaces[0]
  );
}

export function activeTab(ws: Workspace): Tab {
  return ws.tabs.find((t) => t.id === ws.activeTabId) ?? ws.tabs[0];
}

export function countPanes(node: LayoutNode): number {
  return node.kind === "pane" ? 1 : countPanes(node.first) + countPanes(node.second);
}

export function findTabByPane(
  layout: Layout,
  paneId: string,
): { ws: Workspace; tab: Tab } | null {
  const contains = (node: LayoutNode): boolean =>
    node.kind === "pane"
      ? node.id === paneId
      : contains(node.first) || contains(node.second);
  for (const ws of layout.workspaces) {
    for (const tab of ws.tabs) {
      if (contains(tab.root)) return { ws, tab };
    }
  }
  return null;
}

/// Split a pane in place; returns the new sibling pane, or null when missing.
export function splitPaneInTab(
  tab: Tab,
  paneId: string,
  dir: "row" | "col",
): PaneNode | null {
  const sibling: PaneNode = defaultPane();
  const visit = (node: LayoutNode, set: (n: LayoutNode) => void): PaneNode | null => {
    if (node.kind === "pane") {
      if (node.id !== paneId) return null;
      set({
        kind: "split",
        id: newId("split"),
        dir,
        sizes: [0.5, 0.5],
        first: node,
        second: sibling,
      });
      return sibling;
    }
    return (
      visit(node.first, (n) => (node.first = n)) ??
      visit(node.second, (n) => (node.second = n))
    );
  };
  return visit(tab.root, (n) => (tab.root = n));
}

/// Close a pane in place, promoting its sibling. Closing the last pane of a
/// tab respawns a fresh pane so a tab always has a root.
export function closePaneInTab(tab: Tab, paneId: string): boolean {
  if (tab.root.kind === "pane") {
    if (tab.root.id !== paneId) return false;
    tab.root = defaultPane();
    return true;
  }
  const visit = (split: SplitNode, set: (n: LayoutNode) => void): boolean => {
    for (const side of ["first", "second"] as const) {
      const child = split[side];
      if (child.kind === "pane" && child.id === paneId) {
        set(side === "first" ? split.second : split.first);
        return true;
      }
      if (child.kind === "split" && visit(child, (n) => (split[side] = n))) return true;
    }
    return false;
  };
  return visit(tab.root, (n) => (tab.root = n));
}

// --- loading ---------------------------------------------------------------

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null;
}

function asString(v: unknown, fallback: string): string {
  return typeof v === "string" && v.length > 0 ? v : fallback;
}

function sanitizeNode(v: unknown): LayoutNode {
  if (!isRecord(v)) return defaultPane();
  if (v["kind"] === "pane" && typeof v["id"] === "string") {
    const node: PaneNode = { kind: "pane", id: v["id"] };
    if (typeof v["cwd"] === "string") node.cwd = v["cwd"];
    if (typeof v["title"] === "string") node.title = v["title"];
    if (Array.isArray(v["cmd"])) {
      node.cmd = (v["cmd"] as unknown[]).filter(
        (c): c is string => typeof c === "string",
      );
    }
    return node;
  }
  if (v["kind"] === "split" && typeof v["id"] === "string") {
    const dir = v["dir"] === "col" ? "col" : "row";
    const sizes = Array.isArray(v["sizes"]) ? v["sizes"] : [];
    const a = typeof sizes[0] === "number" && sizes[0] > 0 ? sizes[0] : 0.5;
    const b = typeof sizes[1] === "number" && sizes[1] > 0 ? sizes[1] : 0.5;
    return {
      kind: "split",
      id: v["id"],
      dir,
      sizes: [a / (a + b), b / (a + b)],
      first: sanitizeNode(v["first"]),
      second: sanitizeNode(v["second"]),
    };
  }
  return defaultPane();
}

function sanitizeTab(v: unknown, index: number): Tab {
  if (!isRecord(v)) return defaultTab(`Tab ${index + 1}`);
  const root = sanitizeNode(v["root"]);
  const tab: Tab = {
    id: asString(v["id"], newId("tab")),
    name: asString(v["name"], `Tab ${index + 1}`),
    root,
  };
  if (
    typeof v["zoomedPaneId"] === "string" &&
    findPane(root, v["zoomedPaneId"]) !== null
  ) {
    tab.zoomedPaneId = v["zoomedPaneId"];
  }
  return tab;
}

function sanitizeWorkspace(v: unknown, index: number): Workspace {
  if (!isRecord(v)) return defaultWorkspace(`Workspace ${index + 1}`);
  const tabs = Array.isArray(v["tabs"])
    ? (v["tabs"] as unknown[]).map((t, i) => sanitizeTab(t, i))
    : [];
  if (tabs.length === 0) tabs.push(defaultTab());
  const ws: Workspace = {
    id: asString(v["id"], newId("ws")),
    name: asString(v["name"], `Workspace ${index + 1}`),
    tabs,
    activeTabId: typeof v["activeTabId"] === "string" ? v["activeTabId"] : "",
  };
  if (!ws.tabs.some((t) => t.id === ws.activeTabId)) ws.activeTabId = ws.tabs[0].id;
  return ws;
}

/// Coerce loaded JSON into a valid Layout, filling defaults for anything
/// broken. Unknown versions start fresh rather than misrender.
export function sanitizeLayout(v: unknown): Layout {
  if (!isRecord(v) || v["version"] !== LAYOUT_VERSION || !Array.isArray(v["workspaces"])) {
    return defaultLayout();
  }
  const workspaces = (v["workspaces"] as unknown[]).map((w, i) =>
    sanitizeWorkspace(w, i),
  );
  if (workspaces.length === 0) workspaces.push(defaultWorkspace());
  const layout: Layout = {
    version: LAYOUT_VERSION,
    workspaces,
    activeWorkspaceId:
      typeof v["activeWorkspaceId"] === "string" ? v["activeWorkspaceId"] : "",
  };
  if (!workspaces.some((w) => w.id === layout.activeWorkspaceId)) {
    layout.activeWorkspaceId = workspaces[0].id;
  }
  return layout;
}

/** All pane node ids in a tree. */
export function collectPaneIds(node: LayoutNode): string[] {
  if (node.kind === "pane") return [node.id];
  return [...collectPaneIds(node.first), ...collectPaneIds(node.second)];
}

/** Agent states by live pane id, as emitted by the backend poller. */
export type AgentSnapshot = Record<string, { state: string; agent?: string }>;

/** Live ids that transitioned from working to anything else. */
export function detectFinished(
  prev: AgentSnapshot,
  next: AgentSnapshot,
): string[] {
  const out: string[] = [];
  for (const [id, state] of Object.entries(next)) {
    if (prev[id]?.state === "working" && state.state !== "working") out.push(id);
  }
  return out;
}

/** Live ids that were working in prev but are absent from next. */
export function detectVanished(
  prev: AgentSnapshot,
  next: AgentSnapshot,
): string[] {
  return Object.keys(prev).filter(
    (id) => prev[id].state === "working" && !(id in next),
  );
}

/** A pane's placement as canvas fractions: [x, y, w, h]. */
export interface PanePlacement {
  node: PaneNode;
  rect: [number, number, number, number];
}

/** A divider's placement: the split's rect plus the boundary position. */
export interface DividerPlacement {
  splitId: string;
  dir: "row" | "col";
  at: number;
  rect: [number, number, number, number];
}

export interface TabLayout {
  panes: PanePlacement[];
  dividers: DividerPlacement[];
}

/**
 * Flatten a split tree into stable keyed placements. Panes keep their node
 * ids across reshapes, so the UI can render them in a keyed each-block and
 * never remount (and kill) a live terminal when splits change.
 */
export function computeLayout(root: LayoutNode): TabLayout {
  const panes: PanePlacement[] = [];
  const dividers: DividerPlacement[] = [];
  const visit = (
    node: LayoutNode,
    x: number,
    y: number,
    w: number,
    h: number,
  ): void => {
    if (node.kind === "pane") {
      panes.push({ node, rect: [x, y, w, h] });
      return;
    }
    const [a, b] = node.sizes;
    const total = a + b > 0 ? a + b : 1;
    const at = a / total;
    dividers.push({ splitId: node.id, dir: node.dir, at, rect: [x, y, w, h] });
    if (node.dir === "row") {
      visit(node.first, x, y, w * at, h);
      visit(node.second, x + w * at, y, w * (1 - at), h);
    } else {
      visit(node.first, x, y, w, h * at);
      visit(node.second, x, y + h * at, w, h * (1 - at));
    }
  };
  visit(root, 0, 0, 1, 1);
  return { panes, dividers };
}

/** Find a split node by id (for divider drags). */
export function findSplit(root: LayoutNode, splitId: string): SplitNode | null {
  if (root.kind === "pane") return null;
  if (root.id === splitId) return root;
  return findSplit(root.first, splitId) ?? findSplit(root.second, splitId);
}

/** Find a pane node by id. */
export function findPane(root: LayoutNode, paneId: string): PaneNode | null {
  if (root.kind === "pane") return root.id === paneId ? root : null;
  return findPane(root.first, paneId) ?? findPane(root.second, paneId);
}

/**
 * Zoom a pane full-tab, or clear the zoom. Unknown ids clear the zoom rather
 * than sticking the tab on a missing pane.
 */
export function setZoomedPane(tab: Tab, paneId: string | null): void {
  tab.zoomedPaneId =
    paneId !== null && findPane(tab.root, paneId) !== null ? paneId : undefined;
}

/** Drop a zoom that points at a removed pane (after closes). */
export function clearStaleZoom(tab: Tab): void {
  if (
    tab.zoomedPaneId !== undefined &&
    findPane(tab.root, tab.zoomedPaneId) === null
  ) {
    tab.zoomedPaneId = undefined;
  }
}
