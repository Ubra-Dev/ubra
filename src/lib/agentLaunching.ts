import type { AgentStatus } from "./agentStatus";

/**
 * How long a pane waits for recognized agent UI before giving up the
 * launch overlay. CLIs without a screen profile never report ready, so
 * the overlay must resolve without backend evidence.
 */
export const AGENT_LAUNCH_TIMEOUT_MS = 15_000;

/**
 * True once the backend shows recognized agent UI for this launch: a
 * named agent at its prompt (idle carries identity then), working,
 * blocked, or done. Missing status, unknown, and identity-less idle
 * (no agent process yet) all mean "not yet ready".
 *
 * Both Ready and Idle serialize as `state: "idle"`; only Ready carries
 * an agent name, so the name is the readiness signal.
 */
export function agentLaunchReady(
  status: AgentStatus | null | undefined,
): boolean {
  if (!status) return false;
  switch (status.state) {
    case "working":
    case "blocked":
    case "done":
      return true;
    case "idle":
      return (status.agent?.trim() ?? "") !== "";
    case "unknown":
      return false;
  }
}

/** True once `timeoutMs` has elapsed since the launch started. */
export function agentLaunchExpired(
  startedAt: number,
  nowMs: number,
  timeoutMs: number = AGENT_LAUNCH_TIMEOUT_MS,
): boolean {
  return nowMs - startedAt >= timeoutMs;
}
