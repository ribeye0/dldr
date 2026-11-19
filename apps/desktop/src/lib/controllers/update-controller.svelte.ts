import * as updates from "$lib/api/updates";
import type { UpdateCheck, UpdateRelease } from "$lib/api/updates";

const errorKeys: Record<string, string> = {
  update_jobs_active: "Finish or pause downloads and wait for syncing to finish before updating.",
  update_signature: "The update signature could not be verified. Nothing was installed. Please try again or download from GitHub.",
  update_filesystem: "DLDR could not replace its files. Check folder permissions and free disk space, then try again.",
  update_feed_unavailable: "Update information is not available yet. Please try again later or visit GitHub Releases.",
};

export class UpdateController {
  open = $state(false);
  phase = $state<"idle" | "checking" | "available" | "current" | "unsupported" | "downloading" | "verifying" | "installing" | "installed" | "error">("idle");
  release = $state<UpdateRelease | null>(null);
  kind = $state<UpdateCheck["kind"]>("unsupported");
  received = $state(0);
  total = $state<number | null>(null);
  error = $state("");
  busy = $derived(["checking", "downloading", "verifying", "installing"].includes(this.phase));
  private disposed = false;

  constructor(private readonly api: Pick<typeof updates, "check" | "install"> = updates) {}

  async check() {
    if (this.busy || this.disposed) return;
    this.open = true;
    this.phase = "checking";
    this.error = "";
    this.release = null;
    this.received = 0;
    this.total = null;
    try {
      const result = await this.api.check();
      if (this.disposed) return;
      this.kind = result.kind;
      this.release = result.release;
      this.phase = result.kind === "unsupported" ? "unsupported" : result.release ? "available" : "current";
    } catch (error) {
      if (!this.disposed) this.fail(error);
    }
  }

  async install() {
    if (this.busy || this.disposed || this.phase !== "available" || !this.release) return;
    this.phase = "downloading";
    this.error = "";
    try {
      await this.api.install(event => {
        if (this.disposed) return;
        if (event.event === "progress") {
          this.received = Math.max(this.received, event.downloaded);
          this.total = event.total && event.total > 0 ? event.total : null;
        } else this.phase = event.event;
      });
      if (!this.disposed) this.phase = "installed";
    } catch (error) {
      if (!this.disposed) this.fail(error);
    }
  }

  close() { if (!this.busy) this.open = false; }
  dispose() { this.disposed = true; }
  private fail(error: unknown) {
    const code = String(error).split(" [diagnostic:")[0];
    this.error = errorKeys[code] ?? "Could not update DLDR. Check your connection and try again. Details are available in Logs.";
    this.phase = "error";
  }
}
