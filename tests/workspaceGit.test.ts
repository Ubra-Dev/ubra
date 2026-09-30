import { describe, it } from "node:test";
import assert from "node:assert/strict";
import type { LayoutNode, Tab, Workspace } from "../src/lib/layout.ts";
import { workspaceDir } from "../src/lib/workspaceGit.ts";

function pane(id: string, cwd?: string): LayoutNode {
  return cwd === undefined
    ? { kind: "pane", id }
    : { kind: "pane", id, cwd };
}

function tab(id: string, root: LayoutNode): Tab {
  return { id, name: id, root };
}

function workspace(tabs: Tab[]): Workspace {
  return { id: "ws", name: "ws", tabs, activeTabId: tabs[0]?.id ?? "" };
}

describe("workspaceDir", () => {
  it("returns the first pane cwd in tab order", () => {
    const ws = workspace([
      tab("t1", pane("p1", "/repo-a")),
      tab("t2", pane("p2", "/repo-b")),
    ]);
    assert.equal(workspaceDir(ws), "/repo-a");
  });

  it("descends splits left-first and skips panes without cwd", () => {
    const ws = workspace([
      tab("t1", {
        kind: "split", id: "s", dir: "row", sizes: [0.5, 0.5],
        first: pane("p1"),
        second: {
          kind: "split", id: "s2", dir: "col", sizes: [0.5, 0.5],
          first: pane("p2", "/nested"),
          second: pane("p3", "/other"),
        },
      }),
    ]);
    assert.equal(workspaceDir(ws), "/nested");
  });

  it("skips blank cwds", () => {
    const ws = workspace([
      tab("t1", pane("p1", "   ")),
      tab("t2", pane("p2", "/real")),
    ]);
    assert.equal(workspaceDir(ws), "/real");
  });

  it("returns null when no pane has a cwd", () => {
    assert.equal(workspaceDir(workspace([tab("t1", pane("p1"))])), null);
    assert.equal(workspaceDir(workspace([])), null);
  });
});
