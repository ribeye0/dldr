import { translator, type UiLocale } from "$lib/i18n";
import type {
  AuditEvent,
  AuditOutcome,
  BulkFailedWork,
  BulkSucceededWork,
  JobSnapshot,
} from "$lib/model/types";
import { formatBytes, shortDate } from "$lib/utils/format";

export function upsertJob(currentJobs: JobSnapshot[], job: JobSnapshot) {
  const index = currentJobs.findIndex((item) => item.id === job.id);

  if (index === -1) {
    return [...currentJobs, job];
  }

  const next = currentJobs.slice();
  next[index] = job;
  return next;
}

export function isDownloadQueueJob(job: JobSnapshot) {
  return job.kind === "workDownload" || job.kind === "bulkWorkDownload";
}

export function auditOutcomeLabel(outcome: AuditOutcome, language: UiLocale = "en") {
  const translate = translator(language);
  switch (outcome) {
    case "unknown":
      return translate("Not confirmed");
    case "queued":
      return translate("Queued");
    case "succeeded":
      return translate("Succeeded");
    case "failed":
      return translate("Failed");
    case "cancelled":
      return translate("Cancelled");
  }
}

export function auditDetail(event: AuditEvent) {
  return event.errorMessage ?? event.message;
}

export function isActiveJob(job: JobSnapshot) {
  return job.status === "queued" || job.status === "running" || job.status === "cancelling";
}

export function isTerminalJob(job: JobSnapshot) {
  return job.status === "succeeded" || job.status === "failed" || job.status === "cancelled";
}

export function jobAccountId(job: JobSnapshot) {
  const accountId = job.metadata.accountId;
  return typeof accountId === "string" ? accountId : null;
}

export function jobWorkId(job: JobSnapshot) {
  const workId = job.metadata.workId;
  return typeof workId === "string" ? workId : null;
}

export function jobLabel(job: JobSnapshot, language: UiLocale = "en") {
  const translate = translator(language);
  if (job.status === "queued") {
    return translate("Queued");
  }

  if (job.status === "cancelling") {
    return translate("Cancelling");
  }

  if (job.status === "failed") {
    return translate("Failed");
  }

  if (job.status === "cancelled") {
    return translate("Cancelled");
  }

  if (job.status === "succeeded") {
    if (job.kind === "workDownload") {
      return translate("Downloaded");
    }

    if (job.kind === "bulkWorkDownload") {
      const succeededCount = jobOutputNumber(job, "succeededCount");
      return typeof succeededCount === "number"
        ? translate("Downloaded {0} works", succeededCount)
        : translate("Downloaded");
    }

    if (job.kind === "bulkWorkDownloadPreview") {
      const plannedCount = jobOutputNumber(job, "plannedCount");
      const failedCount = jobOutputNumber(job, "failedCount");

      if (
        typeof plannedCount === "number" &&
        typeof failedCount === "number" &&
        failedCount > 0
      ) {
        return translate("Planned {0}, {1} failed", plannedCount, failedCount);
      }

      return typeof plannedCount === "number" ? translate("Planned {0} works", plannedCount) : translate("Planned");
    }

    const cachedCount = jobOutputNumber(job, "cachedWorkCount");
    return typeof cachedCount === "number" ? translate("Synced {0} works", cachedCount) : translate("Synced");
  }

  switch (job.phase) {
    case "loggingIn":
      return translate("Signing in");
    case "loadingCount":
      return translate("Checking library");
    case "loadingPurchases":
      return translate("Loading purchases");
    case "loadingWorks":
      return translate("Loading {0} works", job.progress?.total ?? 0);
    case "loadingProducts":
      return translate("Preparing downloads");
    case "bulkDownloading":
      return bulkDownloadJobProgressLabel(job, language);
    case "bulkPlanning":
      return bulkDownloadPlanningJobProgressLabel(job, language);
    case "committing":
      return translate("Saving cache");
    case "completed":
      return translate("Completing");
    case "resolvingDownload":
      return translate("Resolving download");
    case "probingDownload":
      return job.kind === "workDownload" ? downloadJobProgressLabel(job, language) : translate("Resolving download");
    case "downloading":
      return downloadJobProgressLabel(job, language);
    case "unpacking":
      return translate("Decompressing");
    case "finalizing":
      return translate("Finalizing");
    default:
      if (job.kind === "bulkWorkDownload") {
        return translate("Downloading results");
      }

      if (job.kind === "bulkWorkDownloadPreview") {
        return translate("Planning");
      }

      return job.kind === "workDownload" ? translate("Downloading") : translate("Syncing");
  }
}

export function activeJobDetail(job: JobSnapshot, language: UiLocale = "en") {
  if (job.kind === "workDownload") {
    return activeWorkDownloadDetail(job, language);
  }

  if (job.kind === "bulkWorkDownload") {
    return activeBulkDownloadDetail(job, language);
  }

  return null;
}

export function activeWorkDownloadDetail(job: JobSnapshot, language: UiLocale = "en") {
  const translate = translator(language);
  if (job.status === "queued") {
    return translate("Waiting to start");
  }

  switch (job.phase) {
    case "loggingIn":
      return translate("Signing in");
    case "resolvingDownload":
      return translate("Resolving download files");
    case "probingDownload":
    case "downloading":
      return downloadProgressDetail(job, language);
    case "unpacking":
      return translate("Decompressing archive");
    case "finalizing":
      return translate("Moving files into the library");
    case "completed":
      return translate("Completing");
    default:
      return translate("Preparing download");
  }
}

export function activeBulkDownloadDetail(job: JobSnapshot, language: UiLocale = "en") {
  const translate = translator(language);
  if (job.status === "queued") {
    return translate("Waiting to start");
  }

  if (job.phase === "bulkDownloading" && job.progress?.unit === "items") {
    const current = job.progress.current ?? 0;
    const total = job.progress.total;

    return typeof total === "number" && total > 0
      ? translate("{0} of {1} products processed", current, total)
      : translate("Processing products");
  }

  return translate("Preparing bulk download");
}

export function downloadQueueSubtitle(job: JobSnapshot, language: UiLocale = "en") {
  const translate = translator(language);
  if (job.kind === "workDownload") {
    const workId = jobWorkId(job) ?? jobOutputString(job, "workId");
    return workId ? translate("Single work {0}", workId) : translate("Single work download");
  }

  const skippedDownloaded =
    jobOutputNumber(job, "skippedDownloadedCount") ?? metadataNumber(job, "skippedDownloadedCount");
  const skippedQueued =
    jobOutputNumber(job, "skippedQueuedCount") ?? metadataNumber(job, "skippedQueuedCount");
  const parts = [
    typeof skippedDownloaded === "number" ? translate("{0} already downloaded", skippedDownloaded) : null,
    typeof skippedQueued === "number" ? translate("{0} already queued", skippedQueued) : null,
  ].filter((part): part is string => part !== null);

  return parts.length > 0 ? parts.join(", ") : translate("Current Library filters");
}

export function downloadQueueKindLabel(job: JobSnapshot, language: UiLocale = "en") {
  const translate = translator(language);
  if (job.kind === "bulkWorkDownload") {
    return translate("Bulk");
  }

  return translate("Work");
}

export function downloadQueueTime(job: JobSnapshot) {
  return shortDate(job.finishedAt ?? job.startedAt ?? job.createdAt);
}

export function downloadQueueProgressPercent(job: JobSnapshot) {
  const current = job.progress?.current;
  const total = job.progress?.total;

  if (typeof current !== "number" || typeof total !== "number" || total <= 0) {
    return null;
  }

  return Math.min(100, Math.max(0, Math.floor((current * 100) / total)));
}

export function metadataNumber(job: JobSnapshot, key: string) {
  const value = job.metadata[key];
  return typeof value === "number" ? value : null;
}

export function jobOutputString(job: JobSnapshot, key: string) {
  const value = job.output?.[key];
  return typeof value === "string" ? value : null;
}

export function jobOutputBoolean(job: JobSnapshot, key: string) {
  const value = job.output?.[key];
  return typeof value === "boolean" ? value : false;
}

export function jobOutputNumber(job: JobSnapshot, key: string) {
  const value = job.output?.[key];
  return typeof value === "number" ? value : null;
}

export function bulkDownloadResult(job: JobSnapshot) {
  const source = job.output ?? recordValue(job.error?.details.bulkDownload);

  return {
    succeededWorks: parseBulkSucceededWorks(source?.succeededWorks),
    failedWorks: parseBulkFailedWorks(source?.failedWorks),
  };
}

export function parseBulkSucceededWorks(value: unknown): BulkSucceededWork[] {
  if (!Array.isArray(value)) {
    return [];
  }

  return value
    .map((item) => {
      const record = recordValue(item);

      if (!record || typeof record.workId !== "string") {
        return null;
      }

      return {
        workId: record.workId,
        localPath: typeof record.localPath === "string" ? record.localPath : null,
        fileCount: typeof record.fileCount === "number" ? record.fileCount : null,
        archiveExtracted:
          typeof record.archiveExtracted === "boolean" ? record.archiveExtracted : null,
      };
    })
    .filter((item): item is BulkSucceededWork => item !== null);
}

export function parseBulkFailedWorks(value: unknown): BulkFailedWork[] {
  if (!Array.isArray(value)) {
    return [];
  }

  return value
    .map((item) => {
      const record = recordValue(item);

      if (!record || typeof record.workId !== "string") {
        return null;
      }

      return {
        workId: record.workId,
        errorCode: typeof record.errorCode === "string" ? record.errorCode : null,
        errorMessage: typeof record.errorMessage === "string" ? record.errorMessage : null,
      };
    })
    .filter((item): item is BulkFailedWork => item !== null);
}

export function recordValue(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

export function downloadJobProgressLabel(job: JobSnapshot, language: UiLocale = "en") {
  const translate = translator(language);
  if (job.progress?.unit !== "bytes") {
    return translate("Downloading");
  }

  const current = job.progress.current ?? 0;
  const total = job.progress.total;

  if (typeof total === "number" && total > 0) {
    return translate("Downloading {0}%", Math.min(100, Math.floor((current * 100) / total)));
  }

  return translate("Downloading");
}

export function downloadProgressDetail(job: JobSnapshot, language: UiLocale = "en") {
  const translate = translator(language);
  if (job.progress?.unit !== "bytes") {
    return translate("Downloading files");
  }

  const current = job.progress.current ?? 0;
  const total = job.progress.total;

  if (typeof total === "number" && total > 0) {
    return translate("{0} of {1}", formatBytes(current), formatBytes(total));
  }

  return translate("{0} downloaded", formatBytes(current));
}

export function bulkDownloadJobProgressLabel(job: JobSnapshot, language: UiLocale = "en") {
  const translate = translator(language);
  if (job.progress?.unit !== "items") {
    return translate("Downloading results");
  }

  const current = job.progress.current ?? 0;
  const total = job.progress.total;

  if (typeof total === "number" && total > 0) {
    return translate("Downloading {0}/{1}", current, total);
  }

  return translate("Downloading results");
}

export function bulkDownloadPlanningJobProgressLabel(job: JobSnapshot, language: UiLocale = "en") {
  const translate = translator(language);
  if (job.progress?.unit !== "items") {
    return translate("Planning");
  }

  const current = job.progress.current ?? 0;
  const total = job.progress.total;

  if (typeof total === "number" && total > 0) {
    return translate("Planning {0}/{1}", current, total);
  }

  return translate("Planning");
}
