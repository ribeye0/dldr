use crate::download_progress::DownloadJobProgress;
use crate::{dto::*, state::*};
use dldr_audit::{AuditEvent, AuditLogger};
use dldr_jobs::{JobContext, JobMetadata, JobProgress};
use dldr_library::download_progress::DownloadWorkPlan;
use dldr_library::{
    BulkWorkDownloadPreviewProgress, BulkWorkDownloadPreviewProgressSink, BulkWorkDownloadProgress,
    BulkWorkDownloadProgressSink, BulkWorkDownloadReport, LocalWorkImportReport, SyncProgress,
    SyncProgressSink, WorkDownloadProgress, WorkDownloadProgressSink,
};
use serde_json::{json, Value};
use std::time::Instant;

pub(crate) struct JobSyncProgressSink {
    pub(crate) context: JobContext,
}

impl SyncProgressSink for JobSyncProgressSink {
    fn emit(&self, progress: SyncProgress) {
        match progress {
            SyncProgress::LoggingIn => {
                self.context.set_phase("loggingIn");
                self.context.clear_progress();
                self.context.info("Signing in");
            }
            SyncProgress::LoadingCount => {
                self.context.set_phase("loadingCount");
                self.context.clear_progress();
                self.context.info("Checking library count");
            }
            SyncProgress::LoadingPurchases => {
                self.context.set_phase("loadingPurchases");
                self.context.clear_progress();
                self.context.info("Loading purchases");
            }
            SyncProgress::LoadingWorks { work_count } => {
                self.context.set_phase("loadingWorks");
                self.context
                    .set_progress(JobProgress::items(None, Some(work_count as u64)));
                self.context
                    .info(format!("Loading {work_count} work details"));
            }
            SyncProgress::Committing { work_count } => {
                self.context.set_phase("committing");
                self.context.set_progress(JobProgress::items(
                    Some(work_count as u64),
                    Some(work_count as u64),
                ));
                self.context.info("Saving product cache");
            }
            SyncProgress::Completed {
                sync_run_id,
                cached_work_count,
            } => {
                self.context.set_phase("completed");
                self.context.set_progress(JobProgress::items(
                    Some(cached_work_count as u64),
                    Some(cached_work_count as u64),
                ));
                self.context.info(format!(
                    "Sync run {sync_run_id} cached {cached_work_count} works"
                ));
            }
        }
    }
}

pub(crate) struct JobWorkDownloadProgressSink {
    pub(crate) context: JobContext,
    pub(crate) work_id: String,
    pub(crate) metrics: DownloadJobProgress,
}

impl JobWorkDownloadProgressSink {
    pub(crate) fn new(context: JobContext, plan: DownloadWorkPlan) -> Self {
        Self {
            context,
            work_id: plan.work_id.clone(),
            metrics: DownloadJobProgress::new(vec![plan], false),
        }
    }
}

#[derive(Default)]
pub(crate) struct WorkDownloadProgressThrottle {
    pub(crate) last_download_phase: Option<dldr_download::DownloadPhase>,
    pub(crate) last_download_emit_at: Option<Instant>,
}

impl WorkDownloadProgressThrottle {
    pub(crate) fn emit_decision(&mut self, phase: dldr_download::DownloadPhase) -> Option<bool> {
        self.emit_decision_at(phase, Instant::now())
    }

    pub(crate) fn emit_decision_at(
        &mut self,
        phase: dldr_download::DownloadPhase,
        now: Instant,
    ) -> Option<bool> {
        let phase_changed = self.last_download_phase != Some(phase);

        if phase_changed {
            self.last_download_phase = Some(phase);
            self.last_download_emit_at = Some(now);
            return Some(true);
        }

        if phase != dldr_download::DownloadPhase::Downloading {
            self.last_download_emit_at = Some(now);
            return Some(false);
        }

        let interval_elapsed = match self.last_download_emit_at {
            Some(last_emit) => {
                now.duration_since(last_emit) >= WORK_DOWNLOAD_PROGRESS_EVENT_INTERVAL
            }
            None => true,
        };

        if interval_elapsed {
            self.last_download_emit_at = Some(now);
            return Some(false);
        }

        None
    }

    pub(crate) fn reset(&mut self) {
        self.last_download_phase = None;
        self.last_download_emit_at = None;
    }
}

impl WorkDownloadProgressSink for JobWorkDownloadProgressSink {
    fn emit(&self, progress: WorkDownloadProgress) {
        match &progress {
            WorkDownloadProgress::Warning(message) => self.context.warn(message),
            WorkDownloadProgress::LoggingIn => {
                self.context.set_phase("loggingIn");
                self.context.info("Signing in for download");
            }
            WorkDownloadProgress::ResolvingPlan => {
                self.context.set_phase("resolvingDownload");
                self.context.info("Resolving download files");
            }
            WorkDownloadProgress::Download(p) => match p.phase {
                dldr_download::DownloadPhase::ProbingMetadata => return,
                dldr_download::DownloadPhase::ResolvingPlan => {
                    self.context.set_phase("resolvingDownload")
                }
                dldr_download::DownloadPhase::Downloading => self.context.set_phase("downloading"),
                dldr_download::DownloadPhase::Unpacking => self.context.set_phase("unpacking"),
                dldr_download::DownloadPhase::Finalizing => self.context.set_phase("finalizing"),
            },
            WorkDownloadProgress::Finalizing => {
                self.context.set_phase("finalizing");
                self.context.info("Moving files into the library");
            }
            WorkDownloadProgress::Completed => {
                self.context.set_phase("completed");
                self.context.info("Download completed");
            }
        }
        self.metrics
            .observe(&self.context, &self.work_id, &progress);
    }
}

pub(crate) struct JobBulkWorkDownloadProgressSink {
    pub(crate) context: JobContext,
    pub(crate) metrics: DownloadJobProgress,
}

pub(crate) struct JobBulkWorkDownloadPreviewProgressSink {
    pub(crate) context: JobContext,
}

impl BulkWorkDownloadPreviewProgressSink for JobBulkWorkDownloadPreviewProgressSink {
    fn emit(&self, progress: BulkWorkDownloadPreviewProgress) {
        match progress {
            BulkWorkDownloadPreviewProgress::Selecting => {
                self.context.set_phase("loadingProducts");
                self.context.clear_progress();
                self.context.info("Selecting matching Library results");
            }
            BulkWorkDownloadPreviewProgress::Selected {
                total_count,
                requested_count,
                skipped_downloaded_count,
            } => {
                self.context.set_phase("bulkPlanning");
                self.context
                    .set_progress(JobProgress::items(Some(0), Some(requested_count as u64)));
                self.context.info(format!(
                    "Planning {requested_count} works from {total_count} matching products; skipped {skipped_downloaded_count} already downloaded"
                ));
            }
            BulkWorkDownloadPreviewProgress::WorkStarted { current, total, .. } => {
                self.context.set_phase("bulkPlanning");
                self.context.set_progress(JobProgress::items(
                    Some(current.saturating_sub(1) as u64),
                    Some(total as u64),
                ));
            }
            BulkWorkDownloadPreviewProgress::WorkPlanned { current, total, .. } => {
                self.context
                    .set_progress(JobProgress::items(Some(current as u64), Some(total as u64)));
            }
            BulkWorkDownloadPreviewProgress::WorkFailed {
                work_id,
                current,
                total,
                error_code,
                error_message,
            } => {
                self.context
                    .set_progress(JobProgress::items(Some(current as u64), Some(total as u64)));
                dldr_audit::record_current_failure(
                    &error_code,
                    json!({"phase":"bulkPlanning","workId":work_id,"current":current,"total":total}),
                );
                self.context.warn(format!(
                    "Failed to plan {work_id} ({current}/{total}): {error_code}: {error_message}"
                ));
            }
            BulkWorkDownloadPreviewProgress::Completed {
                planned_count,
                failed_count,
                ..
            } => {
                self.context.set_phase("completed");
                self.context.clear_progress();
                self.context.info(format!(
                    "Bulk download plan completed: {planned_count} planned, {failed_count} failed"
                ));
            }
        }
    }
}

impl BulkWorkDownloadProgressSink for JobBulkWorkDownloadProgressSink {
    fn emit(&self, progress: BulkWorkDownloadProgress) {
        match progress {
            BulkWorkDownloadProgress::Planned { works } => {
                self.metrics.reset(&self.context, works);
            }
            BulkWorkDownloadProgress::WorkProgress { work_id, progress } => {
                if let WorkDownloadProgress::Warning(message) = &progress {
                    self.context.warn(message);
                }
                self.metrics.observe(&self.context, &work_id, &progress);
            }
            BulkWorkDownloadProgress::Selecting => {
                self.context.set_phase("loadingProducts");
                self.context.clear_progress();
                self.context.info("Selecting matching Library results");
            }
            BulkWorkDownloadProgress::Selected {
                total_count,
                requested_count,
                skipped_downloaded_count,
            } => {
                self.context.set_phase("bulkDownloading");
                self.context
                    .set_progress(JobProgress::items(Some(0), Some(requested_count as u64)));
                self.context.info(format!(
                    "Selected {requested_count} works for download from {total_count} matching products; skipped {skipped_downloaded_count} already downloaded"
                ));
            }
            BulkWorkDownloadProgress::WorkStarted {
                work_id,
                current,
                total,
            } => {
                self.context.set_phase("bulkDownloading");
                self.metrics.start(&self.context, &work_id);
                dldr_audit::record_current(AuditEvent::queued("work.download", "").with_details(
                    json!({"workId":work_id,"phase":"downloading","current":current,"total":total}),
                ));
                self.context
                    .info(format!("Starting {work_id} (work {current}/{total})"));
            }
            BulkWorkDownloadProgress::WorkCompleted {
                work_id,
                current,
                total,
            } => {
                self.metrics.finish(&self.context, &work_id, true);
                dldr_audit::record_current(AuditEvent::succeeded("work.download", "").with_details(
                    json!({"workId":work_id,"phase":"completed","current":current,"total":total}),
                ));
                self.context.info(format!("Downloaded {work_id}"));
            }
            BulkWorkDownloadProgress::WorkFailed {
                work_id,
                current,
                total,
                error_code,
                error_message,
                error_details,
            } => {
                if error_code == "work_deferred" {
                    self.metrics.defer(&self.context, &work_id);
                    self.context.info(format!(
                        "Paused or cancelled {work_id} individually ({current}/{total})"
                    ));
                    return;
                }
                dldr_audit::record_current_failure(
                    &error_code,
                    json!({
                        "phase": "bulkDownloading", "workId": work_id,
                        "current": current, "total": total, "errorDetails": error_details,
                    }),
                );
                self.metrics.finish(&self.context, &work_id, false);
                self.context.warn(format!(
                    "Failed to download {work_id} ({current}/{total}): {error_code}: {error_message}"
                ));
            }
            BulkWorkDownloadProgress::Completed { report } => {
                self.context.set_phase("completed");
                self.context.info(format!(
                    "Bulk download completed: {} succeeded, {} failed, {} paused/cancelled individually",
                    report.succeeded_count, report.failed_count, report.deferred_count
                ));
            }
        }
    }
}

pub(crate) fn post_sync_scan_output(
    context: &JobContext,
    result: dldr_library::Result<LocalWorkImportReport>,
) -> Value {
    match result {
        Ok(report) => {
            context.info(format!(
                "Local scan imported {} folders and updated metadata for {} works",
                report.imported_count, report.metadata_updated_count
            ));
            if report.metadata_missing_count > 0 {
                context.warn(format!(
                    "Local scan could not find public metadata for {} works",
                    report.metadata_missing_count
                ));
            }
            if let Some(error) = &report.metadata_error {
                context.warn(format!(
                    "Local scan metadata lookup had a non-fatal error: {error}"
                ));
            }

            for error in &report.recovery_errors {
                context.warn(error);
            }
            let mut details = local_work_import_report_details(&report);
            if let Value::Object(ref mut object) = details {
                object.insert("status".to_owned(), json!("succeeded"));
            }
            details
        }
        Err(error) => {
            dldr_audit::record_current_failure(
                error.failure_code(),
                json!({
                    "phase": "scanningLocalDownloads",
                    "errorDetails": error.support_details(),
                }),
            );
            let message = error.support_message();

            context.warn(format!("Local scan failed after sync: {message}"));
            json!({
                "status": "failed",
                "errorCode": error.failure_code(),
                "errorMessage": message,
                "errorDetails": error.support_details(),
            })
        }
    }
}

pub(crate) fn local_work_import_report_details(report: &LocalWorkImportReport) -> Value {
    json!({
        "scannedDirectories": report.scanned_directories,
        "importedCount": report.imported_count,
        "skippedNoId": report.skipped_no_id,
        "skippedAmbiguous": report.skipped_ambiguous,
        "skippedNonUtf8": report.skipped_non_utf8,
        "skippedExisting": report.skipped_existing,
        "metadataCandidateCount": report.metadata_candidate_count,
        "metadataUpdatedCount": report.metadata_updated_count,
        "metadataMissingCount": report.metadata_missing_count,
        "metadataError": report.metadata_error.as_deref(),
        "recoveryErrors": report.recovery_errors,
    })
}

pub(crate) async fn record_audit(logger: &AuditLogger, event: AuditEvent) {
    logger.record_now(event);
}

pub(crate) fn job_download_reservation_id(event: &dldr_jobs::JobEvent) -> Option<&str> {
    event
        .snapshot
        .metadata
        .get(DOWNLOAD_RESERVATION_METADATA_KEY)?
        .as_str()
}

pub(crate) fn unpack_policy_label(policy: dldr_download::UnpackPolicy) -> &'static str {
    match policy {
        dldr_download::UnpackPolicy::KeepArchives => "keepArchives",
        dldr_download::UnpackPolicy::UnpackWhenRecognized => "unpackWhenRecognized",
    }
}

pub(crate) fn bulk_download_output(
    report: &BulkWorkDownloadReport,
    skipped_queued_count: usize,
) -> JobMetadata {
    let mut output = JobMetadata::new();

    output.insert("deferredCount".into(), json!(report.deferred_count));
    output.insert("totalCount".to_owned(), json!(report.total_count));
    output.insert("requestedCount".to_owned(), json!(report.requested_count));
    output.insert(
        "skippedDownloadedCount".to_owned(),
        json!(report.skipped_downloaded_count),
    );
    output.insert("skippedQueuedCount".to_owned(), json!(skipped_queued_count));
    output.insert("succeededCount".to_owned(), json!(report.succeeded_count));
    output.insert("failedCount".to_owned(), json!(report.failed_count));
    output.insert(
        "succeededWorks".to_owned(),
        json!(report
            .succeeded_works
            .iter()
            .map(|success| {
                json!({
                    "workId": success.work_id.as_str(),
                    "localPath": success.local_path.to_string_lossy().to_string(),
                    "fileCount": success.file_count,
                    "archiveExtracted": success.archive_extracted,
                    "warnings": success.warnings,
                })
            })
            .collect::<Vec<_>>()),
    );
    output.insert(
        "failedWorks".to_owned(),
        json!(report
            .failed_works
            .iter()
            .map(|failure| {
                json!({
                    "workId": failure.work_id.as_str(),
                    "errorCode": failure.error_code.as_str(),
                    "errorMessage": failure.error_message.as_str(),
                })
            })
            .collect::<Vec<_>>()),
    );

    output
}

pub(crate) fn bulk_download_preview_output(preview: &BulkWorkDownloadPreviewDto) -> JobMetadata {
    let mut output = JobMetadata::new();

    output.insert("totalCount".to_owned(), json!(preview.total_count));
    output.insert("requestedCount".to_owned(), json!(preview.requested_count));
    output.insert(
        "skippedDownloadedCount".to_owned(),
        json!(preview.skipped_downloaded_count),
    );
    output.insert(
        "skippedQueuedCount".to_owned(),
        json!(preview.skipped_queued_count),
    );
    output.insert("plannedCount".to_owned(), json!(preview.planned_count));
    output.insert("failedCount".to_owned(), json!(preview.failed_count));
    output.insert(
        "knownExpectedBytes".to_owned(),
        json!(preview.known_expected_bytes),
    );
    output.insert(
        "totalExpectedBytes".to_owned(),
        json!(preview.total_expected_bytes),
    );
    output.insert(
        "unknownSizeCount".to_owned(),
        json!(preview.unknown_size_count),
    );

    output
}

pub(crate) fn bulk_download_noop_output(
    total_count: u64,
    skipped_downloaded_count: usize,
    skipped_queued_count: usize,
) -> JobMetadata {
    let mut output = JobMetadata::new();

    output.insert("totalCount".to_owned(), json!(total_count));
    output.insert("requestedCount".to_owned(), json!(0usize));
    output.insert(
        "skippedDownloadedCount".to_owned(),
        json!(skipped_downloaded_count),
    );
    output.insert("skippedQueuedCount".to_owned(), json!(skipped_queued_count));
    output.insert("succeededCount".to_owned(), json!(0usize));
    output.insert("failedCount".to_owned(), json!(0usize));
    output.insert(
        "succeededWorks".to_owned(),
        json!(Vec::<serde_json::Value>::new()),
    );
    output.insert(
        "failedWorks".to_owned(),
        json!(Vec::<serde_json::Value>::new()),
    );

    output
}
