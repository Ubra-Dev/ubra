export interface TerminalOutput { id: number; data: string; sequence: number }
export interface TerminalExit { id: number; success: boolean; code: number | null }
export interface TerminalSnapshot { data: string; sequence: number; cols: number; rows: number }
/** Live backend session: `key` is the stable pane id, null when unkeyed. */
export interface PtySessionInfo { id: number; key: string | null }

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
