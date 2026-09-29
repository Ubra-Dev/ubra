/**
 * Live PTY registry keyed by pane node id.
 *
 * A pane's Svelte component remounts when the pane moves across tabs or
 * workspaces (each tab owns its own keyed block), but the backend PTY must
 * survive: killing it would destroy the running process. The remounted
 * terminal reattaches to the registered live id and repaints from a backend
 * screen snapshot. Entries are dropped on process exit and on true closes.
 */
const liveByNode = new Map<string, number>();

export function peekLiveId(sessionKey: string): number | null {
  return liveByNode.get(sessionKey) ?? null;
}

export function claimLiveId(sessionKey: string, liveId: number): void {
  liveByNode.set(sessionKey, liveId);
}

/**
 * Drop the entry only when it still points at `liveId`, so a stale
 * dispose/exit never removes a newer session for the same pane.
 */
export function dropLiveId(sessionKey: string, liveId: number): void {
  if (liveByNode.get(sessionKey) === liveId) liveByNode.delete(sessionKey);
}
