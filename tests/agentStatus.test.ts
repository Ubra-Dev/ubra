import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  acknowledgeAgent, applyAgentUpdate, effectiveAgentStatus, emptyAgentModel,
  paneIsVisible, registerAgentPane, rollupStatuses, type AgentTransition,
} from "../src/lib/agentStatus.ts";
const completion: AgentTransition = {
  eventId: "task:1", paneId: 1, agentInstanceId: "instance:1", kind: "task-completed",
};
describe("agent updates", () => {
  it("keeps newest runtime state and never replays events", () => {
    let result = applyAgentUpdate(emptyAgentModel(), {
      revision: 3, states: { 1: { state: "done", agentInstanceId: "instance:1" } }, transitions: [completion],
    });
    assert.equal(result.transitions.length, 1);
    assert.equal(result.model.unread[1].kind, "done");
    const newest = result.model;
    result = applyAgentUpdate(newest, { revision: 2, states: { 1: { state: "working" } }, transitions: [] });
    assert.equal(result.model, newest);
    assert.deepEqual(result.transitions, []);
    result = applyAgentUpdate(newest, { revision: 4, states: newest.states, transitions: [completion] });
    assert.deepEqual(result.transitions, []);
  });
  it("delivers a live event whose snapshot revision arrived first", () => {
    const model = applyAgentUpdate(emptyAgentModel(), {
      revision: 3, states: { 1: { state: "done" } }, transitions: [],
    }).model;
    const result = applyAgentUpdate(model, { revision: 3, states: model.states, transitions: [completion] });
    assert.deepEqual(result.transitions, [completion]);
    const duplicate = applyAgentUpdate(result.model, { revision: 3, states: model.states, transitions: [completion] });
    assert.deepEqual(duplicate.transitions, []);
  });
  it("does not fabricate finishes for unknown or blocked to idle", () => {
    for (const state of ["unknown", "blocked"] as const) {
      const before = applyAgentUpdate(emptyAgentModel(), {
        revision: 1, states: { 1: { state } }, transitions: [],
      }).model;
      const result = applyAgentUpdate(before, { revision: 2, states: { 1: { state: "idle" } }, transitions: [] });
      assert.deepEqual(result.transitions, []);
      assert.deepEqual(result.model.unread, {});
    }
  });
  it("acknowledges unread without changing runtime status", () => {
    const model = applyAgentUpdate(emptyAgentModel(), {
      revision: 1, states: { 1: { state: "done" } }, transitions: [completion],
    }).model;
    assert.equal(acknowledgeAgent(model, 1, false), model);
    const seen = acknowledgeAgent(model, 1, true);
    assert.equal(seen.states, model.states);
    assert.equal(seen.states[1].state, "done");
    assert.deepEqual(seen.unread, {});
  });
  it("clears obsolete review for a new task or agent instance", () => {
    const model = applyAgentUpdate(emptyAgentModel(), {
      revision: 1, states: { 1: { state: "done" } }, transitions: [completion],
    }).model;
    for (const state of [{ state: "working" as const, agentInstanceId: "instance:1" },
      { state: "idle" as const, agentInstanceId: "instance:2" }]) {
      const next = applyAgentUpdate(model, { revision: 2, states: { 1: state }, transitions: [completion] });
      assert.deepEqual(next.model.unread, {});
      assert.deepEqual(next.transitions, []);
    }
  });
  it("suppresses an obsolete transition after instance replacement", () => {
    const result = applyAgentUpdate(emptyAgentModel(), {
      revision: 1, states: { 1: { state: "idle", agentInstanceId: "instance:2" } }, transitions: [completion],
    });
    assert.deepEqual(result.model.unread, {});
    assert.deepEqual(result.transitions, []);
  });
  it("allows transitions before the pane mapping has registered", () => {
    const result = applyAgentUpdate(emptyAgentModel(), {
      revision: 1, states: {}, transitions: [completion],
    });
    assert.deepEqual(result.transitions, [completion]);
    assert.equal(result.model.unread[1].kind, "done");
  });
});
describe("effective status and rollups", () => {
  it("checks all panes for Blocked regardless of traversal order", () => {
    assert.equal(rollupStatuses(["working", "blocked"]), "blocked");
    assert.equal(rollupStatuses(["blocked", "working"]), "blocked");
    assert.equal(rollupStatuses(["unknown", "done", "attention"]), "attention");
    assert.equal(effectiveAgentStatus({ state: "working" }, { kind: "attention", agentInstanceId: "a" }), "working");
    assert.equal(effectiveAgentStatus({ state: "idle" }, { kind: "done", agentInstanceId: "a" }), "done");
  });
});
describe("review visibility", () => {
  const visible = { activeWorkspaceId: "ws", workspaceId: "ws", activeTabId: "tab", tabId: "tab",
    nodeId: "pane", focusedNodeId: "pane", foreground: true };
  it("requires actual focus in the foreground window", () => {
    assert.equal(paneIsVisible(visible), true);
    assert.equal(paneIsVisible({ ...visible, foreground: false }), false);
    assert.equal(paneIsVisible({ ...visible, focusedNodeId: null }), false);
    assert.equal(paneIsVisible({ ...visible, focusedNodeId: "other" }), false);
  });
  it("keeps zoom-hidden panes and inactive tabs or workspaces unseen", () => {
    assert.equal(paneIsVisible({ ...visible, zoomedPaneId: "other" }), false);
    assert.equal(paneIsVisible({ ...visible, zoomedPaneId: "pane" }), true);
    assert.equal(paneIsVisible({ ...visible, activeWorkspaceId: "other" }), false);
    assert.equal(paneIsVisible({ ...visible, activeTabId: "other" }), false);
  });
});

describe("pane session registration", () => {
  it("replaces the dead session on respawn without duplicating pane rows", () => {
    const old = { 1: "pane-a", 2: "pane-b" };
    const next = registerAgentPane(old, 3, "pane-a");
    assert.deepEqual(next.mapping, { 2: "pane-b", 3: "pane-a" });
    assert.deepEqual(next.replacedLiveIds, [1]);
    assert.deepEqual(old, { 1: "pane-a", 2: "pane-b" });
  });
  it("preserves session metadata on reattachment after a move", () => {
    const next = registerAgentPane({ 1: "pane-a", 2: "pane-b" }, 1, "pane-a");
    assert.deepEqual(next.mapping, { 1: "pane-a", 2: "pane-b" });
    assert.deepEqual(next.replacedLiveIds, []);
  });
});
