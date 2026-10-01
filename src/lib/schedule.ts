/**
 * Schedulers for high-frequency UI events (pointer drags, ResizeObserver
 * callbacks). Bursts collapse so layout work runs at most once per frame and
 * backend calls fire once the burst goes quiet. Framework-free with injectable
 * clocks so unit tests can drive them without a browser.
 */

/** Minimal rAF-shaped clock; defaults to the browser's. */
export interface FrameClock {
  request(callback: () => void): number;
  cancel(handle: number): void;
}

const browserFrameClock: FrameClock = {
  request: (callback) => requestAnimationFrame(() => callback()),
  cancel: (handle) => cancelAnimationFrame(handle),
};

export interface Coalescer {
  /** Run `fn` on the next frame, replacing any callback scheduled earlier. */
  schedule(fn: () => void): void;
  /** Run the pending callback now, if any. */
  flush(): void;
  /** Drop the pending callback, if any. */
  cancel(): void;
  readonly hasPending: boolean;
}

/** Collapse bursts into one callback per animation frame (latest wins). */
export function frameCoalescer(
  clock: FrameClock = browserFrameClock,
): Coalescer {
  let pending: (() => void) | null = null;
  let handle = 0;
  let scheduled = false;
  const run = (): void => {
    scheduled = false;
    const fn = pending;
    pending = null;
    fn?.();
  };
  return {
    schedule(fn: () => void): void {
      pending = fn;
      if (scheduled) return;
      scheduled = true;
      handle = clock.request(run);
    },
    flush(): void {
      if (!scheduled) return;
      clock.cancel(handle);
      run();
    },
    cancel(): void {
      if (!scheduled) return;
      scheduled = false;
      pending = null;
      clock.cancel(handle);
    },
    get hasPending(): boolean {
      return scheduled;
    },
  };
}

export interface FrameBudgetQueue {
  /** Enqueue `fn` under `key`, replacing any callback queued for that key. */
  push(key: string, fn: () => void): void;
  /** Drop the queued callback for `key`, if any. */
  drop(key: string): void;
  /** Drop every queued callback. */
  cancel(): void;
  readonly depth: number;
}

/**
 * Drain a deduplicated queue across frames, running at most `maxPerFrame`
 * callbacks per frame (minimum 1). Callbacks queued while a batch runs wait
 * for a later frame; a throwing callback is reported and the drain continues
 * so one failure never wedges the queue.
 */
export function frameBudgetQueue(
  maxPerFrame: number,
  clock: FrameClock = browserFrameClock,
): FrameBudgetQueue {
  const budget = Math.max(1, Math.floor(maxPerFrame));
  const queued = new Map<string, () => void>();
  let scheduled = false;
  let handle = 0;
  const run = (): void => {
    scheduled = false;
    const batch = [...queued.keys()].slice(0, budget);
    for (const key of batch) {
      const fn = queued.get(key);
      if (!fn) continue;
      queued.delete(key);
      try {
        fn();
      } catch (error) {
        console.error(error);
      }
    }
    if (queued.size > 0) {
      scheduled = true;
      handle = clock.request(run);
    }
  };
  const arm = (): void => {
    if (scheduled) return;
    scheduled = true;
    handle = clock.request(run);
  };
  return {
    push(key: string, fn: () => void): void {
      queued.set(key, fn);
      arm();
    },
    drop(key: string): void {
      queued.delete(key);
    },
    cancel(): void {
      queued.clear();
      if (!scheduled) return;
      scheduled = false;
      clock.cancel(handle);
    },
    get depth(): number {
      return queued.size;
    },
  };
}

/** Minimal setTimeout-shaped clock; defaults to the browser's. */
export interface TimerClock {
  set(callback: () => void, ms: number): unknown;
  clear(handle: unknown): void;
}

const browserTimerClock: TimerClock = {
  set: (callback, ms) => setTimeout(callback, ms),
  clear: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
};

export interface Debouncer {
  /** Run `fn` after `waitMs` with no further schedules (latest wins). */
  schedule(fn: () => void): void;
  /** Run the pending callback now, if any. */
  flush(): void;
  /** Drop the pending callback, if any. */
  cancel(): void;
  readonly hasPending: boolean;
}

export interface AsyncCoalescer<T> {
  /**
   * Request a run with `arg`. While a run is in flight, further requests
   * only update the argument for one trailing run, so bursts collapse and
   * overlapping runs never pile up. The worker always sees the latest arg.
   */
  request(arg: T): void;
  readonly isRunning: boolean;
}

/**
 * Collapse async bursts into sequential latest-wins runs. The worker must
 * handle its own errors; a throw is reported and the queue keeps draining
 * so one failure never wedges later requests.
 */
export function asyncCoalescer<T>(
  worker: (arg: T) => Promise<void>,
): AsyncCoalescer<T> {
  let running = false;
  let pending: { arg: T } | null = null;
  const pump = async (): Promise<void> => {
    running = true;
    try {
      while (pending !== null) {
        const { arg } = pending;
        pending = null;
        try {
          await worker(arg);
        } catch (error) {
          console.error(error);
        }
      }
    } finally {
      running = false;
    }
  };
  return {
    request(arg: T): void {
      pending = { arg };
      if (!running) void pump();
    },
    get isRunning(): boolean {
      return running;
    },
  };
}

/** Trailing-edge debounce: fires once after the burst goes quiet. */
export function trailingDebouncer(
  waitMs: number,
  clock: TimerClock = browserTimerClock,
): Debouncer {
  let pending: (() => void) | null = null;
  let handle: unknown = null;
  const run = (): void => {
    handle = null;
    const fn = pending;
    pending = null;
    fn?.();
  };
  return {
    schedule(fn: () => void): void {
      pending = fn;
      if (handle !== null) clock.clear(handle);
      handle = clock.set(run, waitMs);
    },
    flush(): void {
      if (handle === null) return;
      clock.clear(handle);
      run();
    },
    cancel(): void {
      if (handle === null) return;
      clock.clear(handle);
      handle = null;
      pending = null;
    },
    get hasPending(): boolean {
      return handle !== null;
    },
  };
}
