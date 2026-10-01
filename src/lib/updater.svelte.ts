import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

/** Delay before the first background check so boot stays snappy. */
export const AUTO_CHECK_INITIAL_DELAY_MS = 15_000;
/** Background re-check cadence while the app runs. */
export const AUTO_CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;
/** Obviously-fake version for the press-and-hold simulation. */
const SIMULATED_VERSION = "9.9.9-sim";
/** Fake payload size for the simulated download. */
const SIMULATED_BYTES = 24 * 1024 * 1024;

export type UpdatePhase =
  | "idle"
  | "checking"
  | "available"
  | "downloading"
  | "ready"
  | "error";

/**
 * In-app updates: a silent background check on boot + interval, with manual
 * check, download, install, and relaunch. Downloads and relaunches are always
 * explicit; only the check runs on its own.
 */
class UpdaterStore {
  phase = $state<UpdatePhase>("idle");
  /** True once a check completed; distinguishes "up to date" from "never checked". */
  checked = $state(false);
  version = $state<string | null>(null);
  notes = $state<string | null>(null);
  error = $state<string | null>(null);
  downloadedBytes = $state(0);
  totalBytes = $state<number | null>(null);
  /** Epoch seconds of the last successful check; null when never checked. */
  lastCheckedAt = $state<number | null>(null);
  /** True while the press-and-hold simulation is driving update UI. */
  simulated = $state(false);
  private pending: Update | null = null;
  private autoTimer: ReturnType<typeof setInterval> | null = null;
  private autoDelay: ReturnType<typeof setTimeout> | null = null;
  private simTimer: ReturnType<typeof setInterval> | null = null;

  async checkForUpdates(opts: { silent?: boolean } = {}): Promise<void> {
    // A real check always exits the simulation first.
    if (this.simulated) this.clearSimulation();
    if (this.phase === "checking" || this.phase === "downloading") return;
    const silent = opts.silent === true;
    // Something actionable is already pending; a background tick must not
    // regress "ready" (installed, awaiting relaunch) back to "available".
    if (silent && (this.phase === "available" || this.phase === "ready")) return;
    // Silent background checks never flash checking/error states; they only
    // surface when an update actually appears.
    if (!silent) {
      this.phase = "checking";
      this.error = null;
    }
    try {
      const update = await check();
      if (this.pending) {
        await this.pending.close();
        this.pending = null;
      }
      this.lastCheckedAt = Math.floor(Date.now() / 1000);
      this.error = null;
      if (!update) {
        this.version = null;
        this.notes = null;
        this.checked = true;
        // A silent check that recovers from an earlier manual failure clears it.
        if (!silent || this.phase === "error") this.phase = "idle";
        return;
      }
      this.pending = update;
      this.version = update.version;
      this.notes = update.body ?? null;
      this.checked = true;
      this.phase = "available";
    } catch (e) {
      if (silent) {
        console.debug("ubra: background update check failed", e);
        return;
      }
      this.version = null;
      this.phase = "error";
      this.error = e instanceof Error ? e.message : String(e);
    }
  }

  async downloadAndInstall(): Promise<void> {
    if (this.phase === "downloading") return;
    if (this.simulated && !this.pending) {
      this.runSimulatedDownload();
      return;
    }
    const update = this.pending;
    if (!update) return;
    this.phase = "downloading";
    this.error = null;
    this.downloadedBytes = 0;
    this.totalBytes = null;
    try {
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          this.totalBytes = event.data.contentLength ?? null;
        } else if (event.event === "Progress") {
          this.downloadedBytes += event.data.chunkLength;
        }
      });
      this.pending = null;
      this.phase = "ready";
    } catch (e) {
      this.phase = "error";
      this.error = e instanceof Error ? e.message : String(e);
    }
  }

  async relaunchApp(): Promise<void> {
    if (this.simulated) {
      // A simulated install has nothing to restart into; dismiss instead.
      this.clearSimulation();
      return;
    }
    await relaunch();
  }

  /** Press-and-hold entry: fake an available update, or dismiss the fake. */
  toggleSimulation(): void {
    if (this.simulated) {
      this.clearSimulation();
      return;
    }
    if (this.phase === "checking" || this.phase === "downloading") return;
    if (this.pending) {
      void this.pending.close();
      this.pending = null;
    }
    this.version = SIMULATED_VERSION;
    this.notes =
      "Simulated update for UI testing — press and hold the app version " +
      "in the status bar again to dismiss.";
    this.error = null;
    this.downloadedBytes = 0;
    this.totalBytes = null;
    this.checked = true;
    this.simulated = true;
    this.phase = "available";
  }

  clearSimulation(): void {
    this.clearSimTimer();
    this.simulated = false;
    this.version = null;
    this.notes = null;
    this.error = null;
    this.downloadedBytes = 0;
    this.totalBytes = null;
    this.checked = false;
    this.phase = "idle";
  }

  private runSimulatedDownload(): void {
    this.phase = "downloading";
    this.error = null;
    this.downloadedBytes = 0;
    this.totalBytes = SIMULATED_BYTES;
    this.clearSimTimer();
    const step = SIMULATED_BYTES / 40;
    this.simTimer = setInterval(() => {
      const total = this.totalBytes ?? SIMULATED_BYTES;
      this.downloadedBytes = Math.min(total, this.downloadedBytes + step);
      if (this.downloadedBytes >= total) {
        this.clearSimTimer();
        this.phase = "ready";
      }
    }, 75);
  }

  private clearSimTimer(): void {
    if (this.simTimer) clearInterval(this.simTimer);
    this.simTimer = null;
  }

  /** Start the delayed first check + interval; safe to call repeatedly. */
  startAutoCheck(): void {
    if (this.autoTimer || this.autoDelay) return;
    this.autoDelay = setTimeout(() => {
      this.autoDelay = null;
      void this.checkForUpdates({ silent: true });
      this.autoTimer = setInterval(() => {
        void this.checkForUpdates({ silent: true });
      }, AUTO_CHECK_INTERVAL_MS);
    }, AUTO_CHECK_INITIAL_DELAY_MS);
  }

  stopAutoCheck(): void {
    if (this.autoDelay) clearTimeout(this.autoDelay);
    if (this.autoTimer) clearInterval(this.autoTimer);
    this.autoDelay = null;
    this.autoTimer = null;
  }
}

export const updater = new UpdaterStore();
