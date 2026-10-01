// Explicit-launch authorization for pane commands.
// Nothing here is serialized: restore always opens shells until an explicit launch authorizes.
import type { PaneNode } from "./layout.ts";

export class StartupCommands {
  private authorized = new Set<string>();

  authorize(paneIds: Iterable<string>): void {
    for (const id of paneIds) this.authorized.add(id);
  }

  resolve(pane: PaneNode): string[] | undefined {
    const authorized = this.authorized.has(pane.id);
    this.authorized.delete(pane.id);
    if (pane.cmdOnRestore !== false) return pane.cmd;
    if (authorized) return pane.cmd;
    return undefined;
  }

  retain(paneIds: ReadonlySet<string>): void {
    for (const id of [...this.authorized]) {
      if (!paneIds.has(id)) this.authorized.delete(id);
    }
  }

  clear(): void {
    this.authorized.clear();
  }
}
