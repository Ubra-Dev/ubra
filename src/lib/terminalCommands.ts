export interface TerminalControls {
  selection(): string;
  paste(text: string): void;
  selectAll(): void;
  running(): boolean;
}
export interface TerminalRestart {
  available(): boolean;
  restart(): void;
}

/** Registrations belong to component instances so a stale unmount cannot remove a newer one. */
export class TerminalCommandRegistry {
  private controls = new Map<string, TerminalControls>();
  private restarts = new Map<string, TerminalRestart>();
  private listeners = new Set<() => void>();

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => { this.listeners.delete(listener); };
  }
  changed(): void { for (const listener of this.listeners) listener(); }

  register(id: string, controls: TerminalControls): () => void {
    this.controls.set(id, controls);
    this.changed();
    return () => {
      if (this.controls.get(id) === controls) { this.controls.delete(id); this.changed(); }
    };
  }
  registerRestart(id: string, restart: TerminalRestart): () => void {
    this.restarts.set(id, restart);
    this.changed();
    return () => {
      if (this.restarts.get(id) === restart) { this.restarts.delete(id); this.changed(); }
    };
  }
  get(id: string | undefined): TerminalControls | undefined { return id ? this.controls.get(id) : undefined; }
  canRestart(id: string | undefined): boolean { return !!id && !!this.restarts.get(id)?.available(); }
  restart(id: string): boolean {
    const target = this.restarts.get(id);
    if (!target?.available()) return false;
    target.restart();
    this.changed();
    return true;
  }
}

export const terminalCommands = new TerminalCommandRegistry();
