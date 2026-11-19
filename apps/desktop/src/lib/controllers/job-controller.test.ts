import { describe, expect, it, vi } from "vitest";
import type { JobEvent, JobSnapshot } from "$lib/model/types";
import { JobController } from "./job-controller.svelte";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(yes => { resolve = yes; });
  return { promise, resolve };
}
const job = (status: JobSnapshot["status"]): JobSnapshot => ({ id: "fixture", kind: "workDownload", title: "Fixture", status, phase: null, progress: null, metadata: {}, output: null, error: null, cancellable: true, createdAt: "", startedAt: null, finishedAt: null });
const event = (snapshot: JobSnapshot): JobEvent => ({ sequence: 1, eventKind: "updated", jobId: snapshot.id, kind: snapshot.kind, status: snapshot.status, phase: null, progress: null, message: null, log: null, snapshot });
function fixture() {
  const api = { listJobs: vi.fn(async (): Promise<JobSnapshot[]> => []), listen: vi.fn(async (_handler: (event: JobEvent) => void) => () => {}) };
  return { api, controller: new JobController(api) };
}

describe("job snapshots and live events", () => {
  it("keeps live completions when an older list read finishes, without resurrecting cleared jobs", async () => {
    const { controller, api } = fixture();
    const old = deferred<JobSnapshot[]>(); api.listJobs.mockReturnValueOnce(old.promise);
    const loading = controller.load(); controller.applyEvent(event(job("succeeded")));
    old.resolve([job("running")]); await loading;
    expect(controller.jobs[0].status).toBe("succeeded");
    await controller.load(); expect(controller.jobs).toEqual([]);
  });

  it("ignores replaced list requests and callbacks after disposal", async () => {
    const { controller, api } = fixture();
    const old = deferred<JobSnapshot[]>(); api.listJobs.mockReturnValueOnce(old.promise);
    const first = controller.load(); await controller.load();
    old.resolve([job("running")]); await first;
    expect(controller.jobs).toEqual([]);
    const last = deferred<JobSnapshot[]>(); api.listJobs.mockReturnValueOnce(last.promise);
    const pending = controller.load(); controller.dispose();
    last.resolve([job("queued")]); await pending;
    controller.applyEvent(event(job("succeeded")));
    expect(controller.jobs).toEqual([]);
  });
});
