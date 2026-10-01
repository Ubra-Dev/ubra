import { gitStatus, summarizeStatus, type GitSummary } from "./git";
import { store } from "./store.svelte";
import { workspaceDir } from "./workspaceGit";

/** Poll cadence for workspace branches; checkouts happen outside our view. */
export const GIT_BRANCH_POLL_MS = 10_000;

class WorkspaceGitStore {
  branches = $state<Record<string, string | null>>({});
  summaries = $state<Record<string, GitSummary | null>>({});
  private started = false;
  private runId = 0;
  private timer: ReturnType<typeof setInterval> | null = null;
  private backendDown = false;

  branchFor(workspaceId: string): string | null {
    return this.branches[workspaceId] ?? null;
  }

  summaryFor(workspaceId: string): GitSummary | null {
    return this.summaries[workspaceId] ?? null;
  }

  start(): void {
    if (this.started) return;
    this.started = true;
    void this.refresh();
    this.timer = setInterval(() => void this.refresh(), GIT_BRANCH_POLL_MS);
    window.addEventListener("focus", this.onFocus);
    document.addEventListener("visibilitychange", this.onVisibility);
  }

  private onFocus = (): void => {
    void this.refresh();
  };

  private onVisibility = (): void => {
    if (document.visibilityState === "visible") void this.refresh();
  };

  /** Resolve branch plus change counts for every workspace with a directory. */
  async refresh(): Promise<void> {
    if (this.backendDown) return;
    const layout = store.layout;
    if (!layout) return;
    const run = ++this.runId;
    const live = new Set(layout.workspaces.map((ws) => ws.id));
    for (const id of Object.keys(this.branches)) {
      if (!live.has(id)) delete this.branches[id];
    }
    for (const id of Object.keys(this.summaries)) {
      if (!live.has(id)) delete this.summaries[id];
    }
    let failures = 0;
    await Promise.all(
      layout.workspaces.map(async (ws) => {
        const dir = workspaceDir(ws);
        if (dir === null) {
          if (run === this.runId) {
            delete this.branches[ws.id];
            delete this.summaries[ws.id];
          }
          return;
        }
        try {
          const status = await gitStatus(dir);
          if (run === this.runId) {
            this.branches[ws.id] = status.isRepo ? status.branch : null;
            this.summaries[ws.id] = status.isRepo ? summarizeStatus(status) : null;
          }
        } catch {
          failures += 1;
          if (run === this.runId) {
            delete this.branches[ws.id];
            delete this.summaries[ws.id];
          }
        }
      }),
    );
    if (
      run === this.runId &&
      layout.workspaces.length > 0 &&
      failures === layout.workspaces.length
    ) {
      // No backend (e.g. browser dev): stop polling instead of logging forever.
      this.backendDown = true;
      if (this.timer !== null) clearInterval(this.timer);
      this.timer = null;
      console.error("ubra: git status unavailable; workspace git polling stopped");
    }
  }
}

export const workspaceGit = new WorkspaceGitStore();
