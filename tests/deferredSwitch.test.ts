import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { DeferredSwitch } from "../src/lib/deferredSwitch.ts";
import type { FrameClock } from "../src/lib/schedule.ts";

function fakeFrameClock(): FrameClock & {
  callbacks: Map<number, () => void>;
  /** Run one scheduled frame (the earliest), if any. */
  runOne(): void;
} {
  const callbacks = new Map<number, () => void>();
  let next = 1;
  return {
    callbacks,
    request(cb: () => void): number {
      const handle = next++;
      callbacks.set(handle, cb);
      return handle;
    },
    cancel(handle: number): void {
      callbacks.delete(handle);
    },
    runOne(): void {
      const first = [...callbacks.keys()].sort((a, b) => a - b)[0];
      const cb = first === undefined ? undefined : callbacks.get(first);
      if (first !== undefined) callbacks.delete(first);
      cb?.();
    },
  };
}

function setup(): {
  clock: ReturnType<typeof fakeFrameClock>;
  pendingChanges: (string | null)[];
  commits: string[];
  sw: DeferredSwitch;
} {
  const clock = fakeFrameClock();
  const pendingChanges: (string | null)[] = [];
  const commits: string[] = [];
  const sw = new DeferredSwitch(
    { onPendingChange: (id) => pendingChanges.push(id) },
    clock,
  );
  return { clock, pendingChanges, commits, sw };
}

describe("DeferredSwitch", () => {
  it("sets pending synchronously, commits on the next frame, clears the frame after", () => {
    const { clock, pendingChanges, commits, sw } = setup();
    sw.request("b", () => commits.push("b"));
    assert.equal(sw.pendingId, "b");
    assert.deepEqual(pendingChanges, ["b"]);
    assert.deepEqual(commits, []);

    clock.runOne();
    assert.deepEqual(commits, ["b"]);
    assert.equal(sw.pendingId, "b");

    clock.runOne();
    assert.equal(sw.pendingId, null);
    assert.deepEqual(pendingChanges, ["b", null]);
  });

  it("collapses rapid requests so only the latest commit runs", () => {
    const { clock, pendingChanges, commits, sw } = setup();
    sw.request("b", () => commits.push("b"));
    sw.request("c", () => commits.push("c"));
    assert.equal(sw.pendingId, "c");

    clock.runOne();
    assert.deepEqual(commits, ["c"]);
    clock.runOne();
    assert.equal(sw.pendingId, null);
    assert.deepEqual(pendingChanges, ["b", "c", null]);
  });

  it("keeps a superseding request pending through its own commit frame", () => {
    const { clock, commits, sw } = setup();
    sw.request("b", () => commits.push("b"));
    clock.runOne();
    assert.deepEqual(commits, ["b"]);
    sw.request("c", () => commits.push("c"));
    clock.runOne();
    assert.deepEqual(commits, ["b", "c"]);
    assert.equal(sw.pendingId, "c");
    clock.runOne();
    assert.equal(sw.pendingId, null);
  });

  it("cancels a scheduled switch without committing", () => {
    const { clock, pendingChanges, commits, sw } = setup();
    sw.request("b", () => commits.push("b"));
    sw.cancel();
    assert.equal(sw.pendingId, null);
    assert.equal(sw.hasPending, false);
    clock.runOne();
    assert.deepEqual(commits, []);
    assert.deepEqual(pendingChanges, ["b", null]);
  });

  it("flush commits synchronously and clears on the next frame", () => {
    const { clock, commits, sw } = setup();
    sw.request("b", () => commits.push("b"));
    sw.flush();
    assert.deepEqual(commits, ["b"]);
    assert.equal(sw.pendingId, "b");
    clock.runOne();
    assert.equal(sw.pendingId, null);
  });
});
