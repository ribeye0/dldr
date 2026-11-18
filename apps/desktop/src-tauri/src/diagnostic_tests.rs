use crate::download_progress::DownloadJobProgress;
use crate::progress::*;
use dldr_audit::{AuditEvent, AuditLogger};
use dldr_audit::{AuditOutcome, ExportScope};
use dldr_jobs::JobStatus;
use dldr_jobs::{JobFailure, JobId, JobManager, JobMetadata};
use dldr_library::{BulkWorkDownloadProgress, BulkWorkDownloadProgressSink};
use serde_json::json;
use std::io::Read;
use std::{path::Path, time::Duration};

async fn finished_job(manager: &JobManager, id: &JobId) -> dldr_jobs::JobSnapshot {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let job = manager.get_job(id).unwrap();
            if job.status.is_terminal() {
                return job;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("job must finish")
}

fn persisted_and_exported(logger: &AuditLogger, root: &Path) -> Vec<Vec<AuditEvent>> {
    assert!(logger.flush(Duration::from_secs(2)));
    let persisted = std::fs::read_to_string(
        root.join("diagnostics")
            .join(logger.run_id())
            .join("events-0001.jsonl"),
    )
    .unwrap();
    let destination = root.join("support.zip");
    logger.export(&destination, ExportScope::default()).unwrap();
    let mut archive = zip::ZipArchive::new(std::fs::File::open(destination).unwrap()).unwrap();
    let mut exported = String::new();
    archive
        .by_name("events.jsonl")
        .unwrap()
        .read_to_string(&mut exported)
        .unwrap();
    assert_eq!(persisted, exported);
    [persisted, exported]
        .into_iter()
        .map(|data| {
            assert!(!data.contains("PRIVATE-"));
            data.lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect()
        })
        .collect()
}

#[tokio::test]
async fn bulk_failures_keep_item_causes_and_partial_outcome_in_files_and_zip() {
    let root = tempfile::tempdir().unwrap();
    let audit = AuditLogger::new(root.path()).unwrap();
    let jobs = JobManager::new(Default::default()).with_diagnostics(audit.clone());
    let id = jobs.spawn(
        "bulkWorkDownload",
        "test",
        JobMetadata::new(),
        |context| async move {
            let sink = JobBulkWorkDownloadProgressSink {
                context,
                metrics: DownloadJobProgress::new(Vec::new(), true),
            };
            sink.emit(BulkWorkDownloadProgress::WorkCompleted {
                work_id: "PRIVATE-SUCCESS".into(),
                current: 1,
                total: 4,
            });
            let failures = [
                dldr_library::LibraryError::Io(std::io::Error::from_raw_os_error(13)),
                dldr_library::LibraryError::Download(dldr_download::DownloadError::Io(
                    std::io::Error::from_raw_os_error(28),
                )),
                dldr_library::LibraryError::Download(dldr_download::DownloadError::Archive(
                    dldr_archive::ArchiveError::UnsafeArchiveEntry {
                        entry: "PRIVATE-ENTRY".into(),
                    },
                )),
            ];
            for (index, error) in failures.into_iter().enumerate() {
                sink.emit(BulkWorkDownloadProgress::WorkFailed {
                    work_id: format!("PRIVATE-WORK-{index}"),
                    current: index + 2,
                    total: 4,
                    error_code: error.failure_code().into(),
                    error_message: error.support_message(),
                    error_details: error.support_details(),
                });
            }
            Err(JobFailure::with_code("partial_failure", "PRIVATE-MESSAGE")
                .with_detail("bulkDownload", json!({"failedCount":3,"succeededCount":1})))
        },
    );
    assert_eq!(finished_job(&jobs, &id).await.status, JobStatus::Failed);
    for events in persisted_and_exported(&audit, root.path()) {
        let failures: Vec<_> = events.iter().filter(|e| e.stage == "cause").collect();
        assert_eq!(failures.len(), 3);
        assert_eq!(failures[0].details["errorDetails"]["osCode"], 13);
        assert_eq!(failures[1].details["errorDetails"]["osCode"], 28);
        assert_eq!(
            failures[2].details["errorDetails"]["archiveErrorKind"],
            "unsafe_entry"
        );
        let terminal = events.iter().find(|e| e.kind == "finished").unwrap();
        assert_eq!(terminal.outcome, AuditOutcome::Failed);
        assert_eq!(terminal.details["errorCode"], "partial_failure");
        assert_eq!(
            terminal.details["errorDetails"]["bulkDownload"]["succeededCount"],
            1
        );
        let references: std::collections::BTreeSet<_> = failures
            .iter()
            .map(|e| {
                assert_eq!(e.outcome, AuditOutcome::Failed);
                assert_eq!(e.job_id.as_deref(), Some(id.as_str()));
                assert_eq!(e.operation_id, terminal.operation_id);
                assert_eq!(e.details["phase"], "bulkDownloading");
                let reference = e.details["workId"].as_str().unwrap();
                assert!(reference.starts_with("ref-"));
                reference
            })
            .collect();
        assert_eq!(references.len(), 3);
    }
}

#[tokio::test]
async fn scan_failure_survives_successful_sync_in_files_and_zip() {
    let root = tempfile::tempdir().unwrap();
    let audit = AuditLogger::new(root.path()).unwrap();
    let jobs = JobManager::new(Default::default()).with_diagnostics(audit.clone());
    let id = jobs.spawn(
        "accountSync",
        "test",
        JobMetadata::new(),
        |context| async move {
            let result = post_sync_scan_output(
                &context,
                Err(dldr_library::LibraryError::Io(
                    std::io::Error::from_raw_os_error(13),
                )),
            );
            assert_eq!(result["status"], "failed");
            Ok(JobMetadata::from([
                ("cachedWorkCount".into(), json!(12)),
                ("localScan".into(), result),
            ]))
        },
    );
    let job = finished_job(&jobs, &id).await;
    assert_eq!(job.status, JobStatus::Succeeded);
    assert_eq!(job.output.unwrap()["localScan"]["status"], "failed");
    for events in persisted_and_exported(&audit, root.path()) {
        let failures: Vec<_> = events
            .iter()
            .filter(|e| e.outcome == AuditOutcome::Failed)
            .collect();
        assert_eq!(failures.len(), 1);
        let failure = failures[0];
        assert_eq!(failure.details["phase"], "scanningLocalDownloads");
        assert_eq!(failure.error_code.as_deref(), Some("io"));
        assert_eq!(failure.details["errorDetails"]["osCode"], 13);
        assert_eq!(failure.job_id.as_deref(), Some(id.as_str()));
        let terminals: Vec<_> = events.iter().filter(|e| e.kind == "finished").collect();
        assert_eq!(terminals.len(), 1);
        assert_eq!(terminals[0].outcome, AuditOutcome::Succeeded);
        assert_eq!(terminals[0].details["output"]["cachedWorkCount"], 12);
        assert_eq!(failure.operation_id, terminals[0].operation_id);
    }
}
