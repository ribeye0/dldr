import { describe, expect, it } from "vitest";
import type { DownloadQueueEntry } from "$lib/api/tauri";
import type { JobSnapshot } from "$lib/model/types";
import { activeDownloadWorks, downloadCenterSummary } from "./download-center";
const entries = (statuses: string[]) => statuses.map((status, i) => ({ workId: `RJ${i}`, status } as DownloadQueueEntry));
const work = { workId: "RJ0", title: "One", phase: "downloading", bytesReceived: 20, bytesTotal: 100, bytesPerSecond: 5, rateUpdatedAt: 10000 };
const job = (status: JobSnapshot["status"] = "running"): JobSnapshot => ({
  id: "batch", kind: "bulkWorkDownload", title: "Batch", status, phase: "bulkDownloading",
  progress: null, metadata: {}, output: null, error: null, cancellable: true,
  createdAt: "2026-11-16T00:00:00Z", startedAt: null, finishedAt: null,
  progressDetails: { download: { totalWorks: 5, completedWorks: 1, succeededWorks: 1,
    failedWorks: 0, bytesReceived: 20, activeWorks: [work] } },
});
describe("download center summaries", () => {
  it("shows recovered paused/failed entries without any live jobs", () => {
    expect(downloadCenterSummary(entries(["paused", "failed", "completed"]), [], 11000)).toMatchObject({ running: 0, queued: 0, paused: 1, failed: 1, percent: 66, rate: 0 });
  });
  it("counts persistent works rather than their parent batch jobs", () => {
    expect(downloadCenterSummary(entries(["running", "running", "queued", "queued", "completed"]), [job()], 11000)).toMatchObject({ running: 2, queued: 2, percent: 20, rate: 5 });
  });
  it("never uses stale progress from a terminal batch for a resumed work", () => {
    expect(activeDownloadWorks([job("cancelled")]).size).toBe(0);
    expect(activeDownloadWorks([job("cancelled"), job()]).get("RJ0")).toEqual(work);
    expect(downloadCenterSummary([], [job()], 14000).rate).toBe(0);
  });
});
