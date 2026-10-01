import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  countLayoutPanes,
  parseQuitAction,
  quitDialogCopy,
  resolveQuitRequest,
} from "../src/lib/quitConfirm.ts";
import { defaultLayout } from "../src/lib/layout.ts";

describe("parseQuitAction", () => {
  it("accepts keep and stop, defaults everything else to ask", () => {
    assert.equal(parseQuitAction("keep"), "keep");
    assert.equal(parseQuitAction("stop"), "stop");
    assert.equal(parseQuitAction("ask"), "ask");
    assert.equal(parseQuitAction(null), "ask");
    assert.equal(parseQuitAction(""), "ask");
    assert.equal(parseQuitAction("bogus"), "ask");
  });
});

describe("countLayoutPanes", () => {
  it("counts zero without a layout", () => {
    assert.equal(countLayoutPanes(null), 0);
  });

  it("counts panes across workspaces and tabs", () => {
    const layout = defaultLayout();
    assert.equal(countLayoutPanes(layout), 1);
    layout.workspaces[0].tabs.push({
      ...layout.workspaces[0].tabs[0],
      id: "tab-2",
      root: {
        kind: "split",
        id: "split-1",
        dir: "row",
        sizes: [0.5, 0.5],
        first: { kind: "pane", id: "pane-2" },
        second: { kind: "pane", id: "pane-3" },
      },
    });
    assert.equal(countLayoutPanes(layout), 3);
  });
});

describe("resolveQuitRequest", () => {
  it("quits silently when no panes survive", () => {
    assert.equal(resolveQuitRequest("ask", 0), "keep");
    assert.equal(resolveQuitRequest("stop", 0), "keep");
  });

  it("asks by default when panes survive", () => {
    assert.equal(resolveQuitRequest("ask", 1), "dialog");
    assert.equal(resolveQuitRequest("ask", 4), "dialog");
  });

  it("honors a remembered choice", () => {
    assert.equal(resolveQuitRequest("keep", 2), "keep");
    assert.equal(resolveQuitRequest("stop", 2), "stop");
  });
});

describe("quitDialogCopy", () => {
  it("names panes and agents with plurals", () => {
    const copy = quitDialogCopy(3, 2);
    assert.equal(copy.title, "Quit Ubra?");
    assert.match(copy.detail, /3 terminal panes \(2 agents\)/);
  });

  it("uses singular forms for one pane and one agent", () => {
    assert.match(quitDialogCopy(1, 1).detail, /1 terminal pane \(1 agent\)/);
  });

  it("omits the agent count when no agent is attached", () => {
    const copy = quitDialogCopy(2, 0);
    assert.match(copy.detail, /2 terminal panes will keep running/);
    assert.doesNotMatch(copy.detail, /agent/);
  });

  it("warns that panes stop without survival mode", () => {
    const copy = quitDialogCopy(3, 2, false);
    assert.equal(copy.title, "Quit Ubra?");
    assert.match(copy.detail, /3 terminal panes \(2 agents\) will stop/);
    assert.match(copy.detail, /Quitting ends every process/);
    assert.doesNotMatch(copy.detail, /keep running|reattach/);
  });

  it("omits the agent count without survival or attached agents", () => {
    const copy = quitDialogCopy(2, 0, false);
    assert.match(copy.detail, /2 terminal panes will stop/);
    assert.doesNotMatch(copy.detail, /agent/);
  });
});
