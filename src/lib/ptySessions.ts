/** Pending and live PTYs belong to stable pane identities, not components. */
export interface SessionLease {
  readonly ready: Promise<number>;
  id: number | null;
  cancelled: boolean;
  /** True when the lease adopted a surviving backend session (reattach). */
  attached: boolean;
  /** Consumed by the first mount that delivers this lease; gates restore. */
  restoreTaken: boolean;
  kill: (id: number) => Promise<unknown>;
}

const sessions = new Map<string, SessionLease>();

export function acquireSession(
  key: string,
  spawn: () => Promise<number>,
  kill: (id: number) => Promise<unknown>,
): SessionLease {
  const current = sessions.get(key);
  if (current) return current;
  const lease: SessionLease = {
    id: null,
    cancelled: false,
    attached: false,
    restoreTaken: false,
    kill,
    ready: Promise.resolve().then(spawn).then(async (id) => {
      lease.id = id;
      if (lease.cancelled) await kill(id);
      return id;
    }).catch((error) => {
      if (sessions.get(key) === lease) sessions.delete(key);
      throw error;
    }),
  };
  sessions.set(key, lease);
  return lease;
}

/** A true close removes ownership immediately and kills exactly once. */
export function closeSession(key: string): void {
  const lease = sessions.get(key);
  if (!lease) return;
  sessions.delete(key);
  lease.cancelled = true;
  if (lease.id !== null) void lease.kill(lease.id).catch(console.error);
}

/**
 * Drop ownership without killing: the daemon is gone (or never answered),
 * so there is nothing to kill. In-flight spawns still self-clean on
 * resolution via the shared cancelled flag. Returns evicted leases.
 */
export function evictSessions(keys: Iterable<string>): SessionLease[] {
  const evicted: SessionLease[] = [];
  for (const key of keys) {
    const lease = sessions.get(key);
    if (!lease) continue;
    sessions.delete(key);
    lease.cancelled = true;
    evicted.push(lease);
  }
  return evicted;
}

/** Evict only leases that never resolved a live id (failed boot spawns). */
export function evictDeadSessions(keys: Iterable<string>): SessionLease[] {
  return evictSessions(
    [...keys].filter((key) => sessions.get(key)?.id === null),
  );
}

/** Late exits cannot delete a newer lease. */
export function dropSession(key: string, lease: SessionLease): void {
  if (sessions.get(key) === lease) sessions.delete(key);
}
