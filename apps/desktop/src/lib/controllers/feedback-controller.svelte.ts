import type { Toast, ToastKind } from "$lib/model/types";

/** Owns transient notices and their timers; disposing prevents late async notices. */
export class NotificationController {
  toasts = $state<Toast[]>([]);
  private sequence = 0;
  private timers = new Map<string, ReturnType<typeof setTimeout>>();
  private disposed = false;

  success = (message: string) => this.push("success", message);
  info = (message: string) => this.push("info", message);
  error = (message: string) => this.push("error", message, 7000);

  dismiss = (id: string) => {
    this.toasts = this.toasts.filter(toast => toast.id !== id);
    this.clearTimer(id);
  };

  dispose() {
    this.disposed = true;
    for (const id of this.timers.keys()) this.clearTimer(id);
    this.toasts = [];
  }

  private push(kind: ToastKind, message: string, duration = 3600) {
    if (this.disposed) return;
    const id = `toast-${Date.now()}-${this.sequence++}`;
    this.toasts = [{ id, kind, message }, ...this.toasts].slice(0, 5);
    this.timers.set(id, setTimeout(() => this.dismiss(id), duration));
    const visible = new Set(this.toasts.map(toast => toast.id));
    for (const id of this.timers.keys()) if (!visible.has(id)) this.clearTimer(id);
  }

  private clearTimer(id: string) {
    clearTimeout(this.timers.get(id));
    this.timers.delete(id);
  }
}

/** A replaced or unmounted confirmation always resolves as cancelled. */
export class DecisionController<T> {
  dialog = $state<T | null>(null);
  private resolve: ((confirmed: boolean) => void) | null = null;
  private disposed = false;

  show = (dialog: T): Promise<boolean> => {
    this.close();
    if (this.disposed) return Promise.resolve(false);
    this.dialog = dialog;
    return new Promise(resolve => { this.resolve = resolve; });
  };

  close = (confirmed = false) => {
    const resolve = this.resolve;
    this.resolve = null;
    this.dialog = null;
    resolve?.(confirmed);
  };

  dispose() { this.disposed = true; this.close(); }
}

export type Notifications = Pick<NotificationController, "success" | "error">;
