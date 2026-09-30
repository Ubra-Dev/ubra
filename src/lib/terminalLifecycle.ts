import { invoke } from "@tauri-apps/api/core";

export interface TerminalOutput { id: number; data: string; sequence: number }
export interface TerminalExit { id: number; success: boolean; code: number | null }
export interface TerminalSnapshot { data: string; sequence: number; cols: number; rows: number }

/** Daemon attach-or-create outcome for one stable pane key. */
export interface AttachResult {
  ok: boolean;
  pane: number;
  attached: "reused" | "created" | "recovered" | "exited" | "unavailable" | string;
  resumed: boolean;
  epoch: number;
  incarnation: number;
  note?: string | null;
  error?: string | null;
  exit?: { success: boolean; code: number | null } | null;
  replay?: string | null;
  replayPages?: number | null;
  replayTruncated?: boolean | null;
}

/** One page of an immutable daemon snapshot. */
export interface SnapshotPage {
  pane: number;
  page: number;
  pages: number;
  data: string;
  sequence: number;
  incarnation: number;
  epoch: number;
  cols: number;
  rows: number;
}

/** One page of persisted recovery history. */
export interface ReplayPage {
  key: string;
  page: number;
  pages: number;
  data: string;
}

export interface DaemonStatus {
  connected: boolean;
  epoch?: number | null;
  reconnected?: boolean;
  error?: string | null;
}

/**
 * Snapshot failures mean transport/identity trouble, never natural exit:
 * retained exits snapshot fine, and live panes only fail when the daemon
 * is gone or replaced. Retry the attach a bounded number of times, then
 * surface unavailable-with-retry instead of a fake exit.
 */
export type SnapshotFailurePlan = "retry" | "unavailable";
export const MAX_SNAPSHOT_FAILURES = 3;
export function snapshotFailurePlan(failures: number): SnapshotFailurePlan {
  return failures < MAX_SNAPSHOT_FAILURES ? "retry" : "unavailable";
}

/** Listeners precede spawn/snapshot. Painting and replay are one synchronous cutover. */
export class TerminalAttachment {
  private id: number | null = null;
  private sequence = 0;
  private attached = false;
  private outputs = new Map<number, TerminalOutput[]>();
  private exits = new Map<number, TerminalExit>();

  output(event: TerminalOutput): string | null {
    if (!this.attached) {
      const buffered = this.outputs.get(event.id) ?? [];
      buffered.push(event);
      this.outputs.set(event.id, buffered);
      return null;
    }
    if (event.id !== this.id || event.sequence <= this.sequence) return null;
    this.sequence = event.sequence;
    return event.data;
  }

  exit(event: TerminalExit): TerminalExit | null {
    if (!this.attached) { this.exits.set(event.id, event); return null; }
    return event.id === this.id ? event : null;
  }

  restore(id: number, snapshot: TerminalSnapshot | null): { chunks: string[]; exit: TerminalExit | null } {
    this.id = id;
    this.sequence = snapshot?.sequence ?? 0;
    const chunks = snapshot ? [snapshot.data] : [];
    for (const output of (this.outputs.get(id) ?? []).sort((a, b) => a.sequence - b.sequence)) {
      if (output.sequence > this.sequence) { chunks.push(output.data); this.sequence = output.sequence; }
    }
    const exit = this.exits.get(id) ?? null;
    this.outputs.clear();
    this.exits.clear();
    this.attached = true;
    return { chunks, exit };
  }
}

/**
 * Assemble a full snapshot from its head page plus a page fetcher. Pure
 * apart from the injected fetcher, so unit tests cover paging directly.
 */
export async function assembleSnapshot(
  head: SnapshotPage,
  fetchPage: (page: number) => Promise<string>,
): Promise<TerminalSnapshot> {
  let data = head.data;
  for (let page = 1; page < head.pages; page++) {
    data += await fetchPage(page);
  }
  return { data, sequence: head.sequence, cols: head.cols, rows: head.rows };
}

/** Fetch every snapshot page for a live pane from the daemon. */
export async function fetchSnapshot(
  id: number,
  epoch: number,
  incarnation: number,
): Promise<TerminalSnapshot> {
  const head = await invoke<SnapshotPage>("pty_snapshot", { id, epoch, incarnation });
  return assembleSnapshot(head, async (page) => {
    const next = await invoke<SnapshotPage>("pty_snapshot_page", {
      id,
      page,
      epoch,
      incarnation,
    });
    return next.data;
  });
}

/**
 * Assemble persisted recovery history. Small histories arrive inline;
 * truncated ones page from the daemon record.
 */
export async function assembleReplay(
  inline: string | null | undefined,
  pages: number | null | undefined,
  truncated: boolean | null | undefined,
  fetchPage: (page: number) => Promise<string>,
): Promise<string> {
  if (!truncated) return inline ?? "";
  const count = Math.max(pages ?? 1, 1);
  let out = "";
  for (let page = 0; page < count; page++) {
    out += await fetchPage(page);
  }
  return out;
}

/** Fetch a pane's full persisted recovery history. */
export async function fetchReplay(
  key: string,
  result: Pick<AttachResult, "replay" | "replayPages" | "replayTruncated">,
): Promise<string> {
  return assembleReplay(result.replay, result.replayPages, result.replayTruncated, async (page) => {
    const next = await invoke<ReplayPage>("pty_replay_page", { key, page });
    return next.data;
  });
}
