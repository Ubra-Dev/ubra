export type ToastKind = "agent" | "copy";

export interface Toast {
  id: number;
  title: string;
  body: string;
  nodeId: string;
  kind: ToastKind;
}

export interface PushOptions {
  /** Per-toast auto-dismiss; defaults to DISMISS_MS. */
  dismissMs?: number;
  kind?: ToastKind;
}

const MAX_TOASTS = 5;
const DISMISS_MS = 8000;

class ToastStore {
  items = $state<Toast[]>([]);
  private nextId = 1;

  push(title: string, body: string, nodeId: string, opts: PushOptions = {}): void {
    const id = this.nextId++;
    const kind = opts.kind ?? "agent";
    const dismissMs = opts.dismissMs ?? DISMISS_MS;
    this.items = [
      ...this.items.slice(-(MAX_TOASTS - 1)),
      { id, title, body, nodeId, kind },
    ];
    setTimeout(() => this.dismiss(id), dismissMs);
  }

  dismiss(id: number): void {
    this.items = this.items.filter((t) => t.id !== id);
  }
}

export const toasts = new ToastStore();
