import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  baseName,
  clearStaleZoom,
  closePaneInTab,
  collectPaneIds,
  computeLayout,
  countPanes,
  defaultLayout,
  defaultTab,
  defaultWorkspace,
  extractPane,
  extractPaneToNewTab,
  extractPaneToNewWorkspace,
  findNeighbor,
  findPane,
  findPaneAtPoint,
  findSplit,
  findTabByPane,
  gridTab,
  isActiveAgentState,
  moveWorkspace,
  resizePaneInTab,
  MAX_LAYOUT_BYTES,
  MAX_LAYOUT_DEPTH,
  MAX_LAYOUT_ENTITIES,
  newId,
  sanitizeLayout,
  swapPanesInTab,
  setZoomedPane,
  splitPaneInTab,
  type Tab,
} from "../src/lib/layout.ts";

function rootPaneId(tab: Tab): string {
  assert.equal(tab.root.kind, "pane");
  return tab.root.id;
}

describe("defaultLayout", () => {
  it("creates one workspace with one tab and one pane", () => {
    const layout = defaultLayout();
    assert.deepEqual(sanitizeLayout(JSON.parse(JSON.stringify(layout))), layout);
    assert.equal(layout.workspaces.length, 1);
    assert.equal(layout.activeWorkspaceId, layout.workspaces[0].id);
    assert.equal(layout.workspaces[0].tabs.length, 1);
    assert.equal(countPanes(layout.workspaces[0].tabs[0].root), 1);
  });
});

describe("gridTab", () => {
  it("creates four distinct terminals in equal 2×2 quadrants", () => {
    const tab = gridTab();
    assert.equal(tab.name, "Tab 1");
    assert.equal(countPanes(tab.root), 4);
    const panes = computeLayout(tab.root).panes;
    assert.equal(new Set(panes.map((p) => p.node.id)).size, 4);
    assert.deepEqual(panes.map((p) => p.rect), [
      [0, 0, 0.5, 0.5],
      [0.5, 0, 0.5, 0.5],
      [0, 0.5, 0.5, 0.5],
      [0.5, 0.5, 0.5, 0.5],
    ]);
  });

  it("preserves the grid when saving and restoring a layout", () => {
    const layout = defaultLayout();
    const tab = gridTab("Grid");
    layout.workspaces[0].tabs = [tab];
    layout.workspaces[0].activeTabId = tab.id;
    assert.deepEqual(sanitizeLayout(JSON.parse(JSON.stringify(layout))), layout);
  });
});

describe("splitPaneInTab", () => {
  it("splits a root pane into two", () => {
    const tab = defaultTab();
    const sibling = splitPaneInTab(tab, rootPaneId(tab), "row");
    assert.ok(sibling);
    assert.equal(tab.root.kind, "split");
    assert.equal(countPanes(tab.root), 2);
  });

  it("splits nested panes", () => {
    const tab = defaultTab();
    const first = splitPaneInTab(tab, rootPaneId(tab), "row");
    assert.ok(first);
    const second = splitPaneInTab(tab, first.id, "col");
    assert.ok(second);
    assert.equal(countPanes(tab.root), 3);
  });

  it("returns null for unknown panes", () => {
    const tab = defaultTab();
    assert.equal(splitPaneInTab(tab, "nope", "row"), null);
    assert.equal(countPanes(tab.root), 1);
  });

  it("keeps the original pane id as first child (survivor stability)", () => {
    // The canvas renders panes in a block keyed by node id; the surviving
    // pane must keep its id across the split or its live PTY is remounted
    // (killed) instead of preserved.
    const tab = defaultTab();
    const survivor = rootPaneId(tab);
    const sibling = splitPaneInTab(tab, survivor, "row");
    assert.ok(sibling);
    assert.notEqual(sibling.id, survivor);
    assert.equal(tab.root.kind, "split");
    if (tab.root.kind !== "split") return;
    assert.equal(tab.root.first.kind, "pane");
    if (tab.root.first.kind !== "pane") return;
    assert.equal(tab.root.first.id, survivor);
  });
});

describe("closePaneInTab", () => {
  it("collapses the parent split", () => {
    const tab = defaultTab();
    const victim = rootPaneId(tab);
    const sibling = splitPaneInTab(tab, victim, "row");
    assert.ok(sibling);
    assert.equal(closePaneInTab(tab, victim), true);
    assert.equal(countPanes(tab.root), 1);
    assert.equal(tab.root.kind, "pane");
    assert.equal(tab.root.id, sibling.id);
  });

  it("respawns a fresh pane when closing the last one", () => {
    const tab = defaultTab();
    const oldId = rootPaneId(tab);
    assert.equal(closePaneInTab(tab, oldId), true);
    assert.equal(countPanes(tab.root), 1);
    assert.notEqual(rootPaneId(tab), oldId);
  });

  it("returns false for unknown panes", () => {
    const tab = defaultTab();
    assert.equal(closePaneInTab(tab, "nope"), false);
  });
});

describe("findTabByPane", () => {
  it("locates panes and misses unknown ids", () => {
    const layout = defaultLayout();
    const tab = layout.workspaces[0].tabs[0];
    const found = findTabByPane(layout, rootPaneId(tab));
    assert.ok(found);
    assert.equal(found.tab.id, tab.id);
    assert.equal(findTabByPane(layout, "missing"), null);
  });
});

describe("sanitizeLayout", () => {
  it("rejects corrupt and unsupported documents instead of starting fresh", () => {
    for (const bad of [null, 42, "x", {}, { version: 999 }, { version: 1 }]) {
      assert.throws(() => sanitizeLayout(bad));
    }
  });

  it("round-trips valid active and zoom references without changing identities", () => {
    const layout = defaultLayout();
    const workspace = defaultWorkspace("Second");
    const tab = gridTab("Selected");
    workspace.tabs.push(tab);
    workspace.activeTabId = tab.id;
    tab.zoomedPaneId = collectPaneIds(tab.root)[2];
    layout.workspaces.push(workspace);
    layout.activeWorkspaceId = workspace.id;
    assert.deepEqual(sanitizeLayout(JSON.parse(JSON.stringify(layout))), layout);
  });

  it("rejects globally duplicated identities including cross-workspace panes", () => {
    for (const category of ["pane", "tab", "workspace", "cross-kind"]) {
      const layout = defaultLayout();
      const first = layout.workspaces[0];
      const second = defaultWorkspace("Second");
      layout.workspaces.push(second);
      if (category === "pane") second.tabs[0].root.id = first.tabs[0].root.id;
      if (category === "tab") second.tabs[0].id = first.tabs[0].id;
      if (category === "workspace") second.id = first.id;
      if (category === "cross-kind") second.tabs[0].root.id = first.id;
      assert.throws(() => sanitizeLayout(layout), /duplicate identity/);
    }
  });

  it("rejects empty node identities and missing active references", () => {
    const layout = defaultLayout();
    layout.workspaces[0].tabs[0].root.id = " ";
    assert.throws(() => sanitizeLayout(layout), /nonempty/);
    layout.workspaces[0].tabs[0].root.id = "p";
    layout.workspaces[0].activeTabId = "missing";
    assert.throws(() => sanitizeLayout(layout), /active tab/);
    layout.workspaces[0].activeTabId = layout.workspaces[0].tabs[0].id;
    layout.activeWorkspaceId = "missing";
    assert.throws(() => sanitizeLayout(layout), /active workspace/);
  });

  it("normalizes extreme finite weights without overflow or lost geometry", () => {
    const layout = defaultLayout();
    const tab = gridTab();
    layout.workspaces[0].tabs = [tab];
    layout.workspaces[0].activeTabId = tab.id;
    if (tab.root.kind !== "split") throw new Error("Expected split");
    tab.root.sizes = [1e308, 1e308];
    const loaded = sanitizeLayout(layout).workspaces[0].tabs[0].root;
    if (loaded.kind !== "split") throw new Error("Expected split");
    assert.deepEqual(loaded.sizes, [0.5, 0.5]);
    assert.deepEqual(computeLayout(loaded).panes.map((pane) => pane.rect), [
      [0, 0, 0.5, 0.5], [0.5, 0, 0.5, 0.5],
      [0, 0.5, 0.5, 0.5], [0.5, 0.5, 0.5, 0.5],
    ]);
  });

  it("keeps representable geometry at both ratio boundaries and subnormal weights", () => {
    const layout = defaultLayout();
    const tab = gridTab();
    layout.workspaces[0].tabs = [tab];
    layout.workspaces[0].activeTabId = tab.id;
    if (tab.root.kind !== "split") throw new Error("Expected split");
    for (const sizes of [[0.05, 0.95], [0.95, 0.05], [Number.MIN_VALUE, Number.MIN_VALUE]]) {
      tab.root.sizes = sizes as [number, number];
      const loaded = sanitizeLayout(layout).workspaces[0].tabs[0].root;
      if (loaded.kind !== "split") throw new Error("Expected split");
      assert.deepEqual(loaded.sizes, sizes[0] === Number.MIN_VALUE ? [0.5, 0.5] : sizes);
    }
  });

  it("rejects nonfinite, nonpositive and degenerate weights", () => {
    const layout = defaultLayout();
    const tab = gridTab();
    layout.workspaces[0].tabs = [tab];
    layout.workspaces[0].activeTabId = tab.id;
    if (tab.root.kind !== "split") throw new Error("Expected split");
    for (const sizes of [[Infinity, 1], [NaN, 1], [0, 1], [-1, 1], [1e308, 1e-308]]) {
      tab.root.sizes = sizes as [number, number];
      assert.throws(() => sanitizeLayout(layout), /split sizes/);
    }
  });

  it("rejects documents above byte, depth and global entity limits", () => {
    const large = defaultLayout();
    if (large.workspaces[0].tabs[0].root.kind !== "pane") throw new Error("Expected pane");
    large.workspaces[0].tabs[0].root.cwd = "x".repeat(MAX_LAYOUT_BYTES);
    assert.throws(() => sanitizeLayout(large), /byte limit/);
    const deep = defaultLayout();
    const tab = deep.workspaces[0].tabs[0];
    for (let i = 0; i < MAX_LAYOUT_DEPTH; i++) {
      tab.root = {
        kind: "split", id: `split-${i}`, dir: "row", sizes: [0.5, 0.5],
        first: tab.root, second: { kind: "pane", id: `side-${i}` },
      };
    }
    assert.throws(() => sanitizeLayout(deep), /depth limit/);
    const crowded = defaultLayout();
    for (let i = 0; i < Math.ceil(MAX_LAYOUT_ENTITIES / 3); i++) {
      crowded.workspaces.push(defaultWorkspace());
    }
    assert.throws(() => sanitizeLayout(crowded), /entity limit/);
  });

  it("accepts the maximum tree depth and entity count without recursive overflow", () => {
    const layout = defaultLayout();
    const tab = layout.workspaces[0].tabs[0];
    for (let i = 1; i < MAX_LAYOUT_DEPTH; i++) {
      tab.root = {
        kind: "split", id: `split-${i}`, dir: "row", sizes: [0.5, 0.5],
        first: tab.root, second: { kind: "pane", id: `side-${i}` },
      };
    }
    assert.deepEqual(sanitizeLayout(layout), layout);
    const crowded = defaultLayout();
    crowded.workspaces.push(defaultWorkspace("Second"));
    // Two workspaces/tabs/panes use six entities; each added tab/pane uses two.
    for (let i = 0; i < (MAX_LAYOUT_ENTITIES - 6) / 2; i++) {
      crowded.workspaces[0].tabs.push(defaultTab());
    }
    assert.deepEqual(sanitizeLayout(crowded), crowded);
  });

  it("retains full UUIDs for lifetime identity generation", () => {
    assert.match(newId("pane"), /^pane-[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
  });

  it("preserves startup commands", () => {
    const layout = sanitizeLayout({
      version: 1,
      activeWorkspaceId: "w",
      workspaces: [
        {
          id: "w",
          name: "w",
          activeTabId: "t",
          tabs: [
            {
              id: "t",
              name: "t",
              root: { kind: "pane", id: "p", cmd: ["claude", "--resume"] },
            },
          ],
        },
      ],
    });
    const root = layout.workspaces[0].tabs[0].root;
    assert.equal(root.kind, "pane");
    if (root.kind === "pane") assert.deepEqual(root.cmd, ["claude", "--resume"]);
  });
});

describe("collectPaneIds", () => {
  it("lists every pane in a split tree", () => {
    const tab = defaultTab();
    assert.equal(collectPaneIds(tab.root).length, 1);
    splitPaneInTab(tab, rootPaneId(tab), "row");
    assert.equal(collectPaneIds(tab.root).length, 2);
  });
});

describe("isActiveAgentState", () => {
  it("treats working, blocked, and unknown as active", () => {
    assert.equal(isActiveAgentState("working"), true);
    assert.equal(isActiveAgentState("blocked"), true);
    assert.equal(isActiveAgentState("unknown"), true);
    assert.equal(isActiveAgentState("idle"), false);
    assert.equal(isActiveAgentState("done"), false);
    assert.equal(isActiveAgentState(undefined), false);
    assert.equal(isActiveAgentState("bogus"), false);
  });
});

describe("findNeighbor", () => {
  it("finds geometric neighbors in split layouts", () => {
    const tab = defaultTab();
    const firstId = rootPaneId(tab);
    const sibling = splitPaneInTab(tab, firstId, "row");
    assert.ok(sibling);
    assert.equal(findNeighbor(tab.root, firstId, "right")?.id, sibling.id);
    assert.equal(findNeighbor(tab.root, sibling.id, "left")?.id, firstId);
    assert.equal(findNeighbor(tab.root, firstId, "left"), null);
    assert.equal(findNeighbor(tab.root, firstId, "up"), null);
    assert.equal(findNeighbor(tab.root, sibling.id, "down"), null);
    assert.equal(findNeighbor(tab.root, "nope", "right"), null);
  });

  it("picks the closest overlapping pane in nested layouts", () => {
    const tab = defaultTab();
    const a = rootPaneId(tab);
    const right = splitPaneInTab(tab, a, "row")!;
    const c = splitPaneInTab(tab, right.id, "col")!;
    // Layout: [A | B(top) / C(bottom)].
    assert.equal(findNeighbor(tab.root, a, "right")?.id, right.id);
    assert.equal(findNeighbor(tab.root, right.id, "down")?.id, c.id);
    assert.equal(findNeighbor(tab.root, c.id, "up")?.id, right.id);
    assert.equal(findNeighbor(tab.root, c.id, "left")?.id, a);
    assert.equal(findNeighbor(tab.root, a, "left"), null);
  });
});

describe("swapPanesInTab", () => {
  it("exchanges pane positions, keeping ids and terminals", () => {
    const tab = defaultTab();
    const a = rootPaneId(tab);
    const b = splitPaneInTab(tab, a, "row")!;
    findPane(tab.root, a)!.title = "A";
    findPane(tab.root, b.id)!.title = "B";
    assert.equal(swapPanesInTab(tab, a, b.id), true);
    const panes = computeLayout(tab.root).panes;
    const left = panes.find((p) => p.rect[0] === 0)!;
    assert.equal(left.node.id, b.id);
    assert.equal(left.node.title, "B");
    assert.equal(swapPanesInTab(tab, a, a), false);
    assert.equal(swapPanesInTab(tab, a, "nope"), false);
  });
});

describe("findPaneAtPoint", () => {
  it("hits panes in a row split", () => {
    const tab = defaultTab();
    const a = rootPaneId(tab);
    const b = splitPaneInTab(tab, a, "row")!;
    assert.equal(findPaneAtPoint(tab.root, 0.25, 0.5)?.id, a);
    assert.equal(findPaneAtPoint(tab.root, 0.75, 0.5)?.id, b.id);
  });

  it("hits panes in nested splits", () => {
    const tab = defaultTab();
    const a = rootPaneId(tab);
    const right = splitPaneInTab(tab, a, "row")!;
    const c = splitPaneInTab(tab, right.id, "col")!;
    // Layout: [A | B(top) / C(bottom)].
    assert.equal(findPaneAtPoint(tab.root, 0.1, 0.1)?.id, a);
    assert.equal(findPaneAtPoint(tab.root, 0.9, 0.1)?.id, right.id);
    assert.equal(findPaneAtPoint(tab.root, 0.9, 0.9)?.id, c.id);
  });

  it("resolves shared edges to the earlier pane", () => {
    const tab = defaultTab();
    const a = rootPaneId(tab);
    splitPaneInTab(tab, a, "row");
    assert.equal(findPaneAtPoint(tab.root, 0.5, 0.5)?.id, a);
  });

  it("returns null outside the canvas", () => {
    const tab = defaultTab();
    assert.equal(findPaneAtPoint(tab.root, -0.1, 0.5), null);
    assert.equal(findPaneAtPoint(tab.root, 0.5, 1.1), null);
    assert.equal(findPaneAtPoint(tab.root, Number.NaN, 0.5), null);
  });
});

describe("resizePaneInTab", () => {
  it("grows the pane toward the divider side", () => {
    const tab = defaultTab();
    const a = rootPaneId(tab);
    const b = splitPaneInTab(tab, a, "row")!;
    assert.equal(resizePaneInTab(tab, a, "right", 0.25), true);
    assert.equal(tab.root.kind, "split");
    if (tab.root.kind === "split")
      assert.deepEqual(tab.root.sizes, [0.75, 0.25]);
    assert.equal(resizePaneInTab(tab, b.id, "left", 0.25), true);
    if (tab.root.kind === "split")
      assert.deepEqual(tab.root.sizes, [0.5, 0.5]);
  });

  it("returns false at edges and clamps to bounds", () => {
    const tab = defaultTab();
    const a = rootPaneId(tab);
    assert.equal(resizePaneInTab(tab, a, "right", 0.1), false);
    splitPaneInTab(tab, a, "row");
    assert.equal(resizePaneInTab(tab, a, "left", 0.1), false);
    assert.equal(resizePaneInTab(tab, a, "up", 0.1), false);
    assert.equal(resizePaneInTab(tab, "nope", "right", 0.1), false);
    resizePaneInTab(tab, a, "right", 5);
    assert.equal(tab.root.kind, "split");
    if (tab.root.kind === "split") assert.equal(tab.root.sizes[0], 0.9);
  });
});

describe("extractPane", () => {
  it("removes the pane and promotes its sibling", () => {
    const tab = defaultTab();
    const a = rootPaneId(tab);
    const b = splitPaneInTab(tab, a, "row")!;
    const got = extractPane(tab, b.id);
    assert.equal(got?.id, b.id);
    assert.equal(tab.root.kind, "pane");
    if (tab.root.kind === "pane") assert.equal(tab.root.id, a);
    assert.equal(extractPane(tab, "nope"), null);
  });

  it("leaves a fresh pane when extracting the last one", () => {
    const tab = defaultTab();
    const a = rootPaneId(tab);
    const got = extractPane(tab, a);
    assert.equal(got?.id, a);
    assert.equal(tab.root.kind, "pane");
    if (tab.root.kind === "pane") assert.notEqual(tab.root.id, a);
  });
});

describe("extractPaneToNewTab", () => {
  it("moves the pane node into a new tab", () => {
    const layout = defaultLayout();
    const ws = layout.workspaces[0];
    const tab = ws.tabs[0];
    const a = rootPaneId(tab);
    splitPaneInTab(tab, a, "row");
    const moved = extractPaneToNewTab(layout, a);
    assert.ok(moved);
    assert.equal(ws.tabs.length, 2);
    assert.equal(moved.root.kind, "pane");
    if (moved.root.kind === "pane") assert.equal(moved.root.id, a);
    assert.equal(countPanes(tab.root), 1);
    assert.equal(extractPaneToNewTab(layout, "nope"), null);
  });

  it("clears zoom pointing at the moved pane", () => {
    const layout = defaultLayout();
    const tab = layout.workspaces[0].tabs[0];
    const a = rootPaneId(tab);
    splitPaneInTab(tab, a, "row");
    setZoomedPane(tab, a);
    extractPaneToNewTab(layout, a);
    assert.equal(tab.zoomedPaneId, undefined);
  });
});

describe("extractPaneToNewWorkspace", () => {
  it("moves the pane node into a new workspace", () => {
    const layout = defaultLayout();
    const a = rootPaneId(layout.workspaces[0].tabs[0]);
    const ws = extractPaneToNewWorkspace(layout, a);
    assert.ok(ws);
    assert.equal(layout.workspaces.length, 2);
    assert.equal(countPanes(ws.tabs[0].root), 1);
    const src = layout.workspaces[0].tabs[0];
    assert.equal(src.root.kind, "pane");
    if (src.root.kind === "pane") assert.notEqual(src.root.id, a);
    assert.equal(extractPaneToNewWorkspace(layout, "nope"), null);
  });
});

describe("splitPaneInTab options", () => {
  it("honors ratio and inherits cwd but not cmd", () => {
    const tab = defaultTab();
    const a = rootPaneId(tab);
    const node = findPane(tab.root, a)!;
    node.cwd = "/tmp/proj";
    node.cmd = ["claude"];
    const sib = splitPaneInTab(tab, a, "row", 0.75)!;
    assert.equal(sib.cwd, "/tmp/proj");
    assert.equal(sib.cmd, undefined);
    assert.equal(tab.root.kind, "split");
    if (tab.root.kind !== "split") assert.fail("expected split");
    assert.deepEqual(tab.root.sizes, [0.75, 0.25]);
    splitPaneInTab(tab, sib.id, "col", 0.99);
    const inner = tab.root.second;
    assert.equal(inner.kind, "split");
    if (inner.kind === "split") assert.equal(inner.sizes[0], 0.9);
  });
});

describe("computeLayout", () => {
  it("places a single pane over the full canvas", () => {
    const tab = defaultTab();
    const layout = computeLayout(tab.root);
    assert.equal(layout.panes.length, 1);
    assert.deepEqual(layout.panes[0].rect, [0, 0, 1, 1]);
    assert.equal(layout.dividers.length, 0);
  });

  it("splits rows into halves with a divider", () => {
    const tab = defaultTab();
    const firstId = rootPaneId(tab);
    const sibling = splitPaneInTab(tab, firstId, "row");
    assert.ok(sibling);
    const layout = computeLayout(tab.root);
    assert.equal(layout.panes.length, 2);
    assert.deepEqual(layout.panes[0].rect, [0, 0, 0.5, 1]);
    assert.deepEqual(layout.panes[1].rect, [0.5, 0, 0.5, 1]);
    assert.equal(layout.panes[0].node.id, firstId);
    assert.equal(layout.panes[1].node.id, sibling.id);
    assert.equal(layout.dividers.length, 1);
    assert.equal(layout.dividers[0].dir, "row");
    assert.equal(layout.dividers[0].at, 0.5);
  });

  it("respects custom sizes and nesting", () => {
    const tab = defaultTab();
    const firstId = rootPaneId(tab);
    const sibling = splitPaneInTab(tab, firstId, "row");
    assert.ok(sibling);
    assert.equal(tab.root.kind, "split");
    if (tab.root.kind !== "split") return;
    tab.root.sizes = [0.25, 0.75];
    const nested = splitPaneInTab(tab, sibling.id, "col");
    assert.ok(nested);
    const layout = computeLayout(tab.root);
    assert.equal(layout.panes.length, 3);
    assert.equal(layout.dividers.length, 2);
    assert.deepEqual(layout.panes[0].rect, [0, 0, 0.25, 1]);
    // Right column split stacked: y halves of full height.
    assert.deepEqual(layout.panes[1].rect, [0.25, 0, 0.75, 0.5]);
    assert.deepEqual(layout.panes[2].rect, [0.25, 0.5, 0.75, 0.5]);
  });
});

describe("baseName", () => {
  it("returns the last path segment", () => {
    assert.equal(baseName("/Users/me/herdr"), "herdr");
    assert.equal(baseName("/Users/me/herdr/"), "herdr");
    assert.equal(baseName("herdr"), "herdr");
    assert.equal(baseName("C:\\Users\\me\\herdr"), "herdr");
    assert.equal(baseName("C:\\Users\\me\\herdr\\"), "herdr");
    assert.equal(baseName("/"), "/");
    assert.equal(baseName(""), "");
  });
});

describe("findSplit", () => {
  it("locates splits and misses panes and unknown ids", () => {
    const tab = defaultTab();
    assert.equal(findSplit(tab.root, "nope"), null);
    const sibling = splitPaneInTab(tab, rootPaneId(tab), "row");
    assert.ok(sibling);
    assert.equal(tab.root.kind, "split");
    if (tab.root.kind !== "split") return;
    assert.equal(findSplit(tab.root, tab.root.id)?.id, tab.root.id);
    assert.equal(findSplit(tab.root, sibling.id), null);
  });
});

describe("findPane", () => {
  it("locates nested panes and misses splits and unknown ids", () => {
    const tab = defaultTab();
    const firstId = rootPaneId(tab);
    assert.equal(findPane(tab.root, firstId)?.id, firstId);
    assert.equal(findPane(tab.root, "nope"), null);
    const sibling = splitPaneInTab(tab, firstId, "row");
    assert.ok(sibling);
    assert.equal(findPane(tab.root, firstId)?.id, firstId);
    assert.equal(findPane(tab.root, sibling.id)?.id, sibling.id);
    assert.equal(tab.root.kind, "split");
    if (tab.root.kind !== "split") return;
    assert.equal(findPane(tab.root, tab.root.id), null);
  });
});

describe("zoom", () => {
  it("zooms a live pane and clears on null or unknown ids", () => {
    const tab = defaultTab();
    const firstId = rootPaneId(tab);
    const sibling = splitPaneInTab(tab, firstId, "row");
    assert.ok(sibling);
    setZoomedPane(tab, sibling.id);
    assert.equal(tab.zoomedPaneId, sibling.id);
    setZoomedPane(tab, "nope");
    assert.equal(tab.zoomedPaneId, undefined);
    setZoomedPane(tab, firstId);
    assert.equal(tab.zoomedPaneId, firstId);
    setZoomedPane(tab, null);
    assert.equal(tab.zoomedPaneId, undefined);
  });

  it("drops stale zooms pointing at closed panes", () => {
    const tab = defaultTab();
    const victim = rootPaneId(tab);
    const sibling = splitPaneInTab(tab, victim, "row");
    assert.ok(sibling);
    setZoomedPane(tab, victim);
    assert.equal(closePaneInTab(tab, victim), true);
    clearStaleZoom(tab);
    assert.equal(tab.zoomedPaneId, undefined);
    setZoomedPane(tab, sibling.id);
    clearStaleZoom(tab);
    assert.equal(tab.zoomedPaneId, sibling.id);
  });

  it("sanitize keeps valid zooms and rejects missing targets for recovery", () => {
    const tab = defaultTab();
    const firstId = rootPaneId(tab);
    const sibling = splitPaneInTab(tab, firstId, "row");
    assert.ok(sibling);
    const layout = defaultLayout();
    layout.workspaces[0].tabs = [tab];
    layout.workspaces[0].activeTabId = tab.id;
    setZoomedPane(tab, sibling.id);
    const good = sanitizeLayout(JSON.parse(JSON.stringify(layout)));
    assert.equal(good.workspaces[0].tabs[0].zoomedPaneId, sibling.id);
    tab.zoomedPaneId = "ghost";
    assert.throws(() => sanitizeLayout(JSON.parse(JSON.stringify(layout))), /zoomed pane/);
  });
});

describe("moveWorkspace", () => {
  function three() {
    const layout = defaultLayout();
    const a = layout.workspaces[0];
    const b = defaultWorkspace("B");
    const c = defaultWorkspace("C");
    layout.workspaces.push(b, c);
    return { layout, ids: [a.id, b.id, c.id] };
  }

  function order(layout: { workspaces: { id: string }[] }): string[] {
    return layout.workspaces.map((w) => w.id);
  }

  it("moves a workspace before another", () => {
    const { layout, ids } = three();
    assert.equal(moveWorkspace(layout, ids[2], ids[0], "before"), true);
    assert.deepEqual(order(layout), [ids[2], ids[0], ids[1]]);
  });

  it("moves a workspace after another", () => {
    const { layout, ids } = three();
    assert.equal(moveWorkspace(layout, ids[0], ids[2], "after"), true);
    assert.deepEqual(order(layout), [ids[1], ids[2], ids[0]]);
  });

  it("keeps the active workspace selected by id", () => {
    const { layout, ids } = three();
    layout.activeWorkspaceId = ids[0];
    assert.equal(moveWorkspace(layout, ids[0], ids[2], "after"), true);
    assert.equal(layout.activeWorkspaceId, ids[0]);
    assert.deepEqual(order(layout), [ids[1], ids[2], ids[0]]);
  });

  it("rejects unknown ids, self-drops, and no-op adjacent moves", () => {
    const { layout, ids } = three();
    assert.equal(moveWorkspace(layout, "nope", ids[0]), false);
    assert.equal(moveWorkspace(layout, ids[0], "nope"), false);
    assert.equal(moveWorkspace(layout, ids[0], ids[0]), false);
    assert.equal(moveWorkspace(layout, ids[0], ids[1], "before"), false);
    assert.equal(moveWorkspace(layout, ids[1], ids[0], "after"), false);
    assert.deepEqual(order(layout), ids);
  });
});

describe("explicit launch policy", () => {
  it("migrates a valid version-1 layout to version 2 without losing commands or references", () => {
    const tab = gridTab("Selected");
    const ids = collectPaneIds(tab.root);
    const workspace = defaultWorkspace("Second");
    const legacyFirst = workspace.tabs[0].root;
    if (legacyFirst.kind === "pane") legacyFirst.cmd = ["claude", "--resume"];
    workspace.tabs.push(tab);
    workspace.activeTabId = tab.id;
    tab.zoomedPaneId = ids[2];
    const legacy = {
      version: 1,
      activeWorkspaceId: workspace.id,
      workspaces: [
        {
          id: workspace.id,
          name: workspace.name,
          activeTabId: workspace.activeTabId,
          tabs: workspace.tabs.map((t) => {
            const entry: Record<string, unknown> = {
              id: t.id,
              name: t.name,
              root: JSON.parse(JSON.stringify(t.root)),
            };
            if (typeof t.zoomedPaneId === "string") entry["zoomedPaneId"] = t.zoomedPaneId;
            return entry;
          }),
        },
      ],
    };
    const migrated = sanitizeLayout(JSON.parse(JSON.stringify(legacy)));
    assert.equal(migrated.version, 2);
    assert.equal(migrated.activeWorkspaceId, workspace.id);
    const firstRoot = migrated.workspaces[0].tabs[0].root;
    assert.equal(firstRoot.kind, "pane");
    if (firstRoot.kind === "pane") assert.deepEqual(firstRoot.cmd, ["claude", "--resume"]);
    const selected = migrated.workspaces[0].tabs.find((t) => t.id === tab.id);
    assert.ok(selected);
    assert.equal(selected?.zoomedPaneId, ids[2]);
  });

  it("preserves a false restore policy across a save/load round trip", () => {
    const layout = defaultLayout();
    const root = layout.workspaces[0].tabs[0].root;
    assert.equal(root.kind, "pane");
    if (root.kind !== "pane") throw new Error("Expected pane");
    root.cmd = ["node", "-e", "console.log(1)"];
    root.cmdOnRestore = false;
    const loaded = sanitizeLayout(JSON.parse(JSON.stringify({ ...layout, version: 2 })));
    const reloaded = loaded.workspaces[0].tabs[0].root;
    assert.equal(reloaded.kind, "pane");
    if (reloaded.kind === "pane") {
      assert.deepEqual(reloaded.cmd, ["node", "-e", "console.log(1)"]);
      assert.equal(reloaded.cmdOnRestore, false);
    }
  });

  it("rejects a malformed restore policy and future versions", () => {
    const raw = JSON.parse(JSON.stringify({ ...defaultLayout(), version: 2 }));
    raw.workspaces[0].tabs[0].root.cmdOnRestore = "never";
    assert.throws(() => sanitizeLayout(raw), /restore policy/);
    assert.throws(() => sanitizeLayout({ ...defaultLayout(), version: 3 }), /Unsupported/);
  });
});
