import { invoke } from "@tauri-apps/api/core";
import type { CliUsage, SupportedUsageCli } from "./usage";

/** Freshness window matching the backend cache TTL. */
const FRESH_SECONDS = 5 * 60;

class UsageStore {
  /** Latest result per CLI stem. */
  entries = $state<Record<string, CliUsage>>({});
  /** In-flight fetches per CLI stem. */
  loading = $state<Record<string, boolean>>({});
  /** Subscription CLIs with a usage provider; null until loaded. */
  supported = $state<SupportedUsageCli[] | null>(null);
  private inflight = new Map<string, Promise<void>>();
  private supportedInflight: Promise<SupportedUsageCli[]> | null = null;

  /** Provider registry; concurrent callers share the fetch. */
  ensureSupported(): Promise<SupportedUsageCli[]> {
    if (this.supported) return Promise.resolve(this.supported);
    if (!this.supportedInflight) {
      this.supportedInflight = invoke<SupportedUsageCli[]>(
        "supported_usage_clis",
      )
        .then((supported) => {
          this.supported = supported;
          return supported;
        })
        .catch((error: unknown) => {
          console.error("ubra: usage providers fetch failed", error);
          this.supportedInflight = null;
          return [];
        });
    }
    return this.supportedInflight;
  }

  private isFresh(cli: string): boolean {
    const fetchedAt = this.entries[cli]?.snapshot?.fetchedAt;
    if (this.entries[cli]?.status !== "ready" || !fetchedAt) return false;
    return Date.now() / 1000 - fetchedAt < FRESH_SECONDS;
  }

  /**
   * Load usage for one CLI. Skips fresh entries and keeps error states
   * until a forced refresh; concurrent callers share the fetch.
   */
  load(cli: string, force = false): Promise<void> {
    const existing = this.entries[cli];
    if (
      !force &&
      existing &&
      (existing.status !== "ready" || this.isFresh(cli))
    ) {
      return Promise.resolve();
    }
    const running = this.inflight.get(cli);
    if (running) return running;
    this.loading[cli] = true;
    const job = invoke<CliUsage>("cli_usage", { cli, force })
      .then((fetched) => {
        this.entries[cli] = fetched;
      })
      .catch((error: unknown) => {
        console.error("ubra: cli usage fetch failed", error);
        this.entries[cli] = {
          cli,
          status: "offline",
          message: "Couldn't load usage — try again.",
        };
      })
      .finally(() => {
        this.loading[cli] = false;
        this.inflight.delete(cli);
      });
    this.inflight.set(cli, job);
    return job;
  }

  /** Load every listed CLI that has no fresh entry. */
  refreshAll(clis: string[]): void {
    for (const cli of clis) void this.load(cli);
  }

  /** Force a fresh fetch for every listed CLI, bypassing caches. */
  refreshAllForced(clis: string[]): void {
    for (const cli of clis) void this.load(cli, true);
  }

  /** Force a fresh fetch for one CLI, bypassing caches. */
  refresh(cli: string): void {
    void this.load(cli, true);
  }
}

export const usage = new UsageStore();
