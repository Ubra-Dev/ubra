import { invoke } from "@tauri-apps/api/core";
import { store } from "./store.svelte";
import { workspaceDir } from "./workspaceGit";

/** Poll cadence for workspace branches; checkouts happen outside our view. */
export const GIT_BRANCH_POLL_MS = 10_000;

class WorkspaceGitStore {
  branches = $state<Record<string, string | null>>({});
  private started = false;
  private runId = 0;
  private timer: ReturnType<typeof setInterval> | null = null;
  private backendDown = false;

  branchFor(workspaceId: string): string | null {
    return this.branches[workspaceId] ?? null;
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

  /** Resolve the current branch for every workspace with a directory. */
  async refresh(): Promise<void> {
    if (this.backendDown) return;
    const layout = store.layout;
    if (!layout) return;
    const run = ++this.runId;
    const live = new Set(layout.workspaces.map((ws) => ws.id));
    for (const id of Object.keys(this.branches)) {
      if (!live.has(id)) delete this.branches[id];
    }
    let failures = 0;
    await Promise.all(
      layout.workspaces.map(async (ws) => {
        const dir = workspaceDir(ws);
        if (dir === null) {
          if (run === this.runId) delete this.branches[ws.id];
          return;
        }
        try {
          const branch = await invoke<string | null>("git_branch", { path: dir });
          if (run === this.runId) this.branches[ws.id] = branch;
        } catch {
          failures += 1;
          if (run === this.runId) delete this.branches[ws.id];
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
      console.error("ubra: git_branch unavailable; workspace branch polling stopped");
    }
  }
}

export const workspaceGit = new WorkspaceGitStore();
