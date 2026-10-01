import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  DEFAULT_FLEET_STARTER_ID,
  detectedStems,
  FLEET_MAX_PANES,
  FLEET_STARTERS,
  fleetStarterById,
  planFleet,
} from "../src/lib/fleet.ts";

describe("planFleet", () => {
  it("keeps the selection in order within the cap", () => {
    assert.deepEqual(planFleet(["codex", "claude"], null), ["codex", "claude"]);
  });

  it("caps the fleet at FLEET_MAX_PANES", () => {
    assert.equal(FLEET_MAX_PANES, 3);
    assert.deepEqual(planFleet(["a", "b", "c", "d", "e"], null), ["a", "b", "c"]);
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

describe("FLEET_STARTERS", () => {
  it("has unique ids and non-empty prompts", () => {
    const ids = FLEET_STARTERS.map((starter) => starter.id);
    assert.equal(new Set(ids).size, ids.length);
    for (const starter of FLEET_STARTERS) {
      assert.ok(starter.title.trim());
      assert.ok(starter.blurb.trim());
      assert.ok(starter.prompt.trim());
    }
  });

  it("keeps every prompt read-only", () => {
    for (const starter of FLEET_STARTERS) {
      assert.match(starter.prompt, /don't change anything/i);
    }
  });

  it("resolves the default starter and falls back for unknown ids", () => {
    assert.equal(fleetStarterById(DEFAULT_FLEET_STARTER_ID).id, "explain");
    assert.equal(fleetStarterById("nope").id, "explain");
  });
});
