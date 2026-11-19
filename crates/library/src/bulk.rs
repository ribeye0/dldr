use crate::download_progress;
use crate::{
    BulkDownloadCandidates, BulkWorkDownloadFailure, BulkWorkDownloadPreview,
    BulkWorkDownloadPreviewFile, BulkWorkDownloadPreviewProgress, BulkWorkDownloadPreviewRequest,
    BulkWorkDownloadPreviewWork, BulkWorkDownloadProgress, BulkWorkDownloadReport,
    BulkWorkDownloadRequest, BulkWorkDownloadSuccess, CancellationToken, Library, LibraryError,
    Result, WorkDownloadProgress, WorkDownloadProgressSink, WorkDownloadReport,
    WorkDownloadRequest, WorkDownloadSource, DOWNLOAD_CANCELLATION_POLL_INTERVAL,
    MAX_PARALLEL_WORK_DOWNLOADS,
};
use dldr_storage::{ProductListPage, ProductListQuery, Storage, WorkDownloadStatus};
use futures_util::stream::FuturesUnordered;
use futures_util::StreamExt;
use serde_json::json;
use std::collections::BTreeSet;

pub(super) const BULK_DOWNLOAD_PAGE_LIMIT: u32 = 500;

/// Holds one resolved name preference for the whole paginated selection.
pub(super) struct BulkProductPages {
    query: ProductListQuery,
    exhausted: bool,
}

impl BulkProductPages {
    pub(super) fn new(mut query: ProductListQuery) -> Self {
        query.limit = BULK_DOWNLOAD_PAGE_LIMIT;
        query.offset = 0;
        Self {
            query,
            exhausted: false,
        }
    }

    pub(super) async fn next(&mut self, storage: &Storage) -> Result<Option<ProductListPage>> {
        if self.exhausted {
            return Ok(None);
        }
        let page = storage.list_products(&self.query).await?;
        self.exhausted = page.products.len() < self.query.limit as usize;
        self.query.offset = self.query.offset.saturating_add(page.products.len() as u32);
        Ok(Some(page))
    }
}

enum BulkDownloadSource<'a, S> {
    Borrowed(&'a S),
    Owned(S),
}

pub(super) struct BulkWorkDownloadSelection {
    total_count: u64,
    skipped_downloaded_count: usize,
    pub(super) items: Vec<BulkWorkDownloadSelectionItem>,
}

pub(super) struct BulkWorkDownloadSelectionItem {
    pub(super) work_id: String,
    title: String,
    content_size_bytes: Option<u64>,
}

struct BulkWorkProgressForwarder<'a, 'b> {
    request: &'a BulkWorkDownloadRequest<'b>,
    work_id: &'a str,
}

impl WorkDownloadProgressSink for BulkWorkProgressForwarder<'_, '_> {
    fn emit(&self, progress: WorkDownloadProgress) {
        self.request.emit(BulkWorkDownloadProgress::WorkProgress {
            work_id: self.work_id.to_owned(),
            progress,
        });
    }
}

impl Library {
    pub async fn select_bulk_candidates(
        &self,
        query: &ProductListQuery,
        skip_downloaded: bool,
        work_ids: Option<&[String]>,
    ) -> Result<BulkDownloadCandidates> {
        let selection = self
            .bulk_download_selection(query, skip_downloaded, work_ids, None)
            .await?;
        Ok(BulkDownloadCandidates {
            total_count: selection.total_count,
            skipped_downloaded_count: selection.skipped_downloaded_count,
            work_ids: selection
                .items
                .into_iter()
                .map(|item| item.work_id)
                .collect(),
        })
    }

    pub async fn download_products_with_source<S>(
        &self,
        request: BulkWorkDownloadRequest<'_>,
        source: &S,
    ) -> Result<BulkWorkDownloadReport>
    where
        S: WorkDownloadSource + Sync,
    {
        self.download_products_using(request, 1, || Ok(BulkDownloadSource::Borrowed(source)))
            .await
    }

    /// Creates an independent cookie jar for each work. Cloned clients share
    /// cookies and must not be used to authenticate different accounts in parallel.
    pub async fn download_products_with_source_factory<S, F>(
        &self,
        request: BulkWorkDownloadRequest<'_>,
        max_concurrent_works: usize,
        factory: F,
    ) -> Result<BulkWorkDownloadReport>
    where
        S: WorkDownloadSource + Sync,
        F: Fn() -> Result<S> + Send + Sync,
    {
        self.download_products_using(request, max_concurrent_works, || {
            factory().map(BulkDownloadSource::Owned)
        })
        .await
    }

    async fn download_products_using<'s, S, F>(
        &self,
        request: BulkWorkDownloadRequest<'_>,
        max_concurrent_works: usize,
        factory: F,
    ) -> Result<BulkWorkDownloadReport>
    where
        S: WorkDownloadSource + Sync + 's,
        F: Fn() -> Result<BulkDownloadSource<'s, S>> + Send + Sync,
    {
        if !(1..=MAX_PARALLEL_WORK_DOWNLOADS).contains(&max_concurrent_works) {
            return Err(LibraryError::InvalidDownloadConcurrency);
        }
        request.check_cancelled()?;
        request.emit(BulkWorkDownloadProgress::Selecting);

        let selection = self
            .bulk_download_selection(
                &request.query,
                request.skip_downloaded,
                request.work_ids.as_deref(),
                request.cancellation_token,
            )
            .await?;

        let requested_count = selection.items.len();
        request.emit(BulkWorkDownloadProgress::Selected {
            total_count: selection.total_count,
            requested_count,
            skipped_downloaded_count: selection.skipped_downloaded_count,
        });
        request.emit(BulkWorkDownloadProgress::Planned {
            works: selection
                .items
                .iter()
                .map(|item| download_progress::DownloadWorkPlan {
                    work_id: item.work_id.clone(),
                    title: item.title.clone(),
                    bytes_total: item.content_size_bytes,
                })
                .collect(),
        });

        let mut report = BulkWorkDownloadReport {
            total_count: selection.total_count,
            requested_count,
            skipped_downloaded_count: selection.skipped_downloaded_count,
            succeeded_count: 0,
            failed_count: 0,
            deferred_count: 0,
            succeeded_works: Vec::new(),
            failed_works: Vec::new(),
        };

        let batch_token = CancellationToken::new();
        let worker_request = BulkWorkDownloadRequest {
            cancellation_token: Some(&batch_token),
            ..request.clone()
        };
        let mut pending = selection.items.into_iter().enumerate();
        let mut active = FuturesUnordered::new();
        let mut interrupted = None;
        loop {
            if request.is_cancelled() && interrupted.is_none() {
                interrupted = Some((LibraryError::Cancelled, None));
                batch_token.cancel();
            }
            while interrupted.is_none() && active.len() < max_concurrent_works {
                let Some((index, item)) = pending.next() else {
                    break;
                };
                active.push(self.download_bulk_item(
                    &worker_request,
                    item,
                    index + 1,
                    requested_count,
                    &factory,
                ));
            }
            if active.is_empty() {
                break;
            }
            // Cancellation is cooperative: cancel and drain every started operation
            // before publishing a terminal report or releasing native reservations.
            let next = tokio::select! {
                result = active.next() => result,
                _ = tokio::time::sleep(DOWNLOAD_CANCELLATION_POLL_INTERVAL) => continue,
            };
            let Some((work_id, work_result)) = next else {
                break;
            };
            let current = report.succeeded_count + report.failed_count + report.deferred_count + 1;
            match work_result {
                Ok(report_item) => {
                    report.succeeded_count += 1;
                    report.succeeded_works.push(BulkWorkDownloadSuccess {
                        work_id: report_item.work_id.clone(),
                        local_path: report_item.local_path,
                        file_count: report_item.file_count,
                        archive_extracted: report_item.archive_extracted,
                        warnings: report_item.warnings,
                    });
                    request.emit(BulkWorkDownloadProgress::WorkCompleted {
                        work_id,
                        current,
                        total: requested_count,
                    });
                }
                Err(LibraryError::QueueStopped) => {
                    report.deferred_count += 1;
                    request.emit(BulkWorkDownloadProgress::WorkFailed {
                        work_id,
                        current,
                        total: requested_count,
                        error_code: "work_deferred".into(),
                        error_message: "Work paused or cancelled individually".into(),
                        error_details: json!({}),
                    });
                }
                Err(error) if error.is_cancelled() => {
                    if interrupted.is_none() {
                        interrupted = Some((error, Some(work_id)));
                        batch_token.cancel();
                    }
                }
                Err(error) => {
                    let error_code = error.failure_code().to_owned();
                    let error_message = error.support_message();

                    report.failed_count += 1;
                    report.failed_works.push(BulkWorkDownloadFailure {
                        work_id: work_id.clone(),
                        error_code: error_code.clone(),
                        error_message: error_message.clone(),
                    });
                    request.emit(BulkWorkDownloadProgress::WorkFailed {
                        work_id,
                        current,
                        total: requested_count,
                        error_code,
                        error_message,
                        error_details: error.support_details(),
                    });
                }
            }
        }

        if let Some(batch) = request.queue_batch {
            self.storage.pause_queued_batch(batch).await?;
        }
        if let Some((cause, interrupted_work)) = interrupted {
            return Err(LibraryError::BulkInterrupted {
                cause: Box::new(cause),
                report: Box::new(report),
                interrupted_work,
            });
        }

        request.emit(BulkWorkDownloadProgress::Completed {
            report: report.clone(),
        });

        Ok(report)
    }

    async fn download_bulk_item<'s, S, F>(
        &self,
        request: &BulkWorkDownloadRequest<'_>,
        item: BulkWorkDownloadSelectionItem,
        current: usize,
        total: usize,
        factory: &F,
    ) -> (String, Result<WorkDownloadReport>)
    where
        S: WorkDownloadSource + Sync + 's,
        F: Fn() -> Result<BulkDownloadSource<'s, S>> + Sync,
    {
        let work_id = item.work_id;
        let result = async {
            request.check_cancelled()?;
            request.emit(BulkWorkDownloadProgress::WorkStarted {
                work_id: work_id.clone(),
                current,
                total,
            });
            let source = factory()?;
            let source = match &source {
                BulkDownloadSource::Borrowed(source) => *source,
                BulkDownloadSource::Owned(source) => source,
            };
            let forwarder = BulkWorkProgressForwarder {
                request,
                work_id: &work_id,
            };
            self.download_work_with_source(
                WorkDownloadRequest {
                    resume_partial: false,
                    queue_batch: request.queue_batch,
                    work_id: &work_id,
                    account_id: request.query.account_id.as_deref(),
                    password: None,
                    library_root: request.library_root,
                    download_root: request.download_root,
                    unpack_policy: request.unpack_policy,
                    replace_existing: false,
                    cancellation_token: request.cancellation_token,
                    progress_sink: Some(&forwarder),
                    two_factor_prompt: request.two_factor_prompt,
                },
                source,
            )
            .await
        }
        .await;
        (work_id, result)
    }

    pub async fn preview_download_products_with_source<S>(
        &self,
        request: BulkWorkDownloadPreviewRequest<'_>,
        _source: &S,
    ) -> Result<BulkWorkDownloadPreview>
    where
        S: WorkDownloadSource + Sync,
    {
        request.check_cancelled()?;
        request.emit(BulkWorkDownloadPreviewProgress::Selecting);
        let selection = self
            .bulk_download_selection(
                &request.query,
                request.skip_downloaded,
                request.work_ids.as_deref(),
                request.cancellation_token,
            )
            .await?;
        let requested_count = selection.items.len();
        request.emit(BulkWorkDownloadPreviewProgress::Selected {
            total_count: selection.total_count,
            requested_count,
            skipped_downloaded_count: selection.skipped_downloaded_count,
        });
        let mut preview = BulkWorkDownloadPreview {
            total_count: selection.total_count,
            requested_count,
            skipped_downloaded_count: selection.skipped_downloaded_count,
            planned_count: 0,
            failed_count: 0,
            known_expected_bytes: 0,
            total_expected_bytes: Some(0),
            unknown_size_count: 0,
            works: Vec::new(),
            failed_works: Vec::new(),
        };
        for (index, item) in selection.items.into_iter().enumerate() {
            request.check_cancelled()?;
            let current = index + 1;
            let work_id = item.work_id;
            request.emit(BulkWorkDownloadPreviewProgress::WorkStarted {
                work_id: work_id.clone(),
                current,
                total: requested_count,
            });

            if let Some(content_size_bytes) = item.content_size_bytes {
                let work =
                    self.preview_work_download_from_cached_size(&work_id, content_size_bytes);

                preview.planned_count += 1;
                preview.known_expected_bytes = preview
                    .known_expected_bytes
                    .saturating_add(work.known_expected_bytes);
                preview.total_expected_bytes = preview
                    .total_expected_bytes
                    .map(|total| total.saturating_add(content_size_bytes));
                preview.works.push(work);
                request.emit(BulkWorkDownloadPreviewProgress::WorkPlanned {
                    work_id,
                    current,
                    total: requested_count,
                    known_expected_bytes: content_size_bytes,
                    total_expected_bytes: Some(content_size_bytes),
                    unknown_size_count: 0,
                });
                continue;
            }

            let work = self.preview_work_download_from_unknown_cached_size(&work_id);
            preview.planned_count += 1;
            preview.unknown_size_count = preview.unknown_size_count.saturating_add(1);
            preview.total_expected_bytes = None;
            preview.works.push(work);
            request.emit(BulkWorkDownloadPreviewProgress::WorkPlanned {
                work_id,
                current,
                total: requested_count,
                known_expected_bytes: 0,
                total_expected_bytes: None,
                unknown_size_count: 1,
            });
        }

        request.emit(BulkWorkDownloadPreviewProgress::Completed {
            planned_count: preview.planned_count,
            failed_count: preview.failed_count,
            known_expected_bytes: preview.known_expected_bytes,
            total_expected_bytes: preview.total_expected_bytes,
            unknown_size_count: preview.unknown_size_count,
        });

        Ok(preview)
    }

    pub(super) async fn bulk_download_selection(
        &self,
        query: &ProductListQuery,
        skip_downloaded: bool,
        work_ids: Option<&[String]>,
        cancellation_token: Option<&CancellationToken>,
    ) -> Result<BulkWorkDownloadSelection> {
        if cancellation_token.is_some_and(CancellationToken::is_cancelled) {
            return Err(LibraryError::Cancelled);
        }

        let allowed_work_ids =
            work_ids.map(|work_ids| work_ids.iter().map(String::as_str).collect::<BTreeSet<_>>());
        let query = self.resolved_query(query).await?;
        let total_count = self.storage.list_products(&query).await?.total_count;
        let mut pages = BulkProductPages::new(query);

        let mut work_ids = Vec::new();
        let mut skipped_downloaded_count = 0usize;
        let mut matched_count = 0u64;
        let mut seen = BTreeSet::new();

        loop {
            if cancellation_token.is_some_and(CancellationToken::is_cancelled) {
                return Err(LibraryError::Cancelled);
            }

            let Some(page) = pages.next(&self.storage).await? else {
                break;
            };
            for product in page.products {
                if !seen.insert(product.work_id.clone()) {
                    continue;
                }
                if let Some(allowed_work_ids) = &allowed_work_ids {
                    if !allowed_work_ids.contains(product.work_id.as_str()) {
                        continue;
                    }
                }
                matched_count += 1;
                if skip_downloaded && product.download.status == WorkDownloadStatus::Downloaded {
                    skipped_downloaded_count += 1;
                    continue;
                }

                work_ids.push(BulkWorkDownloadSelectionItem {
                    work_id: product.work_id,
                    title: product.title,
                    content_size_bytes: product.content_size_bytes,
                });
            }
        }

        Ok(BulkWorkDownloadSelection {
            total_count: if allowed_work_ids.is_some() {
                matched_count
            } else {
                total_count
            },
            skipped_downloaded_count,
            items: work_ids,
        })
    }

    pub(super) fn preview_work_download_from_cached_size(
        &self,
        work_id: &str,
        content_size_bytes: u64,
    ) -> BulkWorkDownloadPreviewWork {
        BulkWorkDownloadPreviewWork {
            work_id: work_id.to_owned(),
            file_count: 1,
            known_expected_bytes: content_size_bytes,
            total_expected_bytes: Some(content_size_bytes),
            unknown_size_count: 0,
            files: vec![BulkWorkDownloadPreviewFile {
                file_index: 0,
                file_name: "DLsite content".to_owned(),
                expected_size: Some(content_size_bytes),
            }],
        }
    }

    pub(super) fn preview_work_download_from_unknown_cached_size(
        &self,
        work_id: &str,
    ) -> BulkWorkDownloadPreviewWork {
        BulkWorkDownloadPreviewWork {
            work_id: work_id.to_owned(),
            file_count: 0,
            known_expected_bytes: 0,
            total_expected_bytes: None,
            unknown_size_count: 1,
            files: Vec::new(),
        }
    }
}
