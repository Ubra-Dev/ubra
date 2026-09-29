export interface Toast {
  id: number;
  title: string;
  body: string;
  nodeId: string;
}

const MAX_TOASTS = 5;
const DISMISS_MS = 8000;

class ToastStore {
  items = $state<Toast[]>([]);
  private nextId = 1;

  push(title: string, body: string, nodeId: string): void {
    const id = this.nextId++;
    this.items = [...this.items.slice(-(MAX_TOASTS - 1)), { id, title, body, nodeId }];
    setTimeout(() => this.dismiss(id), DISMISS_MS);
  }

  dismiss(id: number): void {
    this.items = this.items.filter((t) => t.id !== id);
  }
}

export const toasts = new ToastStore();
