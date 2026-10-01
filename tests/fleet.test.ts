import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  detectedStems,
  FLEET_DEFAULT_PICKS,
  planFleet,
  rollFleetPlan,
} from "../src/lib/fleet.ts";
import { collectPaneIds, countPanes } from "../src/lib/layout.ts";

describe("planFleet", () => {
  it("keeps the selection in order", () => {
    assert.deepEqual(planFleet(["codex", "claude"], null), ["codex", "claude"]);
  });

  it("sets no launch cap", () => {
    assert.equal(FLEET_DEFAULT_PICKS, 4);
    assert.deepEqual(
      planFleet(["a", "b", "c", "d", "e", "f", "g"], null),
      ["a", "b", "c", "d", "e", "f", "g"],
    );
  });

  it("trims picks and drops blanks", () => {
    assert.deepEqual(planFleet(["  codex ", "", "   "], null), ["codex"]);
  });

  it("collapses duplicate picks", () => {
    assert.deepEqual(planFleet(["codex", "codex", "claude"], null), ["codex", "claude"]);
  });

  it("falls back to the custom command for an empty selection", () => {
    assert.deepEqual(planFleet([], "  my-agent --yes "), ["my-agent --yes"]);
    assert.deepEqual(planFleet(["codex"], "my-agent"), ["codex"]);
    assert.deepEqual(planFleet([], "   "), []);
    assert.deepEqual(planFleet([], null), []);
    assert.deepEqual(planFleet([], undefined), []);
  });
});

describe("detectedStems", () => {
  it("maps detected entries to stems in order", () => {
    assert.deepEqual(
      detectedStems([
        { cli: "codex", label: "Codex", path: "/bin/codex" },
        { cli: "claude", label: "Claude Code", path: "/bin/claude" },
      ]),
      ["codex", "claude"],
    );
  });
});

describe("rollFleetPlan", () => {
  it("covers every pane exactly once", () => {
    const plan = rollFleetPlan(5, () => 0.5);
    assert.equal(countPanes(plan.tab.root), 5);
    assert.deepEqual(new Set(plan.order), new Set(collectPaneIds(plan.tab.root)));
  });
});

