import * as updates from "$lib/api/updates";
import type { ReadyPackage, UpdateCheck, UpdateRelease } from "$lib/api/updates";

const errorKeys: Record<string, string> = {
  update_jobs_active: "Finish or pause downloads and wait for syncing to finish before updating.",
  update_signature: "The update signature could not be verified. Nothing was installed. Please try again or download from GitHub.",
  update_filesystem: "DLDR could not replace its files. Check folder permissions and free disk space, then try again.",
  update_feed_unavailable: "Update information is not available yet. Please try again later or visit GitHub Releases.",
  update_package: "This file is not a compatible DLDR package. Select the package for your current platform and installation type.",
  update_version: "The package version is missing or inconsistent. Use its original filename or enter the correct version.",
  update_unsupported: "Automatic updates are available in the Linux AppImage and Windows releases. This development or native build must be updated manually.",
  update_untrusted: "Confirm that you trust this unsigned package before installing it.",
};
type UpdateApi = Pick<typeof updates, "check" | "download" | "cancel" | "prepareLocal" | "install" | "chooseFile">;

export class UpdateController {
  open = $state(false);
  phase = $state<"idle" | "checking" | "available" | "current" | "unsupported" | "local" | "choosing" | "preparing" | "downloading" | "verifying" | "cancelling" | "ready" | "installing" | "installed" | "error">("idle");
  release = $state<UpdateRelease | null>(null);
  ready = $state<ReadyPackage | null>(null);
  kind = $state<UpdateCheck["kind"]>("unsupported");
  localPath = $state("");
  localVersion = $state("");
  trustLocal = $state(false);
  received = $state(0);
  total = $state<number | null>(null);
  error = $state("");
  busy = $derived(["checking", "choosing", "preparing", "downloading", "verifying", "cancelling", "installing"].includes(this.phase));
  canCancel = $derived(["downloading", "verifying", "preparing"].includes(this.phase));
  statusLabel = $derived(this.phase === "ready" ? "Update ready" : this.busy ? "View update progress" : "Check for updates");
  private disposed = false;
  private transferId: string | null = null;
  private transferTask: Promise<void> | null = null;
  private cancelled = false;

  constructor(private readonly api: UpdateApi = updates, private readonly onReady: () => void = () => {}) {}

  async check() {
    if (this.disposed) return;
    this.open = true;
    if (this.busy || this.phase === "ready") return;
    this.phase = "checking";
    this.error = "";
    this.release = null;
    this.ready = null;
    this.localPath = "";
    this.received = 0;
    this.total = null;
    try {
      const result = await this.api.check();
      if (this.disposed) return;
      this.kind = result.kind;
      this.release = result.release;
      this.phase = result.kind === "unsupported" ? "unsupported" : result.release ? "available" : "current";
    } catch (error) { if (!this.disposed) this.fail(error); }
  }

  async chooseLocal() {
    if (this.disposed) return;
    if (this.busy) { this.open = true; return; }
    const previous = this.phase;
    this.phase = "choosing";
    this.open = true;
    try {
      const path = await this.api.chooseFile();
      if (this.disposed) return;
      if (!path) { this.phase = previous; return; }
      this.localPath = path;
      this.localVersion = "";
      this.ready = null;
      this.trustLocal = false;
      this.error = "";
      this.phase = "local";
    } catch (error) { if (!this.disposed) this.fail(error); }
  }

  download() {
    if (this.busy || this.disposed || this.phase !== "available") return Promise.resolve();
    return this.transfer("downloading", id => this.api.download(id, event => {
      if (this.disposed || this.transferId !== id || this.cancelled) return;
      if (event.event === "progress") {
        this.received = Math.max(this.received, event.downloaded);
        this.total = event.total && event.total > 0 ? event.total : null;
      } else this.phase = "verifying";
    }));
  }

  prepareLocal() {
    if (this.busy || this.disposed || !this.localPath) return Promise.resolve();
    return this.transfer("preparing", id => this.api.prepareLocal(id, this.localPath, this.localVersion));
  }

  private transfer(phase: "downloading" | "preparing", run: (id: string) => Promise<ReadyPackage>) {
    const id = crypto.randomUUID();
    this.transferId = id;
    this.cancelled = false;
    this.phase = phase;
    this.error = "";
    this.ready = null;
    this.trustLocal = false;
    this.received = 0;
    this.total = null;
    this.transferTask = Promise.resolve().then(() => run(id)).then(packageInfo => {
      if (this.disposed || this.transferId !== id || this.cancelled) return;
      this.ready = packageInfo;
      this.release = packageInfo.release;
      this.phase = "ready";
      if (!this.open) this.onReady();
    }).catch(error => {
      if (!this.disposed && this.transferId === id && !this.cancelled) this.fail(error);
    }).finally(() => {
      if (this.transferId !== id) return;
      this.transferId = null;
      this.transferTask = null;
      if (!this.disposed && this.cancelled) {
        this.phase = this.localPath ? "local" : "available";
        this.error = "";
      }
    });
    return this.transferTask;
  }

  async cancel() {
    const id = this.transferId;
    if (!id || !this.canCancel) return;
    const task = this.transferTask;
    this.cancelled = true;
    this.phase = "cancelling";
    try {
      await this.api.cancel(id);
      // Wait until native code has released the transfer before allowing a retry.
      await task;
    } catch (error) {
      if (!this.disposed && this.transferId === id) {
        this.cancelled = false;
        this.error = errorKeys[String(error)] ?? "Could not cancel the update. Please try again.";
        this.phase = this.localPath ? "preparing" : "downloading";
      }
    }
  }

  async install() {
    if (this.disposed || this.phase !== "ready" || !this.ready) return;
    if (!this.ready.verified && !this.trustLocal) { this.error = errorKeys.update_untrusted; return; }
    this.phase = "installing";
    this.error = "";
    try {
      await this.api.install(this.trustLocal);
      if (!this.disposed) this.phase = "installed";
    } catch (error) {
      if (!this.disposed) {
        this.fail(error);
        // Keep verified bytes for retry, including when downloads are still active.
        this.phase = "ready";
      }
    }
  }

  close() { if (this.phase !== "installing") this.open = false; }
  dispose() {
    this.disposed = true;
    if (this.transferId) void this.api.cancel(this.transferId).catch(() => {});
  }
  private fail(error: unknown) {
    const code = String(error).split(" [diagnostic:")[0];
    this.error = errorKeys[code] ?? "Could not update DLDR. Check your connection and try again. Details are available in Logs.";
    this.phase = "error";
  }
}
