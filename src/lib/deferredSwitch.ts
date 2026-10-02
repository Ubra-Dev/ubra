/**
 * Deferred workspace switching: the sidebar highlights the target workspace
 * synchronously (same frame as the click) while the expensive reveal work —
 * unhiding canvases, terminal refits, panel remounts — commits on the next
 * animation frame. Framework-free with an injectable clock so unit tests can
 * drive it without a browser.
 */
import { frameCoalescer, type Coalescer, type FrameClock } from "./schedule.ts";

export interface DeferredSwitchCallbacks {
  /** Fired with the target id on request, then null once committed+painted. */
  onPendingChange(id: string | null): void;
}

export class DeferredSwitch {
  private coalescer: Coalescer;
  private pending: string | null = null;
  private cb: DeferredSwitchCallbacks;

  constructor(cb: DeferredSwitchCallbacks, clock?: FrameClock) {
    this.cb = cb;
    this.coalescer = clock ? frameCoalescer(clock) : frameCoalescer();
  }

  /** Target id shown as active while the commit is still scheduled. */
  get pendingId(): string | null {
    return this.pending;
  }

  get hasPending(): boolean {
    return this.coalescer.hasPending;
  }

  /**
   * Request a switch to `id`: pending is set synchronously, `commit` runs on
   * the next frame, and pending clears on the frame after so the commit frame
   * still sees the pending target (lets reveal animations start at unhide).
   * Rapid requests collapse: only the latest commit runs.
   */
  request(id: string, commit: () => void): void {
    this.pending = id;
    this.cb.onPendingChange(id);
    this.coalescer.schedule(() => {
      commit();
      const committed = id;
      this.coalescer.schedule(() => {
        if (this.pending === committed) {
          this.pending = null;
          this.cb.onPendingChange(null);
        }
      });
    });
  }

  /** Run the scheduled commit now (the pending clear still takes a frame). */
  flush(): void {
    this.coalescer.flush();
  }

  /** Drop a scheduled switch without committing. */
  cancel(): void {
    this.pending = null;
    this.cb.onPendingChange(null);
    this.coalescer.cancel();
  }
}
