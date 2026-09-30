import { describe, it } from "node:test";
import assert from "node:assert/strict";
import type { PaneNode } from "../src/lib/layout.ts";
import { acquireSession, closeSession } from "../src/lib/ptySessions.ts";
import { StartupCommands } from "../src/lib/startupCommands.ts";

function pane(id: string, cmd?: string[], cmdOnRestore?: boolean): PaneNode {
  const node: PaneNode = { kind: "pane", id };
  if (cmd !== undefined) node.cmd = [...cmd];
  if (cmdOnRestore !== undefined) node.cmdOnRestore = cmdOnRestore;
  return node;
}

describe("StartupCommands", () => {
  it("opens restored manual panes as shells until explicitly authorized", () => {
    const auth = new StartupCommands();
    assert.equal(auth.resolve(pane("p", ["node", "-e", "x"], false)), undefined);
    auth.authorize(["p"]);
    assert.deepEqual(auth.resolve(pane("p", ["node", "-e", "x"], false)), ["node", "-e", "x"]);
  });

  it("consumes authorization exactly once and leaves unrelated panes alone", () => {
    const auth = new StartupCommands();
    auth.authorize(["a"]);
    assert.deepEqual(auth.resolve(pane("a", ["sh"], false)), ["sh"]);
    assert.equal(auth.resolve(pane("a", ["sh"], false)), undefined);
    assert.equal(auth.resolve(pane("b", ["sh"], false)), undefined);
  });

  it("preserves authorization across moves and prunes closed panes", () => {
    const auth = new StartupCommands();
    auth.authorize(["m"]);
    auth.retain(new Set(["m", "other"]));
    assert.deepEqual(auth.resolve(pane("m", ["sh"], false)), ["sh"]);
    auth.authorize(["gone"]);
    auth.retain(new Set(["elsewhere"]));
    assert.equal(auth.resolve(pane("gone", ["sh"], false)), undefined);
  });

  it("still auto-runs legacy panes without the manual policy", () => {
    const auth = new StartupCommands();
    assert.deepEqual(auth.resolve(pane("legacy", ["claude", "--resume"])), ["claude", "--resume"]);
  });

  it("single-flights deferred spawns so a remount never executes twice", async () => {
    const auth = new StartupCommands();
    auth.authorize(["pane-1"]);
    let spawns = 0;
    const spawn = async (): Promise<number> => {
      spawns += 1;
      const target = pane("pane-1", ["node", "argv"], false);
      const argv = auth.resolve(target);
      assert.deepEqual(argv, ["node", "argv"]);
      return 7;
    };
    const kill = async (_id: number): Promise<void> => {};
    const first = acquireSession("startup-test-key", spawn, kill);
    const second = acquireSession("startup-test-key", spawn, kill);
    assert.equal(first, second);
    assert.equal(await first.ready, 7);
    assert.equal(await second.ready, 7);
    assert.equal(spawns, 1);
    // A fresh template pane key gets an independent process.
    const other = acquireSession("startup-test-key-2", async () => 9, kill);
    assert.equal(await other.ready, 9);
    assert.equal(spawns, 1);
    closeSession("startup-test-key");
    closeSession("startup-test-key-2");
  });
});
