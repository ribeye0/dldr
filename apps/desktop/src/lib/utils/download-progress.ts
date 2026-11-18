import { translator, type UiLocale } from "$lib/i18n";
import type { DownloadMetrics, DownloadWorkProgress, JobSnapshot } from "$lib/model/types";
import { formatBytes } from "./format";
import { isActiveJob, isDownloadQueueJob, metadataNumber, recordValue } from "./jobs";

const STALE_RATE_MS = 3000;

// Keep completed jobs in the current queue's denominator until a fresh queue starts.
export function updateDownloadSession(ids: string[], jobs: JobSnapshot[]) {
  const active = jobs.filter((job) => isDownloadQueueJob(job) && isActiveJob(job)).map((job) => job.id);
  const previous = new Set(ids);
  if (active.some((id) => !previous.has(id)) && !active.some((id) => previous.has(id))) return active;
  return [...new Set([...ids, ...active])];
}
function count(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value) && value >= 0;
}

export function downloadMetrics(job: JobSnapshot): DownloadMetrics | null {
  const data = recordValue(job.progressDetails?.download);
  if (!data || !count(data.totalWorks) || !count(data.completedWorks) ||
    !count(data.succeededWorks) || !count(data.failedWorks) || !count(data.bytesReceived) ||
    !Array.isArray(data.activeWorks)) return null;
  const active = new Map<string, DownloadWorkProgress>();
  for (const raw of data.activeWorks) {
    const work = recordValue(raw);
    if (!work || typeof work.workId !== "string" || typeof work.title !== "string" ||
      typeof work.phase !== "string" || !count(work.bytesReceived) || !count(work.rateUpdatedAt) ||
      !(work.bytesTotal === null || count(work.bytesTotal)) ||
      !(work.bytesPerSecond === null || count(work.bytesPerSecond))) continue;
    active.set(work.workId, {
      workId: work.workId, title: work.title, phase: work.phase,
      bytesReceived: work.bytesReceived, bytesTotal: work.bytesTotal,
      bytesPerSecond: work.bytesPerSecond, rateUpdatedAt: work.rateUpdatedAt,
    });
  }
  return {
    totalWorks: data.totalWorks,
    completedWorks: Math.min(data.totalWorks, data.completedWorks),
    succeededWorks: data.succeededWorks,
    failedWorks: data.failedWorks,
    deferredWorks: count(data.deferredWorks) ? data.deferredWorks : 0,
    bytesReceived: data.bytesReceived,
    activeWorks: [...active.values()],
  };
}

export function progressPercent(current: number, total: number | null) {
  return total !== null && total > 0 ? Math.min(100, Math.max(0, Math.floor(current * 100 / total))) : null;
}

export function workRate(work: DownloadWorkProgress, now: number, active = true) {
  if (!active || work.phase !== "downloading" || now - work.rateUpdatedAt > STALE_RATE_MS) return 0;
  return work.bytesPerSecond;
}

export function rateLabel(rate: number | null, language: UiLocale = "en") {
  const translate = translator(language);
  return rate === null ? translate("Measuring speed…") : `${formatBytes(rate)}/s`;
}

export function workPhaseLabel(phase: string, language: UiLocale = "en") {
  const translate = translator(language);
  return ({ loggingIn: translate("Signing in"), resolvingDownload: translate("Resolving files"), downloading: translate("Downloading"), unpacking: translate("Extracting archive"), finalizing: translate("Installing files"), completed: translate("Completed") } as Record<string, string>)[phase] ?? translate("Preparing");
}

export function workBytesLabel(work: DownloadWorkProgress, language: UiLocale = "en") {
  const translate = translator(language);
  return work.bytesTotal === null
    ? translate("{0} downloaded · total size unknown", formatBytes(work.bytesReceived))
    : `${formatBytes(work.bytesReceived)} / ${formatBytes(work.bytesTotal)}`;
}

export function downloadsSummary(jobs: JobSnapshot[], now: number) {
  let total = 0, completed = 0, failed = 0, activeWorks = 0, bytes = 0, rate = 0;
  let measuring = false;
  for (const job of jobs) {
    const metrics = downloadMetrics(job);
    if (metrics) {
      total += metrics.totalWorks;
      completed += metrics.completedWorks;
      failed += metrics.failedWorks;
      bytes += metrics.bytesReceived;
      if (isActiveJob(job)) {
        activeWorks += metrics.activeWorks.length;
        for (const work of metrics.activeWorks) {
          const current = workRate(work, now);
          if (current === null) measuring = true;
          else rate += current;
        }
      }
    } else {
      // Older snapshots still have useful item/byte counters, but no speed samples.
      const planned = job.kind === "bulkWorkDownload"
        ? job.progress?.total ?? metadataNumber(job, "reservedCount") ?? metadataNumber(job, "requestedCount") ?? 0 : 1;
      total += planned;
      completed += job.status === "succeeded" ? planned : job.progress?.unit === "items" ? job.progress.current ?? 0 : 0;
      bytes += job.progress?.unit === "bytes" ? job.progress.current ?? 0 : 0;
      if (isActiveJob(job) && job.status !== "queued") { activeWorks++; measuring = true; }
    }
  }
  return { total, completed, failed, activeWorks, bytes, rate: rate > 0 || !measuring ? rate : null, percent: progressPercent(completed, total) };
}
