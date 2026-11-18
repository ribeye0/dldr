import { describe, expect, it } from "vitest";
import type { DownloadWorkProgress, JobSnapshot } from "$lib/model/types";
import { downloadMetrics, downloadsSummary, progressPercent, updateDownloadSession, workBytesLabel, workRate } from "./download-progress";

const work = (overrides: Partial<DownloadWorkProgress> = {}): DownloadWorkProgress => ({
  workId: "RJ000001", title: "Current work", phase: "downloading", bytesReceived: 100,
  bytesTotal: 400, bytesPerSecond: 50, rateUpdatedAt: 10_000, ...overrides,
});
function job(id: string, works: DownloadWorkProgress[], overrides: Partial<JobSnapshot> = {}): JobSnapshot {
  return {
    id, kind: "bulkWorkDownload", status: "running", progressDetails: {
      download: { totalWorks: 4, completedWorks: 1, succeededWorks: 1, failedWorks: 0, bytesReceived: 500, activeWorks: works },
    }, ...overrides,
  } as JobSnapshot;
}

describe("download progress display", () => {
  it("retains finished jobs in a queue's denominator and resets for the next queue", () => {
    const running = job("a", [work()]);
    const queued = job("b", [], { status: "queued" });
    const done = { ...running, status: "succeeded" as const };
    expect(updateDownloadSession([], [done, queued])).toEqual(["b"]);
    expect(updateDownloadSession(["a", "b"], [done, queued])).toEqual(["a", "b"]);
    expect(updateDownloadSession(["a"], [done])).toEqual(["a"]);
    expect(updateDownloadSession(["a"], [done, queued])).toEqual(["b"]);
    expect(updateDownloadSession(["a"], [running, queued])).toEqual(["a", "b"]);
  });
  it("sums active transfers across jobs and per-work snapshots", () => {
    const jobs = [job("a", [work(), work({ workId: "RJ000002", bytesPerSecond: 75 })]), job("b", [work({ bytesPerSecond: 25 })])];
    expect(downloadsSummary(jobs, 11_000)).toMatchObject({ total: 8, completed: 2, activeWorks: 3, bytes: 1000, rate: 150, percent: 25 });
  });

  it("zeros stalled, extracting and terminal transfer rates", () => {
    expect(workRate(work(), 13_001)).toBe(0);
    expect(workRate(work({ phase: "unpacking" }), 11_000)).toBe(0);
    expect(workRate(work(), 11_000, false)).toBe(0);
    expect(downloadsSummary([job("a", [work()], { status: "cancelled" })], 11_000).rate).toBe(0);
  });

  it("distinguishes measuring from a measured zero speed", () => {
    expect(workRate(work({ bytesPerSecond: null }), 11_000)).toBeNull();
    expect(workRate(work({ bytesPerSecond: 0 }), 11_000)).toBe(0);
    expect(downloadsSummary([job("a", [work({ bytesPerSecond: null })])], 11_000).rate).toBeNull();
  });

  it("keeps unknown totals indeterminate and clamps progress", () => {
    expect(progressPercent(100, null)).toBeNull();
    expect(progressPercent(0, 0)).toBeNull();
    expect(progressPercent(500, 400)).toBe(100);
    expect(workBytesLabel(work({ bytesTotal: null }))).toContain("total size unknown");
  });

  it("handles old snapshots and malformed metrics without inventing a speed", () => {
    expect(downloadMetrics({ progressDetails: { download: { totalWorks: NaN } } } as unknown as JobSnapshot)).toBeNull();
    const legacy = { kind: "workDownload", status: "running", progress: { current: 20, total: 40, unit: "bytes" }, metadata: {} } as JobSnapshot;
    expect(downloadsSummary([legacy], 11_000)).toMatchObject({ total: 1, completed: 0, bytes: 20, rate: null });
  });
});
