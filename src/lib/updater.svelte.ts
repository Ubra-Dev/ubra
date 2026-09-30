import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export type UpdatePhase =
  | "idle"
  | "checking"
  | "available"
  | "downloading"
  | "ready"
  | "error";

/** Manual in-app updates: check, download, install, relaunch. No polling. */
class UpdaterStore {
  phase = $state<UpdatePhase>("idle");
  /** True once a check completed; distinguishes "up to date" from "never checked". */
  checked = $state(false);
  version = $state<string | null>(null);
  notes = $state<string | null>(null);
  error = $state<string | null>(null);
  downloadedBytes = $state(0);
  totalBytes = $state<number | null>(null);
  private pending: Update | null = null;

  async checkForUpdates(): Promise<void> {
    if (this.phase === "checking" || this.phase === "downloading") return;
    this.phase = "checking";
    this.error = null;
    try {
      const update = await check();
      if (this.pending) {
        await this.pending.close();
        this.pending = null;
      }
      if (!update) {
        this.version = null;
        this.notes = null;
        this.checked = true;
        this.phase = "idle";
        return;
      }
      this.pending = update;
      this.version = update.version;
      this.notes = update.body ?? null;
      this.checked = true;
      this.phase = "available";
    } catch (e) {
      this.version = null;
      this.phase = "error";
      this.error = e instanceof Error ? e.message : String(e);
    }
  }

  async downloadAndInstall(): Promise<void> {
    const update = this.pending;
    if (!update || this.phase === "downloading") return;
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
    await relaunch();
  }
}

export const updater = new UpdaterStore();
