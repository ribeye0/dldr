import type { DownloadQueueEntry } from "$lib/api/tauri";
import type { DownloadWorkProgress, JobSnapshot } from "$lib/model/types";
import { downloadMetrics, downloadsSummary, progressPercent } from "./download-progress";
import { isActiveJob } from "./jobs";

export function activeDownloadWorks(jobs: JobSnapshot[]) {
  const works = new Map<string, DownloadWorkProgress>();
  for (const job of jobs.filter(isActiveJob)) {
    for (const work of downloadMetrics(job)?.activeWorks ?? []) works.set(work.workId, work);
  }
  return works;
}

export function downloadCenterSummary(entries: DownloadQueueEntry[], jobs: JobSnapshot[], now: number) {
  const progress = downloadsSummary(jobs, now);
  const count = (statuses: string[]) => entries.filter(e => statuses.includes(e.status)).length;
  const running = entries.length ? count(["running", "pausing", "cancelling"]) : progress.activeWorks;
  const queued = entries.length ? count(["queued"]) : jobs.filter(j => j.status === "queued").length;
  return {
    ...progress,
    total: progress.total || entries.length,
    completed: progress.total ? progress.completed : count(["completed", "failed", "cancelled"]),
    activeWorks: running,
    percent: progress.total ? progress.percent : progressPercent(count(["completed", "failed", "cancelled"]), entries.length),
    running, queued,
    paused: count(["paused"]),
    failed: Math.max(count(["failed"]), progress.failed, jobs.filter(j => j.status === "failed").length),
  };
}
