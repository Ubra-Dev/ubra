import { invoke } from "@tauri-apps/api/core";
import type { DetectedCli } from "./agentClis";

class AgentCliStore {
  /** Detected CLIs; null until the first scan resolves. */
  clis = $state<DetectedCli[] | null>(null);
  private inflight: Promise<DetectedCli[]> | null = null;

  /** Resolve installed CLIs once; concurrent callers share the scan. */
  ensure(): Promise<DetectedCli[]> {
    if (!this.inflight) {
      this.inflight = invoke<DetectedCli[]>("detect_agent_clis")
        .then((clis) => {
          this.clis = clis;
          return clis;
        })
        .catch((error: unknown) => {
          console.error("ubra: agent CLI detection failed", error);
          this.inflight = null;
          this.clis = [];
          return [];
        });
    }
    return this.inflight;
  }
}

export const agentClis = new AgentCliStore();
