import * as api from "$lib/api/tauri";
import { errorMessage } from "$lib/utils/format";

// One queue observer stays alive while the progress panel is closed or pages change.
export class DownloadQueueController {
  entries = $state<api.DownloadQueueEntry[]>([]);
  loading = $state(true);
  busy = $state(false);
  loadError = $state("");
  actionError = $state("");
  now = $state(Date.now());
  private pending: Promise<void> | null = null;
  private disposed = false;

  reload(): Promise<void> {
    if (this.pending) return this.pending;
    this.pending = (async () => {
      try {
        const entries = await api.listDownloadQueue();
        if (!this.disposed) { this.entries = entries; this.loadError = ""; }
      } catch (cause) {
        if (!this.disposed) this.loadError = errorMessage(cause);
      } finally {
        if (!this.disposed) this.loading = false;
        this.pending = null;
      }
    })();
    return this.pending;
  }

  start() {
    void this.reload();
    const poll = setInterval(() => void this.reload(), 1500);
    const clock = setInterval(() => { this.now = Date.now(); }, 500);
    return () => { this.disposed = true; clearInterval(poll); clearInterval(clock); };
  }

  async run(action: () => Promise<unknown>, after?: () => Promise<unknown>) {
    if (this.busy) return;
    this.busy = true;
    this.actionError = "";
    try { await action(); }
    catch (cause) { if (!this.disposed) this.actionError = errorMessage(cause); }
    finally {
      // Drain any older read before fetching the state after this mutation.
      await this.pending;
      if (!this.disposed) {
        try { await this.reload(); await after?.(); }
        finally { this.busy = false; }
      }
    }
  }

  async pauseAll() {
    const errors: string[] = [];
    for (const entry of this.entries.filter(e => ["queued", "running"].includes(e.status))) {
      try { await api.controlDownloadQueue(entry.workId, "pause"); }
      catch (cause) { errors.push(`${entry.workId}: ${errorMessage(cause)}`); }
    }
    if (errors.length) throw new Error(errors.join("\n"));
  }
}
