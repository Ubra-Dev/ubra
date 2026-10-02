import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  AGENT_LAUNCH_GRACE_MS,
  AGENT_LAUNCH_TIMEOUT_MS,
  agentLaunchExpired,
  agentLaunchReady,
} from "../src/lib/agentLaunching.ts";

describe("agentLaunchReady", () => {
  it("is false without any status yet", () => {
    assert.equal(agentLaunchReady(null), false);
    assert.equal(agentLaunchReady(undefined), false);
  });

  it("is false while the agent process has no recognized UI", () => {
    assert.equal(agentLaunchReady({ state: "unknown" }), false);
    assert.equal(
      agentLaunchReady({ state: "unknown", agent: "Codex", cli: "codex" }),
      false,
    );
  });

  it("is false for a plain idle pane with no agent process", () => {
    assert.equal(agentLaunchReady({ state: "idle" }), false);
    assert.equal(agentLaunchReady({ state: "idle", agent: "" }), false);
  });

  it("is true once a named agent reaches its prompt", () => {
    assert.equal(
      agentLaunchReady({ state: "idle", agent: "Codex", cli: "codex" }),
      true,
    );
  });

  it("is true for recognized post-launch states", () => {
    for (const state of ["working", "blocked", "done"] as const) {
      assert.equal(agentLaunchReady({ state }), true);
    }
  });
});

describe("agent launch timings", () => {
  it("shows the overlay only after a short grace, well before the timeout", () => {
    assert.ok(AGENT_LAUNCH_GRACE_MS > 0);
    assert.ok(AGENT_LAUNCH_GRACE_MS < AGENT_LAUNCH_TIMEOUT_MS);
  });
});

describe("agentLaunchExpired", () => {
  it("expires only after the timeout elapses", () => {
    assert.equal(agentLaunchExpired(1000, 1000), false);
    assert.equal(agentLaunchExpired(1000, 1000 + AGENT_LAUNCH_TIMEOUT_MS - 1), false);
    assert.equal(agentLaunchExpired(1000, 1000 + AGENT_LAUNCH_TIMEOUT_MS), true);
    assert.equal(agentLaunchExpired(1000, 1000 + AGENT_LAUNCH_TIMEOUT_MS + 1), true);
  });
});
