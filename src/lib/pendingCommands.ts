/** Commands queued to type into panes on their next terminal spawn. */
export class PendingCommands {
  private readonly queued = new Map<string, string>();

  queue(paneId: string, command: string): void {
    const trimmed = command.trim();
    if (!trimmed) return;
    this.queued.set(paneId, trimmed);
  }

  /** Take the queued command for a pane, clearing it. */
  take(paneId: string): string | null {
    const command = this.queued.get(paneId) ?? null;
    this.queued.delete(paneId);
    return command;
  }

  drop(paneId: string): void {
    this.queued.delete(paneId);
  }

  dropMany(paneIds: readonly string[]): void {
    for (const id of paneIds) this.queued.delete(id);
  }
}
