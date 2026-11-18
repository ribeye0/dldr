mod accounts;
mod bulk;
mod downloads;
mod local;
mod metadata;
mod products;
use async_trait::async_trait;
use chrono::{DateTime, SecondsFormat, Utc};
use dldr_api::{
    ApiError, ContentCount, ContentQuery, Credentials, DlsiteClient, DownloadFile, DownloadPlan,
    LoginOutcome, PublicWork, Purchase, SerialNumber, SessionSnapshot, SessionStatus,
    TwoFactorChallenge, Work, WorkId,
};
use dldr_credentials::{CredentialRef, CredentialStore, CredentialsError};
use dldr_download::{
    DownloadFileMetadata, DownloadJobRequest, DownloadProgress, DownloadedWork, UnpackPolicy,
};
pub use dldr_jobs::CancellationToken;
use dldr_storage::{
    Account, AccountSyncCommit, AccountUpsert, AccountWork, CachedWork, LocalWorkDownloadImport,
    ProductDetail, ProductFilterFacets, ProductListPage, ProductListQuery, Storage, StorageError,
    SyncCancellation, SyncFailure, WorkDownloadState, WorkDownloadStatus, WorkDownloadUpdate,
};
use futures_util::{stream::FuturesUnordered, StreamExt};
use metadata::*;
use serde_json::{json, Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use uuid::Uuid;

pub type Result<T> = std::result::Result<T, LibraryError>;

const BULK_DOWNLOAD_PAGE_LIMIT: u32 = 500;
pub const MAX_PARALLEL_WORK_DOWNLOADS: usize = 4;
const DOWNLOAD_CANCELLATION_POLL_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(50);
const SERIAL_INFORMATION_FILE_NAME: &str = "dldr-serial.txt";
const SERIAL_INFORMATION_NUMBERED_PREFIX: &str = "dldr-serial-";
const SERIAL_INFORMATION_MARKER: &str = "# dldr-desktop serial information";
mod auth;
mod instance;
pub use instance::{profile_dir, LibraryInstanceGuard};
pub mod cover;
mod credits;
pub mod product_page;
mod transfers;
pub use transfers::{DiskBudget, TransferCoordinator};
mod verification;
pub use verification::{VerificationItem, VerificationReport};
pub mod download_progress;
mod finalization;
use auth::acquire_login_lock;
pub use auth::{
    DlsiteAuthSource, TwoFactorPrompt, TwoFactorPromptRequest, TwoFactorPromptResponse,
};

mod error;
pub use error::LibraryError;
#[cfg(test)]
use error::*;

#[derive(Clone)]
pub struct Library {
    transfers: TransferCoordinator,
    storage: Storage,
    credentials: Arc<dyn CredentialStore>,
    /// One lock per account, held across authentication only.
    ///
    /// Without it, jobs started together would each begin their own login and each raise its
    /// own two-factor prompt. Serializing lets the first login store a session the others can
    /// then reuse.
    login_locks: Arc<std::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>,
    work_locks: Arc<std::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>,
    credit_locks: Arc<std::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>,
}

impl Library {
    pub fn new(storage: Storage, credentials: Arc<dyn CredentialStore>) -> Self {
        Self {
            storage,
            credentials,
            transfers: TransferCoordinator::default(),
            login_locks: Arc::new(std::sync::Mutex::new(HashMap::new())),
            work_locks: Arc::new(std::sync::Mutex::new(HashMap::new())),
            credit_locks: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }

    pub fn storage(&self) -> &Storage {
        &self.storage
    }
}

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

    fn check_cancelled(&self) -> Result<()> {
        if self
            .cancellation_token
            .is_some_and(CancellationToken::is_cancelled)
        {
            Err(LibraryError::Cancelled)
        } else {
            Ok(())
        }
    }

    fn emit(&self, progress: SyncProgress) {
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

    fn is_cancelled(&self) -> bool {
        self.cancellation_token
            .is_some_and(CancellationToken::is_cancelled)
    }

    fn check_cancelled(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(LibraryError::Cancelled)
        } else {
            Ok(())
        }
    }

    fn emit(&self, progress: WorkDownloadProgress) {
        if let Some(sink) = self.progress_sink {
            sink.emit(progress);
        }
    }

    fn forward_download_cancellation(
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

struct DownloadCancellationForwarder {
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
}

impl<'a> WorkDownloadRemovalRequest<'a> {
    pub fn new(work_id: &'a str, library_root: &'a Path, download_root: &'a Path) -> Self {
        Self {
            work_id,
            library_root,
            download_root,
        }
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

/// Holds one resolved name preference for the whole paginated selection.
struct BulkProductPages {
    query: ProductListQuery,
    exhausted: bool,
}

impl BulkProductPages {
    fn new(mut query: ProductListQuery) -> Self {
        query.limit = BULK_DOWNLOAD_PAGE_LIMIT;
        query.offset = 0;
        Self {
            query,
            exhausted: false,
        }
    }

    async fn next(&mut self, storage: &Storage) -> Result<Option<ProductListPage>> {
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

struct BulkWorkDownloadSelection {
    total_count: u64,
    skipped_downloaded_count: usize,
    items: Vec<BulkWorkDownloadSelectionItem>,
}

struct BulkWorkDownloadSelectionItem {
    work_id: String,
    title: String,
    content_size_bytes: Option<u64>,
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

    fn is_cancelled(&self) -> bool {
        self.cancellation_token
            .is_some_and(CancellationToken::is_cancelled)
    }

    fn check_cancelled(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(LibraryError::Cancelled)
        } else {
            Ok(())
        }
    }

    fn emit(&self, progress: BulkWorkDownloadProgress) {
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
    fn is_cancelled(&self) -> bool {
        self.cancellation_token
            .is_some_and(CancellationToken::is_cancelled)
    }

    fn check_cancelled(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(LibraryError::Cancelled)
        } else {
            Ok(())
        }
    }

    fn emit(&self, progress: BulkWorkDownloadPreviewProgress) {
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

#[async_trait]
pub trait WorkDownloadSource: DlsiteAuthSource {
    async fn download_plan(&self, work_id: &WorkId) -> Result<DownloadPlan>;
    async fn download_file_metadata(
        &self,
        file_index: usize,
        file: &DownloadFile,
    ) -> Result<DownloadFileMetadata>;
    async fn download_files(
        &self,
        job: &DownloadJobRequest,
        plan: &DownloadPlan,
        cancellation: &dldr_download::CancellationToken,
        progress_sink: &mut (dyn FnMut(DownloadProgress) + Send),
    ) -> Result<DownloadedWork>;
}

#[derive(Clone)]
pub struct DlsiteWorkDownloadSource {
    client: DlsiteClient,
    rate_limiter: Option<dldr_download::DownloadRateLimiter>,
}

impl DlsiteWorkDownloadSource {
    pub fn new(client: DlsiteClient) -> Self {
        Self {
            client,
            rate_limiter: None,
        }
    }

    pub fn with_rate_limiter(mut self, limiter: dldr_download::DownloadRateLimiter) -> Self {
        self.rate_limiter = Some(limiter);
        self
    }
}

#[async_trait]
impl DlsiteAuthSource for DlsiteWorkDownloadSource {
    async fn restore_session(&self, session: &SessionSnapshot) -> Result<()> {
        Ok(self.client.import_session(session)?)
    }

    async fn validate_session(&self) -> Result<bool> {
        Ok(self.client.validate_session().await? == SessionStatus::Authorized)
    }

    async fn begin_login(&self, credentials: &Credentials) -> Result<LoginOutcome> {
        Ok(self.client.begin_login(credentials).await?)
    }

    async fn complete_two_factor(
        &self,
        challenge: &TwoFactorChallenge,
        code: &str,
    ) -> Result<SessionSnapshot> {
        Ok(self.client.complete_two_factor(challenge, code).await?)
    }
}

#[async_trait]
impl WorkDownloadSource for DlsiteWorkDownloadSource {
    async fn download_plan(&self, work_id: &WorkId) -> Result<DownloadPlan> {
        Ok(self.client.download_plan(work_id).await?)
    }

    async fn download_file_metadata(
        &self,
        file_index: usize,
        file: &DownloadFile,
    ) -> Result<DownloadFileMetadata> {
        Ok(dldr_download::probe_download_file_metadata(&self.client, file_index, file).await?)
    }

    async fn download_files(
        &self,
        job: &DownloadJobRequest,
        plan: &DownloadPlan,
        cancellation: &dldr_download::CancellationToken,
        progress_sink: &mut (dyn FnMut(DownloadProgress) + Send),
    ) -> Result<DownloadedWork> {
        Ok(dldr_download::download_work_files_with_rate_limiter(
            self.client.clone(),
            job,
            plan,
            cancellation,
            self.rate_limiter.as_ref(),
            progress_sink,
        )
        .await?)
    }
}

#[async_trait]
pub trait LocalWorkMetadataSource: Send + Sync {
    async fn works(&self, ids: &[WorkId]) -> Result<Vec<PublicWork>>;
}

#[derive(Clone)]
pub struct DlsitePublicMetadataSource {
    client: DlsiteClient,
}

impl DlsitePublicMetadataSource {
    pub fn new(client: DlsiteClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl LocalWorkMetadataSource for DlsitePublicMetadataSource {
    async fn works(&self, ids: &[WorkId]) -> Result<Vec<PublicWork>> {
        Ok(self.client.public_works(ids).await?)
    }
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

#[async_trait]
pub trait AccountSyncSource: DlsiteAuthSource {
    async fn content_count(&self) -> Result<ContentCount>;
    async fn purchases(&self) -> Result<Vec<Purchase>>;
    async fn works(&self, ids: &[WorkId]) -> Result<Vec<Work>>;
}

#[derive(Clone)]
pub struct DlsiteSyncSource {
    client: DlsiteClient,
}

impl DlsiteSyncSource {
    pub fn new(client: DlsiteClient) -> Self {
        Self { client }
    }

    pub fn client(&self) -> &DlsiteClient {
        &self.client
    }
}

#[async_trait]
impl DlsiteAuthSource for DlsiteSyncSource {
    async fn restore_session(&self, session: &SessionSnapshot) -> Result<()> {
        Ok(self.client.import_session(session)?)
    }

    async fn validate_session(&self) -> Result<bool> {
        Ok(self.client.validate_session().await? == SessionStatus::Authorized)
    }

    async fn begin_login(&self, credentials: &Credentials) -> Result<LoginOutcome> {
        Ok(self.client.begin_login(credentials).await?)
    }

    async fn complete_two_factor(
        &self,
        challenge: &TwoFactorChallenge,
        code: &str,
    ) -> Result<SessionSnapshot> {
        Ok(self.client.complete_two_factor(challenge, code).await?)
    }
}

#[async_trait]
impl AccountSyncSource for DlsiteSyncSource {
    async fn content_count(&self) -> Result<ContentCount> {
        Ok(self.client.content_count(ContentQuery::default()).await?)
    }

    async fn purchases(&self) -> Result<Vec<Purchase>> {
        Ok(self.client.sales(ContentQuery::default()).await?)
    }

    async fn works(&self, ids: &[WorkId]) -> Result<Vec<Work>> {
        Ok(self.client.works(ids).await?)
    }
}

fn datetime_to_string(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn unpack_policy_storage_value(policy: UnpackPolicy) -> &'static str {
    match policy {
        UnpackPolicy::KeepArchives => "keep_archives",
        UnpackPolicy::UnpackWhenRecognized => "unpack_when_recognized",
    }
}

async fn write_serial_information_file(
    target_dir: &Path,
    work_id: &str,
    serial_numbers: &[SerialNumber],
    generated_at: &str,
) -> Result<Option<PathBuf>> {
    if serial_numbers.is_empty() {
        remove_owned_serial_information_files(target_dir).await?;
        return Ok(None);
    }

    let path = serial_information_path(target_dir).await?;
    let content = serial_information_content(work_id, serial_numbers, generated_at);

    tokio::fs::write(&path, content).await?;
    Ok(Some(path))
}

async fn serial_information_path(target_dir: &Path) -> Result<PathBuf> {
    let base = target_dir.join(SERIAL_INFORMATION_FILE_NAME);

    if serial_information_path_available(&base).await? {
        return Ok(base);
    }

    for index in 1..100 {
        let candidate = target_dir.join(format!("{SERIAL_INFORMATION_NUMBERED_PREFIX}{index}.txt"));

        if serial_information_path_available(&candidate).await? {
            return Ok(candidate);
        }
    }

    Err(LibraryError::Io(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not choose a serial information file name",
    )))
}

async fn serial_information_path_available(path: &Path) -> Result<bool> {
    match tokio::fs::read_to_string(path).await {
        Ok(content) => Ok(content.starts_with(SERIAL_INFORMATION_MARKER)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(error.into()),
    }
}

async fn remove_owned_serial_information_files(target_dir: &Path) -> Result<()> {
    remove_owned_serial_information_file(&target_dir.join(SERIAL_INFORMATION_FILE_NAME)).await?;

    for index in 1..100 {
        remove_owned_serial_information_file(
            &target_dir.join(format!("{SERIAL_INFORMATION_NUMBERED_PREFIX}{index}.txt")),
        )
        .await?;
    }

    Ok(())
}

async fn remove_owned_serial_information_file(path: &Path) -> Result<()> {
    match tokio::fs::read_to_string(path).await {
        Ok(content) if content.starts_with(SERIAL_INFORMATION_MARKER) => {
            tokio::fs::remove_file(path).await?;
            Ok(())
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn serial_information_content(
    work_id: &str,
    serial_numbers: &[SerialNumber],
    generated_at: &str,
) -> String {
    let mut content = String::new();

    content.push_str(SERIAL_INFORMATION_MARKER);
    content.push('\n');
    content.push_str(&format!("Work ID: {}\n", clean_serial_text(work_id)));
    content.push_str(&format!("Generated at: {generated_at}\n"));
    content.push_str("Source: DLsite download plan\n\n");
    content.push_str("Treat these serial values as sensitive.\n\n");
    content.push_str("Serial numbers:\n");

    for (index, serial) in serial_numbers.iter().enumerate() {
        content.push_str(&format!(
            "{}. {}\n",
            index + 1,
            clean_serial_text(&serial.label)
        ));
        content.push_str(&format!("   Value: {}\n", clean_serial_text(&serial.value)));
    }

    content
}

fn clean_serial_text(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_owned()
}

async fn remove_download_path_from_state(
    path: Option<&str>,
    allowed_roots: &[&Path],
) -> Result<()> {
    let Some(path) = path else {
        return Ok(());
    };

    remove_existing_download_path(Path::new(path), allowed_roots).await
}

async fn remove_existing_download_path(path: &Path, allowed_roots: &[&Path]) -> Result<()> {
    if !path.try_exists()? {
        return Ok(());
    }

    let canonical_path = path.canonicalize()?;
    let canonical_roots = allowed_roots
        .iter()
        .filter_map(|root| root.canonicalize().ok())
        .collect::<Vec<_>>();

    if !path_is_download_child_of_any_root(&canonical_path, &canonical_roots) {
        return Err(LibraryError::DownloadPathOutsideRoots(canonical_path));
    }

    let metadata = tokio::fs::metadata(&canonical_path).await?;
    if metadata.is_dir() {
        tokio::fs::remove_dir_all(&canonical_path).await?;
    } else {
        tokio::fs::remove_file(&canonical_path).await?;
    }

    Ok(())
}

fn canonicalize_existing_directory(path: &Path) -> Result<PathBuf> {
    let canonical_path = path.canonicalize()?;
    let metadata = std::fs::metadata(&canonical_path)?;

    if !metadata.is_dir() {
        return Err(LibraryError::DownloadPathNotDirectory(canonical_path));
    }

    Ok(canonical_path)
}

fn path_is_download_child_of_any_root(path: &Path, roots: &[PathBuf]) -> bool {
    roots
        .iter()
        .any(|root| path != root.as_path() && path.starts_with(root))
}

pub const DL_SITE_WORK_ID_PREFIXES: &[&str] = &["RJ", "VJ", "BJ"];
const WORK_ID_MIN_DIGITS: usize = 6;
const WORK_ID_MAX_DIGITS: usize = 8;

pub fn detect_work_ids_in_text(text: &str) -> Vec<WorkId> {
    let text = text.to_ascii_uppercase();
    let bytes = text.as_bytes();
    let mut index = 0;
    let mut seen = BTreeSet::new();
    let mut detected = Vec::new();

    while index + 2 + WORK_ID_MIN_DIGITS <= bytes.len() {
        let Some(prefix) = work_id_prefix_at(bytes, index) else {
            index += 1;
            continue;
        };

        if !is_work_id_boundary_before(bytes, index) {
            index += 1;
            continue;
        }

        let digit_start = index + prefix.len();
        let mut digit_end = digit_start;

        while digit_end < bytes.len() && bytes[digit_end].is_ascii_digit() {
            digit_end += 1;
        }

        let digit_count = digit_end.saturating_sub(digit_start);
        let is_valid = (WORK_ID_MIN_DIGITS..=WORK_ID_MAX_DIGITS).contains(&digit_count)
            && is_work_id_boundary_after(bytes, digit_end);

        if is_valid {
            let digits =
                std::str::from_utf8(&bytes[digit_start..digit_end]).expect("digits are ascii");
            let work_id = format!("{prefix}{digits}");

            if seen.insert(work_id.clone()) {
                detected.push(WorkId::from(work_id));
            }

            index = digit_end;
        } else {
            index += 1;
        }
    }

    detected
}

pub fn detect_work_ids_in_path(path: impl AsRef<Path>) -> Vec<WorkId> {
    path.as_ref()
        .file_name()
        .and_then(|file_name| file_name.to_str())
        .map(detect_work_ids_in_text)
        .unwrap_or_default()
}

pub fn detect_unique_work_id_in_path(path: impl AsRef<Path>) -> Option<WorkId> {
    let mut detected = detect_work_ids_in_path(path);

    if detected.len() == 1 {
        detected.pop()
    } else {
        None
    }
}

fn work_id_prefix_at(bytes: &[u8], index: usize) -> Option<&'static str> {
    DL_SITE_WORK_ID_PREFIXES.iter().copied().find(|prefix| {
        bytes
            .get(index..index + prefix.len())
            .is_some_and(|candidate| candidate == prefix.as_bytes())
    })
}

fn is_work_id_boundary_before(bytes: &[u8], index: usize) -> bool {
    index == 0 || !bytes[index - 1].is_ascii_alphanumeric()
}

fn is_work_id_boundary_after(bytes: &[u8], index: usize) -> bool {
    index == bytes.len() || !bytes[index].is_ascii_alphanumeric()
}

fn now_string() -> String {
    datetime_to_string(Utc::now())
}

#[cfg(test)]
mod tests;
