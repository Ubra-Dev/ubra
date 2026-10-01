import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { acquireSession, closeSession, dropSession, evictDeadSessions, evictSessions } from "../src/lib/ptySessions.ts";

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

  it("leases start unattached with restore delivery unconsumed", async () => {
    const kill = async () => {};
    const lease = acquireSession("flags", async () => 21, kill);
    assert.equal(await lease.ready, 21);
    assert.equal(lease.attached, false);
    assert.equal(lease.restoreTaken, false);
    closeSession("flags");
  });

  it("eviction drops ownership without killing, and in-flight spawns self-clean", async () => {
    const pending = deferred();
    const killed: number[] = [];
    const kill = async (id: number) => { killed.push(id); };
    const live = acquireSession("evict-live", async () => 31, kill);
    assert.equal(await live.ready, 31);
    const flying = acquireSession("evict-flying", () => pending.promise, kill);
    assert.deepEqual(evictSessions(["evict-live", "evict-flying", "evict-missing"]).map((l) => l.id), [31, null]);
    // Resolved sessions are never killed by eviction; in-flight ones are
    // reaped on resolution instead of leaking.
    pending.resolve(32);
    assert.equal(await flying.ready, 32);
    assert.deepEqual(killed, [32]);
    const replacement = acquireSession("evict-live", async () => 33, kill);
    assert.equal(await replacement.ready, 33);
    closeSession("evict-live");
    assert.deepEqual(killed, [32, 33]);
  });

  it("dead-only eviction keeps resolved leases for readoption", async () => {
    const kill = async () => {};
    const live = acquireSession("dead-live", async () => 41, kill);
    assert.equal(await live.ready, 41);
    const dead = acquireSession("dead-dead", () => new Promise<number>(() => {}), kill);
    assert.deepEqual(evictDeadSessions(["dead-live", "dead-dead"]), [dead]);
    assert.equal(acquireSession("dead-live", async () => 99, kill), live);
    const replacement = acquireSession("dead-dead", async () => 42, kill);
    assert.equal(await replacement.ready, 42);
    closeSession("dead-live");
    closeSession("dead-dead");
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
});
