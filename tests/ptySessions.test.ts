import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  acquireSession, attachResultFor, closeSession, dropSession, forgetSession,
  noteAttachResult,
} from "../src/lib/ptySessions.ts";

function deferred() {
  let resolve!: (id: number) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<number>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

describe("pane session ownership", () => {
  it("rapid remounts adopt one pending spawn and preserve the resolved session", async () => {
    const pending = deferred();
    let spawns = 0;
    const killed: number[] = [];
    const spawn = () => { spawns++; return pending.promise; };
    const kill = async (id: number) => { killed.push(id); };
    const first = acquireSession("moving", spawn, kill);
    const second = acquireSession("moving", spawn, kill);
    const third = acquireSession("moving", spawn, kill);
    pending.resolve(7);
    assert.equal(await first.ready, 7);
    assert.equal(second, first);
    assert.equal(third, first);
    assert.equal(spawns, 1);
    assert.deepEqual(killed, []);
    closeSession("moving");
    closeSession("moving");
    assert.deepEqual(killed, [7]);
  });

  it("close before resolution kills once without replacing a newer lease", async () => {
    const pending = deferred();
    const killed: number[] = [];
    const kill = async (id: number) => { killed.push(id); };
    const old = acquireSession("closed", () => pending.promise, kill);
    closeSession("closed");
    closeSession("closed");
    const replacement = acquireSession("closed", async () => 11, kill);
    pending.resolve(10);
    await old.ready;
    await replacement.ready;
    dropSession("closed", old);
    assert.equal(acquireSession("closed", async () => 99, kill), replacement);
    assert.deepEqual(killed, [10]);
    closeSession("closed");
    assert.deepEqual(killed, [10, 11]);
  });

  it("failed spawn permits retry, while stale failure cannot clear replacement", async () => {
    const pending = deferred();
    const kill = async () => {};
    const failed = acquireSession("failure", () => pending.promise, kill);
    const observed = assert.rejects(failed.ready, /spawn failed/);
    closeSession("failure");
    const replacement = acquireSession("failure", async () => 12, kill);
    pending.reject(new Error("spawn failed"));
    await observed;
    await replacement.ready;
    assert.equal(acquireSession("failure", async () => 99, kill), replacement);
    dropSession("failure", replacement);
    const retried = acquireSession("failure", async () => 13, kill);
    assert.equal(await retried.ready, 13);
    closeSession("failure");
  });

  it("forgetSession releases the lease and cached result without killing", async () => {
    const killed: number[] = [];
    const kill = async (id: number) => { killed.push(id); };
    const lease = acquireSession("restart", async () => 16, kill);
    assert.equal(await lease.ready, 16);
    noteAttachResult("restart", { pane: 16, epoch: 1 });
    forgetSession("restart");
    assert.equal(attachResultFor("restart"), null);
    const replacement = acquireSession("restart", async () => 17, kill);
    assert.notEqual(replacement, lease);
    assert.equal(await replacement.ready, 17);
    assert.deepEqual(killed, []);
    closeSession("restart");
  });
});
