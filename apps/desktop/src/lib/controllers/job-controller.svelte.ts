import * as native from "$lib/api/native";
import * as commands from "$lib/api/tauri";
import type { JobEvent, JobSnapshot } from "$lib/model/types";
import { upsertJob } from "$lib/utils/jobs";
import { RequestGeneration } from "$lib/utils/request-generation";

export class JobController {
  jobs = $state<JobSnapshot[]>([]);
  loading = $state(true);
  messages = $state<Record<string, string>>({});
  private generation = new RequestGeneration();
  private revision = 0;
  private updates = new Map<string, { revision: number; snapshot: JobSnapshot }>();
  private disposed = false;

  constructor(private readonly api = { listJobs: commands.listJobs, listen: native.listenToJobEvents }) {}

  async load() {
    if (this.disposed) return;
    const generation = this.generation.invalidate();
    const revision = this.revision;
    this.loading = true;

    try {
      let jobs = await this.api.listJobs();
      if (!this.generation.current(generation)) return;
      // A list read may have started before a live progress/finish event. Reapply
      // only events received during that read; older events cannot resurrect jobs
      // deliberately removed by a later authoritative list.
      for (const [id, update] of this.updates) {
        if (update.revision > revision) jobs = upsertJob(jobs, update.snapshot);
        else this.updates.delete(id);
      }
      this.jobs = jobs;
    } finally {
      if (this.generation.current(generation)) this.loading = false;
    }
  }

  setMessage(jobId: string, message: string) {
    this.messages = {
      ...this.messages,
      [jobId]: message,
    };
  }

  applyEvent(event: JobEvent) {
    if (this.disposed) return;
    this.updates.set(event.jobId, { revision: ++this.revision, snapshot: event.snapshot });
    this.jobs = upsertJob(this.jobs, event.snapshot);

    if (event.message) {
      this.setMessage(event.jobId, event.message);
    }
  }

  listen(handler: (event: JobEvent) => void | Promise<void>) {
    return this.api.listen((event) => {
      if (this.disposed) return;
      this.applyEvent(event);
      void handler(event);
    });
  }

  dispose() {
    this.disposed = true;
    this.generation.invalidate();
    this.updates.clear();
  }
}
