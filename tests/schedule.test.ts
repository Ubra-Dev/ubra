import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
  asyncCoalescer,
  frameBudgetQueue,
  frameCoalescer,
  trailingDebouncer,
  type FrameClock,
  type TimerClock,
} from "../src/lib/schedule.ts";

function fakeFrameClock(): FrameClock & {
  callbacks: Map<number, () => void>;
  cancels: number[];
  runAll(): void;
} {
  const callbacks = new Map<number, () => void>();
  const cancels: number[] = [];
  let next = 1;
  return {
    callbacks,
    cancels,
    request(cb: () => void): number {
      const handle = next++;
      callbacks.set(handle, cb);
      return handle;
    },
    cancel(handle: number): void {
      cancels.push(handle);
      callbacks.delete(handle);
    },
    runAll(): void {
      const pending = [...callbacks.values()];
      callbacks.clear();
      for (const cb of pending) cb();
    },
  };
}

function fakeTimerClock(): TimerClock & {
  callbacks: Map<unknown, () => void>;
  clears: unknown[];
  lastMs: number;
  runAll(): void;
} {
  const callbacks = new Map<unknown, () => void>();
  const clears: unknown[] = [];
  let next = 1;
  const fake = {
    callbacks,
    clears,
    lastMs: 0,
    set(cb: () => void, ms: number): unknown {
      fake.lastMs = ms;
      const handle = next++;
      callbacks.set(handle, cb);
      return handle;
    },
    clear(handle: unknown): void {
      clears.push(handle);
      callbacks.delete(handle);
    },
    runAll(): void {
      const pending = [...callbacks.values()];
      callbacks.clear();
      for (const cb of pending) cb();
    },
  };
  return fake;
}

describe("frameCoalescer", () => {
  it("runs only the latest callback once per frame", () => {
    const clock = fakeFrameClock();
    const coalescer = frameCoalescer(clock);
    const calls: string[] = [];
    coalescer.schedule(() => calls.push("first"));
    coalescer.schedule(() => calls.push("second"));
    assert.equal(clock.callbacks.size, 1);
    assert.equal(coalescer.hasPending, true);
    clock.runAll();
    assert.deepEqual(calls, ["second"]);
    assert.equal(coalescer.hasPending, false);
  });

  it("schedules a new frame after the previous one ran", () => {
    const clock = fakeFrameClock();
    const coalescer = frameCoalescer(clock);
    let calls = 0;
    coalescer.schedule(() => (calls += 1));
    clock.runAll();
    coalescer.schedule(() => (calls += 1));
    clock.runAll();
    assert.equal(calls, 2);
  });

  it("flush runs the pending callback immediately", () => {
    const clock = fakeFrameClock();
    const coalescer = frameCoalescer(clock);
    let calls = 0;
    coalescer.schedule(() => (calls += 1));
    coalescer.flush();
    assert.equal(calls, 1);
    assert.equal(clock.callbacks.size, 0);
    clock.runAll();
    assert.equal(calls, 1);
  });

  it("cancel drops the pending callback", () => {
    const clock = fakeFrameClock();
    const coalescer = frameCoalescer(clock);
    let calls = 0;
    coalescer.schedule(() => (calls += 1));
    coalescer.cancel();
    assert.equal(coalescer.hasPending, false);
    clock.runAll();
    assert.equal(calls, 0);
  });

  it("flush and cancel are no-ops when idle", () => {
    const clock = fakeFrameClock();
    const coalescer = frameCoalescer(clock);
    coalescer.flush();
    coalescer.cancel();
    assert.deepEqual(clock.cancels, []);
  });
});

describe("asyncCoalescer", () => {
  function deferred(): { promise: Promise<void>; resolve: () => void } {
    let resolve!: () => void;
    const promise = new Promise<void>((res) => {
      resolve = res;
    });
    return { promise, resolve };
  }
  const tick = (): Promise<void> =>
    new Promise((res) => setTimeout(res, 0));

  it("runs a single request to completion", async () => {
    const seen: string[] = [];
    const coalescer = asyncCoalescer(async (arg: string) => {
      seen.push(arg);
    });
    coalescer.request("a");
    assert.equal(coalescer.isRunning, true);
    await tick();
    await tick();
    assert.deepEqual(seen, ["a"]);
    assert.equal(coalescer.isRunning, false);
  });

  it("collapses bursts into one trailing run with the latest arg", async () => {
    const gate = deferred();
    const seen: string[] = [];
    let calls = 0;
    const coalescer = asyncCoalescer(async (arg: string) => {
      calls += 1;
      seen.push(arg);
      if (calls === 1) await gate.promise;
    });
    coalescer.request("a");
    await tick();
    assert.equal(coalescer.isRunning, true);
    coalescer.request("b");
    coalescer.request("c");
    gate.resolve();
    await tick();
    await tick();
    await tick();
    assert.deepEqual(seen, ["a", "c"]);
    assert.equal(coalescer.isRunning, false);
  });

  it("serves new requests after going idle", async () => {
    const seen: string[] = [];
    const coalescer = asyncCoalescer(async (arg: string) => {
      seen.push(arg);
    });
    coalescer.request("a");
    await tick();
    await tick();
    coalescer.request("b");
    await tick();
    await tick();
    assert.deepEqual(seen, ["a", "b"]);
  });

  it("keeps draining after a worker throw", async () => {
    const errors: unknown[] = [];
    const seen: string[] = [];
    const coalescer = asyncCoalescer(async (arg: string) => {
      seen.push(arg);
      if (arg === "boom") throw new Error("worker failed");
    });
    const original = console.error;
    console.error = (...args: unknown[]): void => {
      errors.push(args[0]);
    };
    try {
      coalescer.request("boom");
      await tick();
      await tick();
      coalescer.request("after");
      await tick();
      await tick();
    } finally {
      console.error = original;
    }
    assert.deepEqual(seen, ["boom", "after"]);
    assert.equal(errors.length, 1);
    assert.equal(coalescer.isRunning, false);
  });
});

describe("trailingDebouncer", () => {
  it("fires only the latest callback after a quiet period", () => {
    const clock = fakeTimerClock();
    const debouncer = trailingDebouncer(120, clock);
    const calls: string[] = [];
    debouncer.schedule(() => calls.push("first"));
    debouncer.schedule(() => calls.push("second"));
    assert.equal(clock.lastMs, 120);
    assert.equal(clock.clears.length, 1);
    assert.equal(clock.callbacks.size, 1);
    clock.runAll();
    assert.deepEqual(calls, ["second"]);
    assert.equal(debouncer.hasPending, false);
  });

  it("flush runs the pending callback immediately", () => {
    const clock = fakeTimerClock();
    const debouncer = trailingDebouncer(120, clock);
    let calls = 0;
    debouncer.schedule(() => (calls += 1));
    debouncer.flush();
    assert.equal(calls, 1);
    clock.runAll();
    assert.equal(calls, 1);
  });

  it("cancel drops the pending callback", () => {
    const clock = fakeTimerClock();
    const debouncer = trailingDebouncer(120, clock);
    let calls = 0;
    debouncer.schedule(() => (calls += 1));
    debouncer.cancel();
    assert.equal(debouncer.hasPending, false);
    clock.runAll();
    assert.equal(calls, 0);
  });

  it("flush and cancel are no-ops when idle", () => {
    const clock = fakeTimerClock();
    const debouncer = trailingDebouncer(120, clock);
    debouncer.flush();
    debouncer.cancel();
    assert.deepEqual(clock.clears, []);
  });
});

describe("frameBudgetQueue", () => {
  it("drains at most maxPerFrame callbacks per frame", () => {
    const clock = fakeFrameClock();
    const queue = frameBudgetQueue(2, clock);
    const calls: string[] = [];
    for (const key of ["a", "b", "c", "d", "e"]) {
      queue.push(key, () => calls.push(key));
    }
    assert.equal(queue.depth, 5);
    assert.equal(clock.callbacks.size, 1);
    clock.runAll();
    assert.deepEqual(calls, ["a", "b"]);
    assert.equal(queue.depth, 3);
    clock.runAll();
    assert.deepEqual(calls, ["a", "b", "c", "d"]);
    clock.runAll();
    assert.deepEqual(calls, ["a", "b", "c", "d", "e"]);
    assert.equal(queue.depth, 0);
    assert.equal(clock.callbacks.size, 0);
  });

  it("replaces the queued callback for a repeated key", () => {
    const clock = fakeFrameClock();
    const queue = frameBudgetQueue(4, clock);
    const calls: string[] = [];
    queue.push("a", () => calls.push("stale"));
    queue.push("a", () => calls.push("fresh"));
    assert.equal(queue.depth, 1);
    clock.runAll();
    assert.deepEqual(calls, ["fresh"]);
  });

  it("defers callbacks pushed while a batch runs to a later frame", () => {
    const clock = fakeFrameClock();
    const queue = frameBudgetQueue(4, clock);
    const calls: string[] = [];
    queue.push("a", () => {
      calls.push("a");
      queue.push("b", () => calls.push("b"));
    });
    clock.runAll();
    assert.deepEqual(calls, ["a"]);
    clock.runAll();
    assert.deepEqual(calls, ["a", "b"]);
  });

  it("drops a single key without disturbing the rest", () => {
    const clock = fakeFrameClock();
    const queue = frameBudgetQueue(4, clock);
    const calls: string[] = [];
    queue.push("a", () => calls.push("a"));
    queue.push("b", () => calls.push("b"));
    queue.drop("a");
    queue.drop("missing");
    clock.runAll();
    assert.deepEqual(calls, ["b"]);
  });

  it("cancel drops everything and keeps reporting errors without wedging", () => {
    const clock = fakeFrameClock();
    const queue = frameBudgetQueue(4, clock);
    const calls: string[] = [];
    const errors: unknown[] = [];
    const original = console.error;
    console.error = (error: unknown) => errors.push(error);
    try {
      queue.push("boom", () => {
        throw new Error("gpu failed");
      });
      queue.push("ok", () => calls.push("ok"));
      clock.runAll();
      assert.deepEqual(calls, ["ok"]);
      assert.equal(errors.length, 1);
      queue.push("late", () => calls.push("late"));
      queue.cancel();
      assert.equal(queue.depth, 0);
      clock.runAll();
      assert.deepEqual(calls, ["ok"]);
    } finally {
      console.error = original;
    }
  });
});
