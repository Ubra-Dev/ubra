// Layout model: workspaces -> tabs -> binary split tree of panes.
// Pure module (no Svelte/Tauri imports) so it can be unit-tested with node:test.

export const LAYOUT_VERSION = 2;
export const MAX_LAYOUT_BYTES = 4 * 1024 * 1024;
export const MAX_LAYOUT_DEPTH = 32;
export const MAX_LAYOUT_ENTITIES = 4096;
export const MIN_SPLIT_FRACTION = 0.05;

export interface PaneNode {
  kind: "pane";
  id: string;
  cwd?: string;
  title?: string;
  /** Startup command: [program, ...args]. Defaults to the user's shell. */
  cmd?: string[];
  /** false requires explicit launch authorization; absence keeps legacy auto-run. */
  cmdOnRestore?: boolean;
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
  const uuid = crypto.randomUUID();
  return `${prefix}-${uuid}`;
}

export function defaultPane(): PaneNode {
  return { kind: "pane", id: newId("pane") };
}

export function defaultTab(name = "Tab 1"): Tab {
  return { id: newId("tab"), name, root: defaultPane() };
}

export function gridTab(name = "Tab 1"): Tab {
  const row = (): SplitNode => ({
    kind: "split",
    id: newId("split"),
    dir: "row",
    sizes: [0.5, 0.5],
    first: defaultPane(),
    second: defaultPane(),
  });
  return {
    id: newId("tab"),
    name,
    root: {
      kind: "split",
      id: newId("split"),
      dir: "col",
      sizes: [0.5, 0.5],
      first: row(),
      second: row(),
    },
  };
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
/// `ratio` is the original pane's fraction (clamped); the sibling inherits
/// its working directory but never its startup command.
export function splitPaneInTab(
  tab: Tab,
  paneId: string,
  dir: "row" | "col",
  ratio = 0.5,
): PaneNode | null {
  const sibling: PaneNode = defaultPane();
  const at = Math.min(0.9, Math.max(0.1, ratio));
  const visit = (node: LayoutNode, set: (n: LayoutNode) => void): PaneNode | null => {
    if (node.kind === "pane") {
      if (node.id !== paneId) return null;
      if (node.cwd !== undefined) sibling.cwd = node.cwd;
      set({
        kind: "split",
        id: newId("split"),
        dir,
        sizes: [at, 1 - at],
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

export type Direction = "left" | "right" | "up" | "down";

/**
 * Geometric neighbor of a pane in a direction, or null at the tab edge.
 * Picks the closest pane past that edge, preferring the largest shared
 * border on ties.
 */
export function findNeighbor(
  root: LayoutNode,
  paneId: string,
  dir: Direction,
): PaneNode | null {
  const panes = computeLayout(root).panes;
  const cur = panes.find((p) => p.node.id === paneId);
  if (!cur) return null;
  const [cx, cy, cw, ch] = cur.rect;
  const eps = 1e-9;
  let best: { node: PaneNode; gap: number; overlap: number } | null = null;
  for (const p of panes) {
    if (p.node.id === paneId) continue;
    const [x, y, w, h] = p.rect;
    let gap = -1;
    let overlap = 0;
    if (dir === "left" && x + w <= cx + eps) {
      gap = cx - (x + w);
      overlap = Math.min(y + h, cy + ch) - Math.max(y, cy);
    } else if (dir === "right" && x >= cx + cw - eps) {
      gap = x - (cx + cw);
      overlap = Math.min(y + h, cy + ch) - Math.max(y, cy);
    } else if (dir === "up" && y + h <= cy + eps) {
      gap = cy - (y + h);
      overlap = Math.min(x + w, cx + cw) - Math.max(x, cx);
    } else if (dir === "down" && y >= cy + ch - eps) {
      gap = y - (cy + ch);
      overlap = Math.min(x + w, cx + cw) - Math.max(x, cx);
    }
    if (gap < -eps || overlap <= eps) continue;
    if (
      !best ||
      gap < best.gap - eps ||
      (Math.abs(gap - best.gap) <= eps && overlap > best.overlap)
    ) {
      best = { node: p.node, gap, overlap };
    }
  }
  return best?.node ?? null;
}

/// Exchange two panes' positions in place. Ids (and their live terminals)
/// move with them; split sizes stay put. Null-safe: false unless both exist.
export function swapPanesInTab(tab: Tab, aId: string, bId: string): boolean {
  if (aId === bId) return false;
  const hits = new Map<string, { node: PaneNode; set: (n: LayoutNode) => void }>();
  const visit = (node: LayoutNode, set: (n: LayoutNode) => void): void => {
    if (node.kind === "pane") {
      if (node.id === aId || node.id === bId) hits.set(node.id, { node, set });
      return;
    }
    visit(node.first, (n) => (node.first = n));
    visit(node.second, (n) => (node.second = n));
  };
  visit(tab.root, (n) => (tab.root = n));
  const ha = hits.get(aId);
  const hb = hits.get(bId);
  if (!ha || !hb) return false;
  ha.set(hb.node);
  hb.set(ha.node);
  return true;
}

/// Grow the pane toward `dir` by `delta`, moving the divider on that side.
/// Returns false at the tab edge or without a matching-orientation split.
export function resizePaneInTab(
  tab: Tab,
  paneId: string,
  dir: Direction,
  delta: number,
): boolean {
  const horizontal = dir === "left" || dir === "right";
  const path: { split: SplitNode; side: "first" | "second" }[] = [];
  const descend = (node: LayoutNode): boolean => {
    if (node.kind === "pane") return node.id === paneId;
    path.push({ split: node, side: "first" });
    if (descend(node.first)) return true;
    path[path.length - 1].side = "second";
    if (descend(node.second)) return true;
    path.pop();
    return false;
  };
  if (!descend(tab.root)) return false;
  for (let i = path.length - 1; i >= 0; i--) {
    const { split, side } = path[i];
    const oriented =
      (split.dir === "row" && horizontal) ||
      (split.dir === "col" && !horizontal);
    if (!oriented) continue;
    const dividerOnSide =
      (dir === "right" && side === "first") ||
      (dir === "down" && side === "first") ||
      (dir === "left" && side === "second") ||
      (dir === "up" && side === "second");
    if (!dividerOnSide) continue;
    const sign = dir === "right" || dir === "down" ? 1 : -1;
    const next = Math.min(0.9, Math.max(0.1, split.sizes[0] + sign * delta));
    split.sizes = [next, 1 - next];
    return true;
  }
  return false;
}

/// Remove a pane, promoting its sibling like a close, and return the
/// removed node. The tab keeps a fresh pane root when emptied.
export function extractPane(tab: Tab, paneId: string): PaneNode | null {
  if (tab.root.kind === "pane") {
    if (tab.root.id !== paneId) return null;
    const node = tab.root;
    tab.root = defaultPane();
    return node;
  }
  const visit = (
    split: SplitNode,
    set: (n: LayoutNode) => void,
  ): PaneNode | null => {
    for (const side of ["first", "second"] as const) {
      const child = split[side];
      if (child.kind === "pane" && child.id === paneId) {
        set(side === "first" ? split.second : split.first);
        return child;
      }
      if (child.kind === "split") {
        const got = visit(child, (n) => (split[side] = n));
        if (got) return got;
      }
    }
    return null;
  };
  return visit(tab.root, (n) => (tab.root = n));
}

/// Move a pane into a new tab of the same workspace. Returns the new tab.
export function extractPaneToNewTab(
  layout: Layout,
  paneId: string,
): Tab | null {
  const found = findTabByPane(layout, paneId);
  if (!found) return null;
  const node = extractPane(found.tab, paneId);
  if (!node) return null;
  clearStaleZoom(found.tab);
  const tab = defaultTab(node.title ?? `Tab ${found.ws.tabs.length + 1}`);
  tab.root = node;
  found.ws.tabs.push(tab);
  return tab;
}

/// Move a pane into a new single-tab workspace. Returns the workspace.
export function extractPaneToNewWorkspace(
  layout: Layout,
  paneId: string,
): Workspace | null {
  const found = findTabByPane(layout, paneId);
  if (!found) return null;
  const node = extractPane(found.tab, paneId);
  if (!node) return null;
  clearStaleZoom(found.tab);
  const ws = defaultWorkspace(`Workspace ${layout.workspaces.length + 1}`);
  ws.tabs[0].root = node;
  if (node.title) ws.tabs[0].name = node.title;
  layout.workspaces.push(ws);
  return ws;
}

// --- loading ---------------------------------------------------------------

function record(v: unknown, label: string): Record<string, unknown> {
  if (typeof v !== "object" || v === null || Array.isArray(v)) {
    throw new Error(`Invalid saved layout: ${label} must be an object.`);
  }
  return v as Record<string, unknown>;
}

function text(v: unknown, label: string): string {
  if (typeof v !== "string" || v.trim().length === 0) {
    throw new Error(`Invalid saved layout: ${label} must be a nonempty string.`);
  }
  return v;
}

interface LoadContext {
  ids: Set<string>;
  entities: number;
}

function identity(v: unknown, context: LoadContext): string {
  const id = text(v, "identity");
  if (context.ids.has(id)) throw new Error(`Invalid saved layout: duplicate identity "${id}".`);
  if (++context.entities > MAX_LAYOUT_ENTITIES) {
    throw new Error(`Saved layout exceeds the ${MAX_LAYOUT_ENTITIES} entity limit.`);
  }
  context.ids.add(id);
  return id;
}

function sanitizeNode(v: unknown, context: LoadContext, depth: number): LayoutNode {
  if (depth > MAX_LAYOUT_DEPTH) {
    throw new Error(`Saved layout exceeds the ${MAX_LAYOUT_DEPTH} level tree depth limit.`);
  }
  const value = record(v, "node");
  const id = identity(value["id"], context);
  if (value["kind"] === "pane") {
    const node: PaneNode = { kind: "pane", id };
    for (const key of ["cwd", "title"] as const) {
      if (value[key] !== undefined) {
        if (typeof value[key] !== "string") throw new Error(`Invalid saved pane ${key}.`);
        node[key] = value[key];
      }
    }
    if (value["cmd"] !== undefined) {
      if (!Array.isArray(value["cmd"]) || !value["cmd"].every((part) => typeof part === "string")) {
        throw new Error("Invalid saved pane startup command.");
      }
      node.cmd = [...value["cmd"]];
    }
    if (value["cmdOnRestore"] !== undefined) {
      if (typeof value["cmdOnRestore"] !== "boolean") {
        throw new Error("Invalid saved pane command restore policy.");
      }
      node.cmdOnRestore = value["cmdOnRestore"];
    }
    return node;
  }
  if (value["kind"] !== "split" || (value["dir"] !== "row" && value["dir"] !== "col")) {
    throw new Error("Invalid saved split kind or direction.");
  }
  const sizes = value["sizes"];
  if (!Array.isArray(sizes) || sizes.length !== 2 ||
      !sizes.every((size) => typeof size === "number" && Number.isFinite(size) && size > 0)) {
    throw new Error("Invalid saved split sizes: two finite positive values are required.");
  }
  const [a, b] = sizes as [number, number];
  // Scaling first avoids overflowing a+b or underflowing both weights.
  const scale = Math.max(a, b);
  const fraction = a + b === 1 ? a : (a / scale) / (a / scale + b / scale);
  if (fraction < MIN_SPLIT_FRACTION || fraction > 1 - MIN_SPLIT_FRACTION) {
    throw new Error("Invalid saved split sizes: a pane would have degenerate geometry.");
  }
  // Preserve already-normalized values exactly for stable round trips.
  const normalized: [number, number] = a + b === 1 ? [a, b] : [fraction, 1 - fraction];
  return {
    kind: "split",
    id,
    dir: value["dir"],
    sizes: normalized,
    first: sanitizeNode(value["first"], context, depth + 1),
    second: sanitizeNode(value["second"], context, depth + 1),
  };
}

function sanitizeTab(v: unknown, context: LoadContext): Tab {
  const value = record(v, "tab");
  const tab: Tab = {
    id: identity(value["id"], context),
    name: text(value["name"], "tab name"),
    root: sanitizeNode(value["root"], context, 1),
  };
  if (value["zoomedPaneId"] !== undefined) {
    const zoom = text(value["zoomedPaneId"], "zoomed pane identity");
    if (!findPane(tab.root, zoom)) throw new Error("Invalid saved layout: zoomed pane is missing.");
    tab.zoomedPaneId = zoom;
  }
  return tab;
}

function sanitizeWorkspace(v: unknown, context: LoadContext): Workspace {
  const value = record(v, "workspace");
  const id = identity(value["id"], context);
  const rawTabs = value["tabs"];
  if (!Array.isArray(rawTabs) || rawTabs.length === 0 || rawTabs.length > MAX_LAYOUT_ENTITIES) {
    throw new Error("Invalid saved layout: workspace must have a bounded, nonempty tab list.");
  }
  const tabs = rawTabs.map((tab) => sanitizeTab(tab, context));
  const activeTabId = text(value["activeTabId"], "active tab identity");
  if (!tabs.some((tab) => tab.id === activeTabId)) {
    throw new Error("Invalid saved layout: active tab is missing.");
  }
  return { id, name: text(value["name"], "workspace name"), tabs, activeTabId };
}

/** Validate before rendering; unsafe repairs require explicit recovery, never a fresh fallback. */
export function sanitizeLayout(v: unknown): Layout {
  const value = record(v, "document");
  if (value["version"] !== 1 && value["version"] !== LAYOUT_VERSION) {
    throw new Error(`Unsupported saved layout version: ${String(value["version"])}.`);
  }
  const rawWorkspaces = value["workspaces"];
  if (!Array.isArray(rawWorkspaces) || rawWorkspaces.length === 0 ||
      rawWorkspaces.length > MAX_LAYOUT_ENTITIES) {
    throw new Error("Invalid saved layout: a bounded, nonempty workspace list is required.");
  }
  const context: LoadContext = { ids: new Set(), entities: 0 };
  const workspaces = rawWorkspaces.map((workspace) => sanitizeWorkspace(workspace, context));
  const activeWorkspaceId = text(value["activeWorkspaceId"], "active workspace identity");
  if (!workspaces.some((workspace) => workspace.id === activeWorkspaceId)) {
    throw new Error("Invalid saved layout: active workspace is missing.");
  }
  let bytes: number;
  try {
    bytes = new TextEncoder().encode(JSON.stringify(v)).byteLength;
  } catch {
    throw new Error("Invalid saved layout: document cannot be serialized safely.");
  }
  if (bytes > MAX_LAYOUT_BYTES) throw new Error(`Saved layout exceeds the ${MAX_LAYOUT_BYTES} byte limit.`);
  return { version: LAYOUT_VERSION, workspaces, activeWorkspaceId };
}

/** All pane node ids in a tree. */
export function collectPaneIds(node: LayoutNode): string[] {
  if (node.kind === "pane") return [node.id];
  return [...collectPaneIds(node.first), ...collectPaneIds(node.second)];
}

/** Agent states by live pane id, as emitted by the backend poller. */
export type AgentSnapshot = Record<
  string,
  { state: string; agent?: string; cli?: string; cwd?: string }
>;

/**
 * Last path segment for display: "/a/b" and "/a/b/" give "b", "/" gives
 * "/", "" gives "". Handles both separators for Windows paths.
 */
export function baseName(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  if (trimmed === "") return path === "" ? "" : "/";
  const parts = trimmed.split(/[\\/]/);
  return parts[parts.length - 1];
}

/**
 * Move a workspace to before/after another workspace, in place.
 * Returns false for unknown ids, self-drops, or no-op adjacent moves.
 */
export function moveWorkspace(
  layout: Layout,
  sourceId: string,
  targetId: string,
  position: "before" | "after" = "before",
): boolean {
  if (sourceId === targetId) return false;
  const from = layout.workspaces.findIndex((w) => w.id === sourceId);
  const to = layout.workspaces.findIndex((w) => w.id === targetId);
  if (from < 0 || to < 0) return false;
  if (position === "before" && from + 1 === to) return false;
  if (position === "after" && from === to + 1) return false;
  const [ws] = layout.workspaces.splice(from, 1);
  let insert = layout.workspaces.findIndex((w) => w.id === targetId);
  if (insert < 0) {
    layout.workspaces.splice(from, 0, ws);
    return false;
  }
  if (position === "after") insert += 1;
  layout.workspaces.splice(insert, 0, ws);
  return true;
}

/** Live-agent states: the pane hosts a running, suspended, or starting agent. */
export function isActiveAgentState(state: string | undefined): boolean {
  return state === "working" || state === "blocked" || state === "unknown";
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

/**
 * Pane under canvas-fraction point (fx, fy), or null outside the canvas.
 * Shared tile edges resolve to the earlier pane in traversal order, so
 * results are deterministic for drag-and-drop hit-testing.
 */
export function findPaneAtPoint(
  root: LayoutNode,
  fx: number,
  fy: number,
): PaneNode | null {
  if (!Number.isFinite(fx) || !Number.isFinite(fy)) return null;
  for (const p of computeLayout(root).panes) {
    const [x, y, w, h] = p.rect;
    if (fx >= x && fx <= x + w && fy >= y && fy <= y + h) return p.node;
  }
  return null;
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
