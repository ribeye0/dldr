//! Requests, results and progress shared by desktop and CLI adapters.
use crate::{
    download_progress, CancellationToken, LibraryError, Result, TwoFactorPrompt,
    DOWNLOAD_CANCELLATION_POLL_INTERVAL,
};
use dldr_download::{DownloadProgress, UnpackPolicy};
use dldr_storage::{ProductListQuery, WorkDownloadState};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveAccountRequest {
    pub id: Option<String>,
    pub label: String,
    pub login_name: Option<String>,
    pub password: Option<String>,
    pub remember_password: bool,
    /// Initial state for new accounts. Existing accounts use `set_account_enabled`.
    pub enabled: bool,
}

impl SaveAccountRequest {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            id: None,
            label: label.into(),
            login_name: None,
            password: None,
            remember_password: false,
            enabled: true,
        }
    }
}

#[derive(Clone, Copy)]
pub struct AccountSyncRequest<'a> {
    pub account_id: &'a str,
    pub password: Option<&'a str>,
    pub cancellation_token: Option<&'a CancellationToken>,
    pub progress_sink: Option<&'a dyn SyncProgressSink>,
    pub two_factor_prompt: Option<&'a dyn TwoFactorPrompt>,
}

impl<'a> AccountSyncRequest<'a> {
    pub fn new(account_id: &'a str) -> Self {
        Self {
            account_id,
            password: None,
            cancellation_token: None,
            progress_sink: None,
            two_factor_prompt: None,
        }
    }

    pub(super) fn check_cancelled(&self) -> Result<()> {
        if self
            .cancellation_token
            .is_some_and(CancellationToken::is_cancelled)
        {
            Err(LibraryError::Cancelled)
        } else {
            Ok(())
        }
    }

    pub(super) fn emit(&self, progress: SyncProgress) {
        if let Some(sink) = self.progress_sink {
            sink.emit(progress);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountSyncReport {
    pub account_id: String,
    pub sync_run_id: String,
    pub purchased_count: usize,
    pub cached_work_count: usize,
    pub missing_detail_count: usize,
    pub page_limit: Option<usize>,
    pub concurrency: Option<usize>,
}

#[derive(Clone, Copy)]
pub struct WorkDownloadRequest<'a> {
    pub resume_partial: bool,
    pub queue_batch: Option<&'a str>,
    pub work_id: &'a str,
    pub account_id: Option<&'a str>,
    pub password: Option<&'a str>,
    pub library_root: &'a Path,
    pub download_root: &'a Path,
    pub unpack_policy: UnpackPolicy,
    pub replace_existing: bool,
    pub cancellation_token: Option<&'a CancellationToken>,
    pub progress_sink: Option<&'a dyn WorkDownloadProgressSink>,
    pub two_factor_prompt: Option<&'a dyn TwoFactorPrompt>,
}

impl<'a> WorkDownloadRequest<'a> {
    pub fn new(work_id: &'a str, library_root: &'a Path, download_root: &'a Path) -> Self {
        Self {
            resume_partial: false,
            queue_batch: None,
            work_id,
            account_id: None,
            password: None,
            library_root,
            download_root,
            unpack_policy: UnpackPolicy::UnpackWhenRecognized,
            replace_existing: false,
            cancellation_token: None,
            progress_sink: None,
            two_factor_prompt: None,
        }
    }

    pub(super) fn is_cancelled(&self) -> bool {
        self.cancellation_token
            .is_some_and(CancellationToken::is_cancelled)
    }

    pub(super) fn check_cancelled(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(LibraryError::Cancelled)
        } else {
            Ok(())
        }
    }

    pub(super) fn emit(&self, progress: WorkDownloadProgress) {
        if let Some(sink) = self.progress_sink {
            sink.emit(progress);
        }
    }

    pub(super) fn forward_download_cancellation(
        &self,
        download_cancellation: &dldr_download::CancellationToken,
    ) -> Option<DownloadCancellationForwarder> {
        let job_cancellation = self.cancellation_token?.clone();
        let download_cancellation = download_cancellation.clone();

        if job_cancellation.is_cancelled() {
            download_cancellation.cancel();
        }

        let handle = tokio::spawn(async move {
            while !job_cancellation.is_cancelled() {
                tokio::time::sleep(DOWNLOAD_CANCELLATION_POLL_INTERVAL).await;
            }

            download_cancellation.cancel();
        });

        Some(DownloadCancellationForwarder { handle })
    }
}

pub(super) struct DownloadCancellationForwarder {
    handle: tokio::task::JoinHandle<()>,
}

impl Drop for DownloadCancellationForwarder {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

pub struct WorkDownloadRemovalRequest<'a> {
    pub work_id: &'a str,
    pub library_root: &'a Path,
    pub download_root: &'a Path,
    pub completed_only: bool,
}

impl<'a> WorkDownloadRemovalRequest<'a> {
    pub fn new(work_id: &'a str, library_root: &'a Path, download_root: &'a Path) -> Self {
        Self {
            work_id,
            library_root,
            download_root,
            completed_only: false,
        }
    }

    pub fn completed_only(mut self) -> Self {
        self.completed_only = true;
        self
    }
}

pub struct WorkDownloadMarkRequest<'a> {
    pub work_id: &'a str,
    pub library_root: &'a Path,
    pub local_path: &'a Path,
}

impl<'a> WorkDownloadMarkRequest<'a> {
    pub fn new(work_id: &'a str, library_root: &'a Path, local_path: &'a Path) -> Self {
        Self {
            work_id,
            library_root,
            local_path,
        }
    }
}

pub struct LocalWorkImportRequest<'a> {
    pub library_root: &'a Path,
}

impl<'a> LocalWorkImportRequest<'a> {
    pub fn new(library_root: &'a Path) -> Self {
        Self { library_root }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalWorkImportReport {
    pub recovery_errors: Vec<String>,
    pub scanned_directories: usize,
    pub imported_count: usize,
    pub skipped_no_id: usize,
    pub skipped_ambiguous: usize,
    pub skipped_non_utf8: usize,
    pub skipped_existing: usize,
    pub metadata_candidate_count: usize,
    pub metadata_updated_count: usize,
    pub metadata_missing_count: usize,
    pub metadata_error: Option<String>,
    pub imported_works: Vec<LocalWorkImportItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalWorkImportItem {
    pub work_id: String,
    pub local_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountRemovalReport {
    pub account_id: String,
    pub label: String,
    pub credential_deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkDownloadReport {
    pub warnings: Vec<String>,
    pub work_id: String,
    pub account_id: String,
    pub local_path: PathBuf,
    pub file_count: usize,
    pub archive_extracted: bool,
    pub download_state: WorkDownloadState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkDownloadProgress {
    Warning(String),
    LoggingIn,
    ResolvingPlan,
    Download(DownloadProgress),
    Finalizing,
    Completed,
}

pub trait WorkDownloadProgressSink: Send + Sync {
    fn emit(&self, progress: WorkDownloadProgress);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BulkDownloadCandidates {
    pub total_count: u64,
    pub skipped_downloaded_count: usize,
    pub work_ids: Vec<String>,
}

#[derive(Clone)]
pub struct BulkWorkDownloadRequest<'a> {
    pub queue_batch: Option<&'a str>,
    pub query: ProductListQuery,
    pub work_ids: Option<Vec<String>>,
    pub library_root: &'a Path,
    pub download_root: &'a Path,
    pub unpack_policy: UnpackPolicy,
    pub skip_downloaded: bool,
    pub cancellation_token: Option<&'a CancellationToken>,
    pub progress_sink: Option<&'a dyn BulkWorkDownloadProgressSink>,
    pub two_factor_prompt: Option<&'a dyn TwoFactorPrompt>,
}

impl<'a> BulkWorkDownloadRequest<'a> {
    pub fn new(query: ProductListQuery, library_root: &'a Path, download_root: &'a Path) -> Self {
        Self {
            queue_batch: None,
            query,
            work_ids: None,
            library_root,
            download_root,
            unpack_policy: UnpackPolicy::UnpackWhenRecognized,
            skip_downloaded: true,
            cancellation_token: None,
            progress_sink: None,
            two_factor_prompt: None,
        }
    }

    pub(super) fn is_cancelled(&self) -> bool {
        self.cancellation_token
            .is_some_and(CancellationToken::is_cancelled)
    }

    pub(super) fn check_cancelled(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(LibraryError::Cancelled)
        } else {
            Ok(())
        }
    }

    pub(super) fn emit(&self, progress: BulkWorkDownloadProgress) {
        if let Some(sink) = self.progress_sink {
            sink.emit(progress);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BulkWorkDownloadReport {
    pub total_count: u64,
    pub requested_count: usize,
    pub skipped_downloaded_count: usize,
    pub succeeded_count: usize,
    pub failed_count: usize,
    pub deferred_count: usize,
    pub succeeded_works: Vec<BulkWorkDownloadSuccess>,
    pub failed_works: Vec<BulkWorkDownloadFailure>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BulkWorkDownloadSuccess {
    pub warnings: Vec<String>,
    pub work_id: String,
    pub local_path: PathBuf,
    pub file_count: usize,
    pub archive_extracted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BulkWorkDownloadFailure {
    pub work_id: String,
    pub error_code: String,
    pub error_message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BulkWorkDownloadProgress {
    Selecting,
    Planned {
        works: Vec<download_progress::DownloadWorkPlan>,
    },
    WorkProgress {
        work_id: String,
        progress: WorkDownloadProgress,
    },
    Selected {
        total_count: u64,
        requested_count: usize,
        skipped_downloaded_count: usize,
    },
    WorkStarted {
        work_id: String,
        current: usize,
        total: usize,
    },
    WorkCompleted {
        work_id: String,
        current: usize,
        total: usize,
    },
    WorkFailed {
        work_id: String,
        current: usize,
        total: usize,
        error_code: String,
        error_message: String,
        /// Structured causes for consumers; raw text must pass diagnostic privacy projection.
        error_details: Value,
    },
    Completed {
        report: BulkWorkDownloadReport,
    },
}

pub trait BulkWorkDownloadProgressSink: Send + Sync {
    fn emit(&self, progress: BulkWorkDownloadProgress);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BulkWorkDownloadPreviewProgress {
    Selecting,
    Selected {
        total_count: u64,
        requested_count: usize,
        skipped_downloaded_count: usize,
    },
    WorkStarted {
        work_id: String,
        current: usize,
        total: usize,
    },
    WorkPlanned {
        work_id: String,
        current: usize,
        total: usize,
        known_expected_bytes: u64,
        total_expected_bytes: Option<u64>,
        unknown_size_count: usize,
    },
    WorkFailed {
        work_id: String,
        current: usize,
        total: usize,
        error_code: String,
        error_message: String,
    },
    Completed {
        planned_count: usize,
        failed_count: usize,
        known_expected_bytes: u64,
        total_expected_bytes: Option<u64>,
        unknown_size_count: usize,
    },
}

pub trait BulkWorkDownloadPreviewProgressSink: Send + Sync {
    fn emit(&self, progress: BulkWorkDownloadPreviewProgress);
}

#[derive(Clone)]
pub struct BulkWorkDownloadPreviewRequest<'a> {
    pub query: ProductListQuery,
    pub work_ids: Option<Vec<String>>,
    pub skip_downloaded: bool,
    pub cancellation_token: Option<&'a CancellationToken>,
    pub progress_sink: Option<&'a dyn BulkWorkDownloadPreviewProgressSink>,
}

impl BulkWorkDownloadPreviewRequest<'_> {
    pub(super) fn is_cancelled(&self) -> bool {
        self.cancellation_token
            .is_some_and(CancellationToken::is_cancelled)
    }

    pub(super) fn check_cancelled(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(LibraryError::Cancelled)
        } else {
            Ok(())
        }
    }

    pub(super) fn emit(&self, progress: BulkWorkDownloadPreviewProgress) {
        if let Some(sink) = self.progress_sink {
            sink.emit(progress);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BulkWorkDownloadPreview {
    pub total_count: u64,
    pub requested_count: usize,
    pub skipped_downloaded_count: usize,
    pub planned_count: usize,
    pub failed_count: usize,
    pub known_expected_bytes: u64,
    pub total_expected_bytes: Option<u64>,
    pub unknown_size_count: usize,
    pub works: Vec<BulkWorkDownloadPreviewWork>,
    pub failed_works: Vec<BulkWorkDownloadFailure>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BulkWorkDownloadPreviewWork {
    pub work_id: String,
    pub file_count: usize,
    pub known_expected_bytes: u64,
    pub total_expected_bytes: Option<u64>,
    pub unknown_size_count: usize,
    pub files: Vec<BulkWorkDownloadPreviewFile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BulkWorkDownloadPreviewFile {
    pub file_index: usize,
    pub file_name: String,
    pub expected_size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncProgress {
    LoggingIn,
    LoadingCount,
    LoadingPurchases,
    LoadingWorks {
        work_count: usize,
    },
    Committing {
        work_count: usize,
    },
    Completed {
        sync_run_id: String,
        cached_work_count: usize,
    },
}

pub trait SyncProgressSink: Send + Sync {
    fn emit(&self, progress: SyncProgress);
}
