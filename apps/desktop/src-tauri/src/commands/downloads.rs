use super::library_tools::queue_entry;
use crate::download_progress::DownloadJobProgress;
use crate::two_factor::JobTwoFactorPrompt;
use crate::{dto::*, errors::*, progress::*, state::*, validation::*};
use dldr_audit::AuditEvent;
use dldr_jobs::{JobFailure, JobMetadata};
use dldr_library::download_progress::DownloadWorkPlan;
use dldr_library::{
    BulkWorkDownloadPreviewRequest, BulkWorkDownloadRequest, DlsitePublicMetadataSource,
    DlsiteSource, LocalWorkImportRequest, WorkDownloadMarkRequest, WorkDownloadRemovalRequest,
    WorkDownloadRequest,
};
use dldr_storage::WorkDownloadStatus;
use serde_json::{json, Value};
use std::path::PathBuf;
use tauri::{AppHandle, State};

#[tauri::command]
pub(crate) async fn start_work_download(
    app: AppHandle,
    state: State<'_, AppState>,
    request: StartWorkDownloadRequest,
) -> Result<StartJobResponse, String> {
    let _admission = state.mutation_lock.read().await;
    let _queue = state.queue_lock.lock().await;
    start_work_download_inner(app, &state, request, None).await
}

pub(crate) async fn start_work_download_inner(
    app: AppHandle,
    state: &AppState,
    request: StartWorkDownloadRequest,
    restored: Option<dldr_storage::DownloadQueueEntry>,
) -> Result<StartJobResponse, String> {
    let diagnostic_operation = state.audit.operation("start_work_download");
    diagnostic_operation
        .command(async {
            let work_id = match normalize_required_id(request.work_id) {
                Ok(work_id) => work_id,
                Err(error) => {
                    record_audit(
                        &state.audit,
                        AuditEvent::failed("work.download.queue", "Failed to validate download")
                            .with_error(Some("validation"), error.clone()),
                    )
                    .await;
                    return Err(error);
                }
            };
            let account_id = match normalize_optional_id(request.account_id) {
                Ok(account_id) => account_id,
                Err(error) => {
                    record_audit(
                        &state.audit,
                        AuditEvent::failed(
                            "work.download.queue",
                            "Failed to validate download account",
                        )
                        .with_error(Some("validation"), error.clone())
                        .with_details(json!({ "workId": work_id })),
                    )
                    .await;
                    return Err(error);
                }
            };
            let password = match normalize_secret(request.password) {
                Ok(password) => password,
                Err(error) => {
                    record_audit(
                        &state.audit,
                        AuditEvent::failed(
                            "work.download.queue",
                            "Failed to validate download secret",
                        )
                        .with_error(Some("validation"), error.clone())
                        .with_details(json!({ "workId": work_id })),
                    )
                    .await;
                    return Err(error);
                }
            };
            let settings = match state.storage.app_settings().await {
                Ok(settings) => settings,
                Err(error) => {
                    let message = command_error(error);
                    record_audit(
                        &state.audit,
                        AuditEvent::failed(
                            "work.download.queue",
                            "Failed to load settings for download",
                        )
                        .with_error(Some("storage"), message.clone())
                        .with_details(json!({ "workId": work_id })),
                    )
                    .await;
                    return Err(message);
                }
            };
            let library_root = match restored
                .as_ref()
                .map(|e| Ok(PathBuf::from(&e.library_root)))
                .unwrap_or_else(|| required_library_root(&settings))
            {
                Ok(root) => root,
                Err(error) => {
                    record_audit(
                        &state.audit,
                        AuditEvent::failed(
                            "work.download.queue",
                            "Failed to resolve library folder",
                        )
                        .with_error(Some("settings"), error.clone())
                        .with_details(json!({ "workId": work_id })),
                    )
                    .await;
                    return Err(error);
                }
            };
            let download_root = match restored
                .as_ref()
                .map(|e| Ok(PathBuf::from(&e.download_root)))
                .unwrap_or_else(|| effective_download_root(&app, &settings))
            {
                Ok(root) => root,
                Err(error) => {
                    record_audit(
                        &state.audit,
                        AuditEvent::failed(
                            "work.download.queue",
                            "Failed to resolve download staging folder",
                        )
                        .with_error(Some("settings"), error.clone())
                        .with_details(json!({ "workId": work_id })),
                    )
                    .await;
                    return Err(error);
                }
            };
            let resume_partial = restored.is_some();
            let unpack_policy = request
                .unpack_policy
                .unwrap_or(if settings.auto_extract {
                    UnpackPolicyDto::UnpackWhenRecognized
                } else {
                    UnpackPolicyDto::KeepArchives
                })
                .into();
            let replace_existing = request.replace_existing.unwrap_or(false);
            let library = state.library.clone();
            let mut metadata = JobMetadata::new();

            metadata.insert("workId".to_owned(), json!(work_id.clone()));
            if let Some(account_id) = &account_id {
                metadata.insert("accountId".to_owned(), json!(account_id));
            }

            let reservation_id = state.download_reservations.next_owner_id();
            let reservation = state
                .download_reservations
                .claim_available(std::slice::from_ref(&work_id), &reservation_id);

            metadata.insert(
                "skippedQueuedCount".to_owned(),
                json!(reservation.reserved.len()),
            );

            if reservation.available.is_empty() {
                metadata.insert("skippedQueued".to_owned(), json!(true));

                let job_work_id = work_id.clone();
                let job_id = state.jobs.spawn(
                    "workDownload",
                    format!("Download {job_work_id}"),
                    metadata,
                    move |context| async move {
                        context.info(
                            "Download is already queued or running; skipping duplicate request",
                        );
                        let mut output = JobMetadata::new();

                        output.insert("workId".to_owned(), json!(job_work_id));
                        output.insert("skippedQueued".to_owned(), json!(true));
                        output.insert("skippedQueuedCount".to_owned(), json!(1usize));

                        Ok(output)
                    },
                );

                record_audit(
                    &state.audit,
                    AuditEvent::queued("work.download", "Queued duplicate work download no-op")
                        .with_details(json!({
                            "workId": work_id,
                            "accountId": account_id,
                            "jobId": job_id.to_string(),
                            "skippedQueuedCount": 1,
                            "replaceExisting": replace_existing,
                            "unpackPolicy": unpack_policy_label(unpack_policy),
                        })),
                )
                .await;

                return Ok(StartJobResponse {
                    job_id: job_id.to_string(),
                });
            }

            let queue_batch = reservation_id.clone();
            let entry = queue_entry(
                &work_id,
                &queue_batch,
                account_id.clone(),
                &library_root,
                &download_root,
                unpack_policy,
                replace_existing,
            );
            if let Err(error) = state.storage.enqueue_downloads(&[entry]).await {
                state.download_reservations.release_owner(&reservation_id);
                return Err(command_error(error));
            }
            metadata.insert(
                DOWNLOAD_RESERVATION_METADATA_KEY.to_owned(),
                json!(reservation_id),
            );
            metadata.insert("reservedWorkIds".to_owned(), json!(reservation.available));

            let job_work_id = work_id.clone();
            let detail = library.product_detail(&job_work_id).await.ok();
            let progress_plan = DownloadWorkPlan {
                work_id: job_work_id.clone(),
                title: detail
                    .as_ref()
                    .map(|d| d.title.clone())
                    .unwrap_or_else(|| job_work_id.clone()),
                bytes_total: detail.and_then(|d| d.content_size_bytes),
            };
            let audit_account_id = account_id.clone();
            let rate_limiter = state.download_rate_limiter.clone();
            let two_factor_prompts = state.two_factor_prompts.clone();
            let two_factor_app = app.clone();
            let job_id = state.jobs.spawn(
                "workDownload",
                format!("Download {job_work_id}"),
                metadata,
                move |context| async move {
                    context.info("Preparing download");
                    let client =
                        dldr_api::DlsiteClient::new(dldr_api::DlsiteClientConfig::default())
                            .map_err(|error| {
                                JobFailure::with_code("api_client", error.to_string())
                            })?;
                    let source = DlsiteSource::new(client).with_rate_limiter(rate_limiter);
                    let progress_sink =
                        JobWorkDownloadProgressSink::new(context.clone(), progress_plan);
                    let two_factor_prompt = JobTwoFactorPrompt::new(
                        two_factor_app,
                        two_factor_prompts,
                        context.clone(),
                    );
                    let report = library
                        .download_work_with_source(
                            WorkDownloadRequest {
                                resume_partial,
                                queue_batch: Some(&queue_batch),
                                work_id: &job_work_id,
                                account_id: account_id.as_deref(),
                                password: password.as_deref(),
                                library_root: &library_root,
                                download_root: &download_root,
                                unpack_policy,
                                replace_existing,
                                cancellation_token: Some(context.cancellation_token()),
                                progress_sink: Some(&progress_sink),
                                two_factor_prompt: Some(&two_factor_prompt),
                            },
                            &source,
                        )
                        .await
                        .map_err(work_download_failure)?;
                    let mut output = JobMetadata::new();

                    output.insert("workId".to_owned(), json!(report.work_id));
                    output.insert("accountId".to_owned(), json!(report.account_id));
                    output.insert(
                        "localPath".to_owned(),
                        json!(report.local_path.to_string_lossy().to_string()),
                    );
                    output.insert("fileCount".to_owned(), json!(report.file_count));
                    output.insert("warnings".to_owned(), json!(report.warnings));
                    output.insert(
                        "archiveExtracted".to_owned(),
                        json!(report.archive_extracted),
                    );
                    context.info(format!("Downloaded {}", job_work_id));

                    Ok(output)
                },
            );

            record_audit(
                &state.audit,
                AuditEvent::queued("work.download", "Queued work download").with_details(json!({
                    "workId": work_id,
                    "accountId": audit_account_id,
                    "jobId": job_id.to_string(),
                    "replaceExisting": replace_existing,
                    "unpackPolicy": unpack_policy_label(unpack_policy),
                })),
            )
            .await;

            Ok(StartJobResponse {
                job_id: job_id.to_string(),
            })
        })
        .await
}

#[tauri::command]
pub(crate) async fn start_bulk_work_download(
    app: AppHandle,
    state: State<'_, AppState>,
    request: BulkWorkDownloadCommandRequest,
) -> Result<StartJobResponse, String> {
    let _admission = state.mutation_lock.read().await;
    let _queue = state.queue_lock.lock().await;
    let max_concurrent_works = request.concurrency()?;
    let diagnostic_operation = state.audit.operation("start_bulk_work_download");
    diagnostic_operation.command(async {
    let query = match request.to_query() {
        Ok(query) => query,
        Err(error) => {
            record_audit(
                &state.audit,
                AuditEvent::failed(
                    "work.bulkDownload.queue",
                    "Failed to validate bulk download",
                )
                .with_error(Some("validation"), error.clone()),
            )
            .await;
            return Err(error);
        }
    };
    let settings = match state.storage.app_settings().await {
        Ok(settings) => settings,
        Err(error) => {
            let message = command_error(error);
            record_audit(
                &state.audit,
                AuditEvent::failed(
                    "work.bulkDownload.queue",
                    "Failed to load settings for bulk download",
                )
                .with_error(Some("storage"), message.clone()),
            )
            .await;
            return Err(message);
        }
    };
    let library_root = match required_library_root(&settings) {
        Ok(root) => root,
        Err(error) => {
            record_audit(
                &state.audit,
                AuditEvent::failed(
                    "work.bulkDownload.queue",
                    "Failed to resolve library folder",
                )
                .with_error(Some("settings"), error.clone()),
            )
            .await;
            return Err(error);
        }
    };
    let download_root = match effective_download_root(&app, &settings) {
        Ok(root) => root,
        Err(error) => {
            record_audit(
                &state.audit,
                AuditEvent::failed(
                    "work.bulkDownload.queue",
                    "Failed to resolve download staging folder",
                )
                .with_error(Some("settings"), error.clone()),
            )
            .await;
            return Err(error);
        }
    };
    let unpack_policy = request.unpack_policy.unwrap_or(if settings.auto_extract {
        UnpackPolicyDto::UnpackWhenRecognized
    } else {
        UnpackPolicyDto::KeepArchives
    }).into();
    let skip_downloaded = request.skip_downloaded.unwrap_or(true);
    let work_ids = request.selected_work_ids()?;
    let candidates = match state.library.select_bulk_candidates(&query, skip_downloaded, work_ids.as_deref()).await {
        Ok(candidates) => candidates,
        Err(error) => {
            let message = command_error(error);
            record_audit(
                &state.audit,
                AuditEvent::failed(
                    "work.bulkDownload.queue",
                    "Failed to select bulk download products",
                )
                .with_error(Some("library"), message.clone()),
            )
            .await;
            return Err(message);
        }
    };
    let reservation_id = state.download_reservations.next_owner_id();
    let reservation = state
        .download_reservations
        .claim_available(&candidates.work_ids, &reservation_id);
    let skipped_queued_count = reservation.reserved.len();
    let reserved_work_ids = reservation.available.clone();
    let library = state.library.clone();
    let mut metadata = JobMetadata::new();

    metadata.insert("maxConcurrentWorks".to_owned(), json!(max_concurrent_works));
    metadata.insert("search".to_owned(), json!(query.search.clone()));
    metadata.insert("accountId".to_owned(), json!(query.account_id.clone()));
    metadata.insert("skipDownloaded".to_owned(), json!(skip_downloaded));
    metadata.insert(
        "candidateCount".to_owned(),
        json!(candidates.work_ids.len()),
    );
    metadata.insert(
        "skippedDownloadedCount".to_owned(),
        json!(candidates.skipped_downloaded_count),
    );
    metadata.insert("skippedQueuedCount".to_owned(), json!(skipped_queued_count));
    metadata.insert("reservedCount".to_owned(), json!(reserved_work_ids.len()));
    metadata.insert(
        "unpackPolicy".to_owned(),
        json!(unpack_policy_label(unpack_policy)),
    );

    if !reserved_work_ids.is_empty() {
        metadata.insert(
            DOWNLOAD_RESERVATION_METADATA_KEY.to_owned(),
            json!(reservation_id.clone()),
        );
        metadata.insert(
            "reservedWorkIds".to_owned(),
            json!(reserved_work_ids.clone()),
        );
    }

    if reserved_work_ids.is_empty() {
        let total_count = candidates.total_count;
        let skipped_downloaded_count = candidates.skipped_downloaded_count;
        let audit_metadata = metadata.clone();
        let job_id = state.jobs.spawn(
            "bulkWorkDownload",
            "Download Library results",
            metadata,
            move |context| async move {
                context.info("No matching products are available for download");
                let output = bulk_download_noop_output(
                    total_count,
                    skipped_downloaded_count,
                    skipped_queued_count,
                );

                Ok(output)
            },
        );

        record_audit(
            &state.audit,
            AuditEvent::queued("work.bulkDownload", "Queued bulk work download no-op")
                .with_details(json!({
                    "jobId": job_id.to_string(),
                    "metadata": audit_metadata,
                })),
        )
        .await;

        return Ok(StartJobResponse {
            job_id: job_id.to_string(),
        });
    }

    let queue_batch = reservation_id.clone();
    let entries = reserved_work_ids.iter().map(|id| queue_entry(id, &queue_batch, query.account_id.clone(), &library_root, &download_root, unpack_policy, false)).collect::<Vec<_>>();
    if let Err(error) = state.storage.enqueue_downloads(&entries).await {
        state.download_reservations.release_owner(&reservation_id);
        return Err(command_error(error));
    }
    let audit_metadata = metadata.clone();
    let two_factor_prompts = state.two_factor_prompts.clone();
    let two_factor_app = app.clone();
    let rate_limiter = state.download_rate_limiter.clone();
    let job_id = state.jobs.spawn(
        "bulkWorkDownload",
        "Download Library results",
        metadata,
        move |context| async move {
            context.info("Preparing bulk download");
            if skipped_queued_count > 0 {
                context.info(format!(
                    "Skipping {skipped_queued_count} works already queued or downloading"
                ));
            }
            let progress_sink = JobBulkWorkDownloadProgressSink {
                context: context.clone(),
                metrics: DownloadJobProgress::new(Vec::new(), true),
            };
            let two_factor_prompt =
                JobTwoFactorPrompt::new(two_factor_app, two_factor_prompts, context.clone()).for_bulk_download();
            let mut report = library
                .download_products_with_source_factory(
                    BulkWorkDownloadRequest {
                        queue_batch: Some(&queue_batch),
                        query,
                        work_ids: Some(reserved_work_ids),
                        library_root: &library_root,
                        download_root: &download_root,
                        unpack_policy,
                        skip_downloaded,
                        cancellation_token: Some(context.cancellation_token()),
                        progress_sink: Some(&progress_sink),
                        two_factor_prompt: Some(&two_factor_prompt),
                    },
                    max_concurrent_works,
                    || dldr_api::DlsiteClient::new(dldr_api::DlsiteClientConfig::default())
                        .map(|client| DlsiteSource::new(client).with_rate_limiter(rate_limiter.clone())).map_err(Into::into),
                )
                .await
                .map_err(|error| {
                    let mut failure = work_download_failure(error);
                    if let Some(Value::Object(output)) = failure.details.get_mut("bulkDownload") {
                        output.insert("skippedQueuedCount".to_owned(), json!(skipped_queued_count));
                    }
                    failure
                })?;
            report.total_count = candidates.total_count;
            report.skipped_downloaded_count += candidates.skipped_downloaded_count;
            for work in &report.succeeded_works {
                for warning in &work.warnings { context.warn(warning); }
            }
            let output = bulk_download_output(&report, skipped_queued_count);

            context.info(format!(
                "Bulk download finished: {} downloaded, {} failed, {} downloaded skips, {} queued skips",
                report.succeeded_count,
                report.failed_count,
                report.skipped_downloaded_count,
                skipped_queued_count
            ));

            if report.failed_count > 0 {
                return Err(JobFailure::with_code(
                    "partial_failure",
                    format!(
                        "Downloaded {} works, failed {} works",
                        report.succeeded_count, report.failed_count
                    ),
                )
                .with_detail("bulkDownload", json!(output.clone())));
            }

            Ok(output)
        },
    );

    record_audit(
        &state.audit,
        AuditEvent::queued("work.bulkDownload", "Queued bulk work download").with_details(json!({
            "jobId": job_id.to_string(),
            "metadata": audit_metadata,
        })),
    )
    .await;

    Ok(StartJobResponse {
        job_id: job_id.to_string(),
    })
    }).await
}

#[tauri::command]
pub(crate) async fn preview_bulk_work_download(
    app: AppHandle,
    state: State<'_, AppState>,
    request: BulkWorkDownloadCommandRequest,
) -> Result<BulkWorkDownloadPreviewDto, String> {
    let diagnostic_operation = state.audit.operation("preview_bulk_work_download");
    diagnostic_operation
        .command(async {
            let query = match request.to_query() {
                Ok(query) => query,
                Err(error) => {
                    record_audit(
                        &state.audit,
                        AuditEvent::failed(
                            "work.bulkDownload.preview",
                            "Failed to validate bulk download preview",
                        )
                        .with_error(Some("validation"), error.clone()),
                    )
                    .await;
                    return Err(error);
                }
            };
            let settings = match state.storage.app_settings().await {
                Ok(settings) => settings,
                Err(error) => {
                    let message = command_error(error);
                    record_audit(
                        &state.audit,
                        AuditEvent::failed(
                            "work.bulkDownload.preview",
                            "Failed to load settings for bulk download preview",
                        )
                        .with_error(Some("storage"), message.clone()),
                    )
                    .await;
                    return Err(message);
                }
            };

            if let Err(error) = required_library_root(&settings) {
                record_audit(
                    &state.audit,
                    AuditEvent::failed(
                        "work.bulkDownload.preview",
                        "Failed to resolve library folder",
                    )
                    .with_error(Some("settings"), error.clone()),
                )
                .await;
                return Err(error);
            }

            if let Err(error) = effective_download_root(&app, &settings) {
                record_audit(
                    &state.audit,
                    AuditEvent::failed(
                        "work.bulkDownload.preview",
                        "Failed to resolve download staging folder",
                    )
                    .with_error(Some("settings"), error.clone()),
                )
                .await;
                return Err(error);
            }

            let skip_downloaded = request.skip_downloaded.unwrap_or(true);
            let work_ids = request.selected_work_ids()?;
            let candidates = match state
                .library
                .select_bulk_candidates(&query, skip_downloaded, work_ids.as_deref())
                .await
            {
                Ok(candidates) => candidates,
                Err(error) => {
                    let message = command_error(error);
                    record_audit(
                        &state.audit,
                        AuditEvent::failed(
                            "work.bulkDownload.preview",
                            "Failed to select bulk download products",
                        )
                        .with_error(Some("library"), message.clone()),
                    )
                    .await;
                    return Err(message);
                }
            };
            let reservation = state
                .download_reservations
                .split_available(&candidates.work_ids);
            let skipped_queued_count = reservation.reserved.len();
            let available_work_ids = reservation.available;
            let library = state.library.clone();
            let (result_tx, result_rx) =
                tokio::sync::oneshot::channel::<Result<BulkWorkDownloadPreviewDto, String>>();
            let mut metadata = JobMetadata::new();

            metadata.insert("search".to_owned(), json!(query.search.clone()));
            metadata.insert("accountId".to_owned(), json!(query.account_id.clone()));
            metadata.insert("skipDownloaded".to_owned(), json!(skip_downloaded));
            metadata.insert(
                "candidateCount".to_owned(),
                json!(candidates.work_ids.len()),
            );
            metadata.insert(
                "skippedDownloadedCount".to_owned(),
                json!(candidates.skipped_downloaded_count),
            );
            metadata.insert("skippedQueuedCount".to_owned(), json!(skipped_queued_count));
            metadata.insert(
                "plannedCandidateCount".to_owned(),
                json!(available_work_ids.len()),
            );

            let audit_metadata = metadata.clone();
            let job_id = state.jobs.spawn(
                "bulkWorkDownloadPreview",
                "Plan Bulk Download",
                metadata,
                move |context| async move {
                    context.info("Preparing bulk download plan");
                    let client = match dldr_api::DlsiteClient::new(
                        dldr_api::DlsiteClientConfig::default(),
                    ) {
                        Ok(client) => client,
                        Err(error) => {
                            let message = error.to_string();
                            let _ = result_tx.send(Err(message.clone()));
                            return Err(JobFailure::with_code("api_client", message));
                        }
                    };
                    let source = DlsiteSource::new(client);
                    let progress_sink = JobBulkWorkDownloadPreviewProgressSink {
                        context: context.clone(),
                    };
                    let result = library
                        .preview_download_products_with_source(
                            BulkWorkDownloadPreviewRequest {
                                query,
                                work_ids: Some(available_work_ids),
                                skip_downloaded,
                                cancellation_token: Some(context.cancellation_token()),
                                progress_sink: Some(&progress_sink),
                            },
                            &source,
                        )
                        .await;

                    match result {
                        Ok(mut preview) => {
                            preview.total_count = candidates.total_count;
                            preview.skipped_downloaded_count += candidates.skipped_downloaded_count;
                            let dto = BulkWorkDownloadPreviewDto::from_preview(
                                preview,
                                skipped_queued_count,
                            );
                            let output = bulk_download_preview_output(&dto);

                            let _ = result_tx.send(Ok(dto));

                            Ok(output)
                        }
                        Err(error) => {
                            let failure = work_download_failure(error);
                            let message = failure.message.clone();

                            let _ = result_tx.send(Err(message));

                            Err(failure)
                        }
                    }
                },
            );

            record_audit(
                &state.audit,
                AuditEvent::queued("work.bulkDownload.preview", "Queued bulk download preview")
                    .with_details(json!({
                        "jobId": job_id.to_string(),
                        "metadata": audit_metadata,
                    })),
            )
            .await;

            result_rx.await.map_err(|_| {
                "bulk download preview job stopped before returning a result".to_owned()
            })?
        })
        .await
}

#[tauri::command]
pub(crate) async fn open_work_download(
    app: AppHandle,
    state: State<'_, AppState>,
    request: OpenWorkDownloadRequest,
) -> Result<(), String> {
    let diagnostic_operation = state.audit.operation("open_work_download");
    diagnostic_operation
        .command(async {
            let work_id = match normalize_required_id(request.work_id) {
                Ok(work_id) => work_id,
                Err(error) => {
                    record_audit(
                        &state.audit,
                        AuditEvent::failed("work.open", "Failed to validate open request")
                            .with_error(Some("validation"), error.clone()),
                    )
                    .await;
                    return Err(error);
                }
            };
            let download = match state.storage.work_download_state(&work_id).await {
                Ok(download) => download,
                Err(error) => {
                    let message = command_error(error);
                    record_audit(
                        &state.audit,
                        AuditEvent::failed("work.open", "Failed to load download state")
                            .with_error(Some("storage"), message.clone())
                            .with_details(json!({ "workId": work_id })),
                    )
                    .await;
                    return Err(message);
                }
            };

            if download.status != WorkDownloadStatus::Downloaded {
                let message = format!("{work_id} is not downloaded");
                record_audit(
                    &state.audit,
                    AuditEvent::failed("work.open", "Failed to open downloaded work")
                        .with_error(Some("not_downloaded"), message.clone())
                        .with_details(json!({ "workId": work_id })),
                )
                .await;
                return Err(message);
            }

            let local_path = download
                .local_path
                .as_deref()
                .ok_or_else(|| format!("{work_id} does not have a local path"))?;
            let canonical_path = match canonicalize_existing_path(local_path) {
                Ok(path) => path,
                Err(error) => {
                    record_audit(
                        &state.audit,
                        AuditEvent::failed("work.open", "Failed to resolve downloaded work path")
                            .with_error(Some("path"), error.clone())
                            .with_details(json!({ "workId": work_id, "path": local_path })),
                    )
                    .await;
                    return Err(error);
                }
            };
            let settings = match state.storage.app_settings().await {
                Ok(settings) => settings,
                Err(error) => {
                    let message = command_error(error);
                    record_audit(
                        &state.audit,
                        AuditEvent::failed("work.open", "Failed to load settings")
                            .with_error(Some("storage"), message.clone())
                            .with_details(json!({ "workId": work_id })),
                    )
                    .await;
                    return Err(message);
                }
            };
            let allowed_roots = canonical_open_roots(&app, &settings);

            if !path_is_under_any_root(&canonical_path, &allowed_roots) {
                let message = format!(
                    "download path is outside the configured library or staging folders: {}",
                    canonical_path.display()
                );
                record_audit(
                    &state.audit,
                    AuditEvent::failed(
                        "work.open",
                        "Refused to open path outside configured roots",
                    )
                    .with_error(Some("path_outside_roots"), message.clone())
                    .with_details(json!({
                        "workId": work_id,
                        "path": canonical_path.to_string_lossy().to_string(),
                    })),
                )
                .await;
                return Err(message);
            }

            match crate::opener::open_path(&app, &state.audit, &canonical_path).await {
                Ok(()) => {
                    record_audit(
                        &state.audit,
                        AuditEvent::succeeded("work.open", "Opened downloaded work").with_details(
                            json!({
                                "workId": work_id,
                                "path": canonical_path.to_string_lossy().to_string(),
                            }),
                        ),
                    )
                    .await;
                    Ok(())
                }
                Err(message) => {
                    record_audit(
                        &state.audit,
                        AuditEvent::failed("work.open", "Failed to open downloaded work")
                            .with_error(Some("opener"), message.clone())
                            .with_details(json!({
                                "workId": work_id,
                                "path": canonical_path.to_string_lossy().to_string(),
                            })),
                    )
                    .await;
                    Err(message)
                }
            }
        })
        .await
}

#[tauri::command]
pub(crate) async fn delete_work_download(
    app: AppHandle,
    state: State<'_, AppState>,
    player: State<'_, dldr_player::Player>,
    request: DeleteWorkDownloadRequest,
) -> Result<WorkDownloadStateDto, String> {
    let _admission = state.mutation_lock.read().await;
    let diagnostic_operation = state.audit.operation("delete_work_download");
    diagnostic_operation
        .command(async {
            let work_id = match normalize_required_id(request.work_id) {
                Ok(work_id) => work_id,
                Err(error) => {
                    record_audit(
                        &state.audit,
                        AuditEvent::failed(
                            "work.download.delete",
                            "Failed to validate delete download request",
                        )
                        .with_error(Some("validation"), error.clone()),
                    )
                    .await;
                    return Err(error);
                }
            };
            let settings = match state.storage.app_settings().await {
                Ok(settings) => settings,
                Err(error) => {
                    let message = command_error(error);
                    record_audit(
                        &state.audit,
                        AuditEvent::failed("work.download.delete", "Failed to load settings")
                            .with_error(Some("storage"), message.clone())
                            .with_details(json!({ "workId": work_id })),
                    )
                    .await;
                    return Err(message);
                }
            };
            let library_root = match required_library_root(&settings) {
                Ok(root) => root,
                Err(error) => {
                    record_audit(
                        &state.audit,
                        AuditEvent::failed(
                            "work.download.delete",
                            "Failed to resolve library folder",
                        )
                        .with_error(Some("settings"), error.clone())
                        .with_details(json!({ "workId": work_id })),
                    )
                    .await;
                    return Err(error);
                }
            };
            let download_root = match effective_download_root(&app, &settings) {
                Ok(root) => root,
                Err(error) => {
                    record_audit(
                        &state.audit,
                        AuditEvent::failed(
                            "work.download.delete",
                            "Failed to resolve download staging folder",
                        )
                        .with_error(Some("settings"), error.clone())
                        .with_details(json!({ "workId": work_id })),
                    )
                    .await;
                    return Err(error);
                }
            };

            if player.snapshot().work_id.as_deref() == Some(&work_id) {
                let player = player.inner().clone();
                let id = work_id.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    player.control(id, dldr_player::Control::Stop)
                })
                .await
                .map_err(command_error)?
                .map_err(command_error)?;
            }
            let mut removal =
                WorkDownloadRemovalRequest::new(&work_id, &library_root, &download_root);
            if request.completed_only {
                removal = removal.completed_only();
            }
            let result = state.library.remove_work_download(removal).await;

            match result {
                Ok(state_after_delete) => {
                    record_audit(
                        &state.audit,
                        AuditEvent::succeeded("work.download.delete", "Deleted work download")
                            .with_details(json!({ "workId": work_id })),
                    )
                    .await;
                    Ok(WorkDownloadStateDto::from(state_after_delete))
                }
                Err(error) => {
                    let message = command_error(error);
                    record_audit(
                        &state.audit,
                        AuditEvent::failed(
                            "work.download.delete",
                            "Failed to delete work download",
                        )
                        .with_error(Some("library"), message.clone())
                        .with_details(json!({ "workId": work_id })),
                    )
                    .await;
                    Err(message)
                }
            }
        })
        .await
}

#[tauri::command]
pub(crate) async fn export_work_download(
    state: State<'_, AppState>,
    work_id: String,
    destination_root: String,
) -> Result<String, String> {
    let _admission = state.mutation_lock.read().await;
    let operation = state.audit.operation("export_work_download");
    operation
        .command(async {
            let work_id = normalize_required_id(work_id)?;
            let settings = state.storage.app_settings().await.map_err(command_error)?;
            let library_root = required_library_root(&settings)?;
            let result = state
                .library
                .export_work_download(&work_id, &library_root, &PathBuf::from(destination_root))
                .await;
            match result {
                Ok(path) => {
                    record_audit(
                        &state.audit,
                        AuditEvent::succeeded("work.download.export", "Exported downloaded work")
                            .with_details(json!({"workId": work_id, "path": path})),
                    )
                    .await;
                    Ok(path.to_string_lossy().into_owned())
                }
                Err(error) => {
                    let message = command_error(error);
                    record_audit(
                        &state.audit,
                        AuditEvent::failed(
                            "work.download.export",
                            "Failed to export downloaded work",
                        )
                        .with_error(Some("library"), message.clone())
                        .with_details(json!({"workId": work_id})),
                    )
                    .await;
                    Err(message)
                }
            }
        })
        .await
}

#[tauri::command]
pub(crate) async fn mark_work_downloaded(
    state: State<'_, AppState>,
    request: MarkWorkDownloadedRequest,
) -> Result<WorkDownloadStateDto, String> {
    let _admission = state.mutation_lock.read().await;
    let diagnostic_operation = state.audit.operation("mark_work_downloaded");
    diagnostic_operation
        .command(async {
            let work_id = match normalize_required_id(request.work_id) {
                Ok(work_id) => work_id,
                Err(error) => {
                    record_audit(
                        &state.audit,
                        AuditEvent::failed(
                            "work.download.mark",
                            "Failed to validate manual download request",
                        )
                        .with_error(Some("validation"), error.clone()),
                    )
                    .await;
                    return Err(error);
                }
            };
            let local_path = match normalize_required_path(request.local_path) {
                Ok(path) => path,
                Err(error) => {
                    record_audit(
                        &state.audit,
                        AuditEvent::failed(
                            "work.download.mark",
                            "Failed to validate manual download path",
                        )
                        .with_error(Some("validation"), error.clone())
                        .with_details(json!({ "workId": work_id })),
                    )
                    .await;
                    return Err(error);
                }
            };
            let settings = match state.storage.app_settings().await {
                Ok(settings) => settings,
                Err(error) => {
                    let message = command_error(error);
                    record_audit(
                        &state.audit,
                        AuditEvent::failed("work.download.mark", "Failed to load settings")
                            .with_error(Some("storage"), message.clone())
                            .with_details(json!({ "workId": work_id })),
                    )
                    .await;
                    return Err(message);
                }
            };
            let library_root = match required_library_root(&settings) {
                Ok(root) => root,
                Err(error) => {
                    record_audit(
                        &state.audit,
                        AuditEvent::failed(
                            "work.download.mark",
                            "Failed to resolve library folder",
                        )
                        .with_error(Some("settings"), error.clone())
                        .with_details(json!({ "workId": work_id })),
                    )
                    .await;
                    return Err(error);
                }
            };

            let result = state
                .library
                .mark_work_downloaded(WorkDownloadMarkRequest::new(
                    &work_id,
                    &library_root,
                    &local_path,
                ))
                .await;

            match result {
                Ok(state_after_mark) => {
                    record_audit(
                        &state.audit,
                        AuditEvent::succeeded("work.download.mark", "Marked work as downloaded")
                            .with_details(json!({
                                "workId": work_id,
                                "path": state_after_mark.local_path,
                            })),
                    )
                    .await;
                    Ok(WorkDownloadStateDto::from(state_after_mark))
                }
                Err(error) => {
                    let message = command_error(error);
                    record_audit(
                        &state.audit,
                        AuditEvent::failed(
                            "work.download.mark",
                            "Failed to mark work as downloaded",
                        )
                        .with_error(Some("library"), message.clone())
                        .with_details(json!({
                            "workId": work_id,
                            "path": local_path.to_string_lossy().to_string(),
                        })),
                    )
                    .await;
                    Err(message)
                }
            }
        })
        .await
}

#[tauri::command]
pub(crate) async fn scan_local_work_downloads(
    state: State<'_, AppState>,
) -> Result<LocalWorkImportReportDto, String> {
    let _admission = state.mutation_lock.read().await;
    let diagnostic_operation = state.audit.operation("scan_local_work_downloads");
    diagnostic_operation
        .command(async {
            let settings = match state.storage.app_settings().await {
                Ok(settings) => settings,
                Err(error) => {
                    let message = command_error(error);
                    record_audit(
                        &state.audit,
                        AuditEvent::failed("work.local.scan", "Failed to load settings")
                            .with_error(Some("storage"), message.clone()),
                    )
                    .await;
                    return Err(message);
                }
            };
            let library_root = match required_library_root(&settings) {
                Ok(root) => root,
                Err(error) => {
                    record_audit(
                        &state.audit,
                        AuditEvent::failed("work.local.scan", "Failed to resolve library folder")
                            .with_error(Some("settings"), error.clone()),
                    )
                    .await;
                    return Err(error);
                }
            };
            let metadata_source =
                match dldr_api::DlsiteClient::new(dldr_api::DlsiteClientConfig::default()) {
                    Ok(client) => DlsitePublicMetadataSource::new(client),
                    Err(error) => {
                        let message = command_error(error);
                        record_audit(
                            &state.audit,
                            AuditEvent::failed(
                                "work.local.scan",
                                "Failed to prepare DLsite product metadata lookup",
                            )
                            .with_error(Some("api_client"), message.clone()),
                        )
                        .await;
                        return Err(message);
                    }
                };

            match state
                .library
                .import_local_work_downloads_with_metadata_source(
                    LocalWorkImportRequest::new(&library_root),
                    &metadata_source,
                )
                .await
            {
                Ok(report) => {
                    record_audit(
                        &state.audit,
                        AuditEvent::succeeded("work.local.scan", "Scanned local work folders")
                            .with_details(local_work_import_report_details(&report)),
                    )
                    .await;
                    Ok(LocalWorkImportReportDto::from(report))
                }
                Err(error) => {
                    let message = command_error(error);
                    record_audit(
                        &state.audit,
                        AuditEvent::failed("work.local.scan", "Failed to scan local work folders")
                            .with_error(Some("library"), message.clone()),
                    )
                    .await;
                    Err(message)
                }
            }
        })
        .await
}
