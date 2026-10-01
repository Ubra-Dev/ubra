import { describe, it } from "node:test";
import assert from "node:assert/strict";
import type { WorkspaceAgents } from "../src/lib/agent.svelte.ts";
import type { Rollup } from "../src/lib/agentStatus.ts";
import type { TimerClock } from "../src/lib/schedule.ts";
import {
  buildTrayPayload,
  createTraySync,
  TRAY_MAX_ROWS,
  type TrayOptions,
  type TrayPayload,
} from "../src/lib/traySync.ts";

const OPTIONS: TrayOptions = { daemonConnected: true, showTitle: true, showAgents: true };

function group(wsName: string, agents: { node: string; status: Rollup; tab?: string; dir?: string; pane?: string }[]): WorkspaceAgents {
  return {
    wsId: `ws-${wsName}`,
    wsName,
    agents: agents.map((a, i) => ({
      nodeId: a.node,
      agent: `Agent ${i}`,
      dir: a.dir ?? `dir-${i}`,
      status: a.status,
      statusTitle: a.status,
      tabName: a.tab ?? "Tab 1",
      paneTitle: a.pane,
    })),
  };
}

function payloadFor(groups: WorkspaceAgents[], options: TrayOptions = OPTIONS): TrayPayload {
  return buildTrayPayload(groups, options);
}

function fakeTimerClock(): TimerClock & { runAll(): void } {
  const callbacks = new Map<unknown, () => void>();
  let next = 1;
  return {
    set(cb: () => void): unknown {
      const handle = next++;
      callbacks.set(handle, cb);
      return handle;
    },
    clear(handle: unknown): void {
      callbacks.delete(handle);
    },
    runAll(): void {
      const pending = [...callbacks.values()];
      callbacks.clear();
      for (const cb of pending) cb();
    },
  };
}

describe("tray payload", () => {
  it("reports no agents with a clear title", () => {
    const payload = payloadFor([]);
    assert.equal(payload.header, "No agents running");
    assert.equal(payload.title, null);
    assert.deepEqual(payload.agents, []);
    assert.equal(payload.overflow, 0);
    assert.equal(payload.tooltip, "No agents running\nRuntime: connected");
  });

  it("collapses single-state counts to one phrase", () => {
    const payload = payloadFor([
      group("ws", [
        { node: "a", status: "working" },
        { node: "b", status: "working" },
      ]),
    ]);
    assert.equal(payload.header, "2 working");
    assert.equal(payload.title, "● 2");
  });

  it("summarizes mixed states with the total first", () => {
    const payload = payloadFor([
      group("ws", [
        { node: "a", status: "working" },
        { node: "b", status: "working" },
        { node: "c", status: "blocked" },
        { node: "d", status: "done" },
        { node: "e", status: "idle" },
      ]),
    ]);
    assert.equal(payload.header, "5 agents · 2 working · 1 blocked · 1 done");
    assert.equal(payload.title, "! 1");
  });

  it("prefers attention over working in the menu-bar title", () => {
    const working = payloadFor([group("ws", [{ node: "a", status: "working" }])]);
    assert.equal(working.title, "● 1");
    const attention = payloadFor([
      group("ws", [
        { node: "a", status: "working" },
        { node: "b", status: "attention" },
        { node: "c", status: "blocked" },
      ]),
    ]);
    assert.equal(attention.title, "! 2");
    const settled = payloadFor([
      group("ws", [
        { node: "a", status: "done" },
        { node: "b", status: "idle" },
      ]),
    ]);
    assert.equal(settled.title, null);
  });

  it("hides the title when the pref is off", () => {
    const payload = payloadFor(
      [group("ws", [{ node: "a", status: "working" }])],
      { ...OPTIONS, showTitle: false },
    );
    assert.equal(payload.title, null);
    assert.equal(payload.header, "1 working");
  });

  it("labels the daemon link in every state", () => {
    assert.equal(payloadFor([], { ...OPTIONS, daemonConnected: true }).daemonLine, "Runtime: connected");
    assert.equal(payloadFor([], { ...OPTIONS, daemonConnected: false }).daemonLine, "Runtime: reconnecting…");
    assert.equal(payloadFor([], { ...OPTIONS, daemonConnected: null }).daemonLine, "Runtime: connecting…");
  });

  it("scopes row context to tab and pane within one workspace", () => {
    const payload = payloadFor([
      group("ws", [
        { node: "a", status: "working", tab: "Main", dir: "ubra" },
        { node: "b", status: "idle", tab: "Main", dir: "ubra", pane: "docs" },
      ]),
    ]);
    assert.deepEqual(
      payload.agents.map((row) => [row.nodeId, row.context]),
      [["a", "Main · ubra"], ["b", "Main · docs"]],
    );
  });

  it("prefixes the workspace when several have agents", () => {
    const payload = payloadFor([
      group("one", [{ node: "a", status: "working", tab: "Main", dir: "ubra" }]),
      group("two", [{ node: "b", status: "idle", tab: "Main", dir: "ubra" }]),
    ]);
    assert.deepEqual(
      payload.agents.map((row) => row.context),
      ["one · Main · ubra", "two · Main · ubra"],
    );
  });

  it("caps rows and reports the overflow", () => {
    const agents = Array.from({ length: TRAY_MAX_ROWS + 3 }, (_, i) => ({
      node: `pane-${i}`,
      status: "working" as Rollup,
    }));
    const payload = payloadFor([group("ws", agents)]);
    assert.equal(payload.agents.length, TRAY_MAX_ROWS);
    assert.equal(payload.overflow, 3);
    assert.equal(payload.header, `${TRAY_MAX_ROWS + 3} working`);
  });
});

describe("tray sync", () => {
  it("debounces bursts to the latest payload", () => {
    const clock = fakeTimerClock();
    const sent: TrayPayload[] = [];
    const sync = createTraySync((payload) => sent.push(payload), 300, clock);
    const first = payloadFor([group("ws", [{ node: "a", status: "working" }])]);
    const second = payloadFor([group("ws", [{ node: "a", status: "done" }])]);
    sync.queue(first);
    sync.queue(second);
    assert.deepEqual(sent, []);
    clock.runAll();
    assert.deepEqual(sent, [second]);
  });

  it("never resends an identical payload", () => {
    const clock = fakeTimerClock();
    const sent: TrayPayload[] = [];
    const sync = createTraySync((payload) => sent.push(payload), 300, clock);
    const payload = payloadFor([group("ws", [{ node: "a", status: "working" }])]);
    sync.queue(payload);
    clock.runAll();
    sync.queue(payload);
    clock.runAll();
    assert.equal(sent.length, 1);
  });

  it("cancels a pending push when state reverts", () => {
    const clock = fakeTimerClock();
    const sent: TrayPayload[] = [];
    const sync = createTraySync((payload) => sent.push(payload), 300, clock);
    const first = payloadFor([group("ws", [{ node: "a", status: "working" }])]);
    const second = payloadFor([group("ws", [{ node: "a", status: "done" }])]);
    sync.queue(first);
    clock.runAll();
    sync.queue(second);
    sync.queue(first);
    clock.runAll();
    assert.deepEqual(sent, [first]);
  });
});
