import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  clearStaleZoom,
  closePaneInTab,
  collectPaneIds,
  computeLayout,
  countPanes,
  defaultLayout,
  defaultTab,
  detectFinished,
  detectVanished,
  findPane,
  findSplit,
  findTabByPane,
  sanitizeLayout,
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
    assert.equal(layout.version, 1);
    assert.equal(layout.workspaces.length, 1);
    assert.equal(layout.activeWorkspaceId, layout.workspaces[0].id);
    assert.equal(layout.workspaces[0].tabs.length, 1);
    assert.equal(countPanes(layout.workspaces[0].tabs[0].root), 1);
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
  it("rejects garbage with a fresh default", () => {
    for (const bad of [null, 42, "x", {}, { version: 999 }, { version: 1 }]) {
      const layout = sanitizeLayout(bad);
      assert.equal(layout.version, 1);
      assert.ok(layout.workspaces.length >= 1);
      assert.ok(layout.workspaces[0].tabs.length >= 1);
    }
  });

  it("preserves a valid layout", () => {
    const layout = defaultLayout();
    layout.workspaces[0].name = "proj";
    const clone = JSON.parse(JSON.stringify(layout));
    assert.equal(sanitizeLayout(clone).workspaces[0].name, "proj");
  });

  it("repairs broken ids, dirs, and sizes", () => {
    const layout = sanitizeLayout({
      version: 1,
      activeWorkspaceId: "missing",
      workspaces: [
        {
          id: "w",
          name: "",
          tabs: [
            {
              id: "t",
              name: "T",
              root: {
                kind: "split",
                id: "s",
                dir: "diagonal",
                sizes: [0, -1],
                first: null,
                second: { kind: "pane", id: "p" },
              },
            },
          ],
          activeTabId: "nope",
        },
      ],
    });
    assert.equal(layout.activeWorkspaceId, "w");
    assert.equal(layout.workspaces[0].activeTabId, "t");
    const root = layout.workspaces[0].tabs[0].root;
    assert.equal(root.kind, "split");
    if (root.kind === "split") {
      assert.equal(root.dir, "row");
      assert.deepEqual(root.sizes, [0.5, 0.5]);
      assert.equal(root.first.kind, "pane");
      assert.equal(root.second.kind, "pane");
    }
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

describe("detectFinished", () => {
  it("reports working-to-idle transitions only", () => {
    const prev = {
      "1": { state: "working", agent: "Codex" },
      "2": { state: "working", agent: "Claude Code" },
      "3": { state: "idle" },
    };
    const next = {
      "1": { state: "idle" },
      "2": { state: "working", agent: "Claude Code" },
      "3": { state: "idle" },
      "4": { state: "working", agent: "Amp" },
    };
    assert.deepEqual(detectFinished(prev, next), ["1"]);
    assert.deepEqual(detectFinished({}, next), []);
  });
});

describe("detectVanished", () => {
  it("reports working panes missing from the next snapshot", () => {
    const prev = {
      "1": { state: "working", agent: "Codex" },
      "2": { state: "idle" },
      "3": { state: "working", agent: "Amp" },
    };
    assert.deepEqual(detectVanished(prev, { "3": { state: "working" } }), ["1"]);
    assert.deepEqual(detectVanished(prev, prev), []);
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

  it("sanitize keeps valid zooms and drops broken ones", () => {
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
    const bad = sanitizeLayout(JSON.parse(JSON.stringify(layout)));
    assert.equal(bad.workspaces[0].tabs[0].zoomedPaneId, undefined);
  });
});
