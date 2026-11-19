pub(super) use crate::error::safe_body_snippet_for_log;
pub(super) use crate::metadata::{
    cached_work_from_api, cached_work_from_local_folder, cached_work_from_public_api,
};
pub(super) use crate::paths::path_is_download_child_of_any_root;
pub(super) use crate::serial::{SERIAL_INFORMATION_FILE_NAME, SERIAL_INFORMATION_MARKER};
pub(super) use async_trait::async_trait;
pub(super) use chrono::{DateTime, Utc};
pub(super) use dldr_api::{
    ApiError, ContentCount, Credentials, DownloadFile, DownloadPlan, LoginOutcome, PublicWork,
    Purchase, SerialNumber, SessionSnapshot, TwoFactorChallenge, Work, WorkId,
};
pub(super) use dldr_credentials::CredentialRef;
pub(super) use dldr_download::{
    DownloadFileMetadata, DownloadJobRequest, DownloadProgress, DownloadedWork,
};
pub(super) use dldr_storage::{
    AccountSyncCommit, AccountWork, ProductListQuery, Storage, WorkDownloadStatus,
};
pub(super) use serde_json::{json, Value};
pub(super) use std::{
    collections::BTreeMap, collections::BTreeSet, path::Path, path::PathBuf, sync::Arc,
    time::Duration,
};
mod diagnostics;
mod download_auth;
use super::*;
mod language_priority;
mod maintenance;
mod public_credits;
use dldr_api::{
    AgeCategory, DownloadFileKind, DownloadStreamRequest, Language, LocalizedText, Maker,
    PublicWorkImage, PublicWorkImageObject, WorkKind, WorkThumbnail,
};
use dldr_credentials::InMemoryCredentialStore;
use dldr_download::{DownloadPhase, DownloadedFile};
use dldr_storage::{ProductSort, SyncRunStatus};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};
use url::Url;

#[derive(Debug, Clone)]
struct FakeSyncSource {
    content_count: ContentCount,
    purchases: Vec<Purchase>,
    works: Vec<Work>,
    fail_at: Option<FakeFailurePoint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FakeFailurePoint {
    Login,
    Works,
}

fn assert_fake_credentials(credentials: &Credentials) {
    assert_eq!(credentials.username, "user@example.test");
    assert_eq!(credentials.password, "secret");
}

fn fake_session() -> SessionSnapshot {
    SessionSnapshot {
        cookies_json: "{}".to_owned(),
    }
}

/// Test sources authenticate with a password in one step; `validate_session` always says
/// no so every operation exercises the login path rather than a stored session.
macro_rules! impl_fake_auth_source {
    ($source:ty) => {
        #[async_trait]
        impl DlsiteAuthSource for $source {
            async fn restore_session(&self, _session: &SessionSnapshot) -> Result<()> {
                Ok(())
            }

            async fn validate_session(&self) -> Result<bool> {
                Ok(false)
            }

            async fn begin_login(&self, credentials: &Credentials) -> Result<LoginOutcome> {
                assert_fake_credentials(credentials);
                Ok(LoginOutcome::Authorized(fake_session()))
            }

            async fn complete_two_factor(
                &self,
                _challenge: &TwoFactorChallenge,
                _code: &str,
            ) -> Result<SessionSnapshot> {
                unreachable!("fake source never issues a two-factor challenge")
            }
        }
    };
}

#[async_trait]
impl DlsiteAuthSource for FakeSyncSource {
    async fn restore_session(&self, _session: &SessionSnapshot) -> Result<()> {
        Ok(())
    }

    async fn validate_session(&self) -> Result<bool> {
        Ok(false)
    }

    async fn begin_login(&self, credentials: &Credentials) -> Result<LoginOutcome> {
        assert_fake_credentials(credentials);

        if self.fail_at == Some(FakeFailurePoint::Login) {
            return Err(LibraryError::SyncSource("login failed".to_owned()));
        }

        Ok(LoginOutcome::Authorized(fake_session()))
    }

    async fn complete_two_factor(
        &self,
        _challenge: &TwoFactorChallenge,
        _code: &str,
    ) -> Result<SessionSnapshot> {
        unreachable!("fake sync source never issues a two-factor challenge")
    }
}

#[async_trait]
impl AccountSyncSource for FakeSyncSource {
    async fn content_count(&self) -> Result<ContentCount> {
        Ok(self.content_count.clone())
    }

    async fn purchases(&self) -> Result<Vec<Purchase>> {
        Ok(self.purchases.clone())
    }

    async fn works(&self, ids: &[WorkId]) -> Result<Vec<Work>> {
        if self.fail_at == Some(FakeFailurePoint::Works) {
            return Err(LibraryError::SyncSource("works failed".to_owned()));
        }

        let requested = ids.iter().map(|id| id.as_ref()).collect::<Vec<_>>();
        let works = self
            .works
            .iter()
            .filter(|work| requested.contains(&work.id.as_ref()))
            .cloned()
            .collect();

        Ok(works)
    }
}

mod parallel_downloads;

#[derive(Debug, Clone, Default)]
struct FakeLocalWorkMetadataSource {
    works: Vec<PublicWork>,
    fail: bool,
}

#[async_trait]
impl LocalWorkMetadataSource for FakeLocalWorkMetadataSource {
    async fn works(&self, ids: &[WorkId]) -> Result<Vec<PublicWork>> {
        if self.fail {
            return Err(LibraryError::SyncSource(
                "local metadata lookup failed".to_owned(),
            ));
        }

        let requested = ids.iter().map(|id| id.as_ref()).collect::<Vec<_>>();
        Ok(self
            .works
            .iter()
            .filter(|work| requested.contains(&work.id.as_ref()))
            .cloned()
            .collect())
    }
}

#[derive(Debug, Clone, Default)]
struct FakeDownloadSource;

impl_fake_auth_source!(FakeDownloadSource);

#[async_trait]
impl WorkDownloadSource for FakeDownloadSource {
    async fn download_plan(&self, work_id: &WorkId) -> Result<DownloadPlan> {
        Ok(DownloadPlan {
            work_id: work_id.clone(),
            files: vec![DownloadFile {
                kind: DownloadFileKind::Direct,
                stream_request: DownloadStreamRequest {
                    url: Url::parse("https://download.example.test/get/=/file/RJ000001.zip/_/1")
                        .unwrap(),
                },
            }],
            serial_numbers: Vec::new(),
        })
    }

    async fn download_file_metadata(
        &self,
        file_index: usize,
        file: &DownloadFile,
    ) -> Result<DownloadFileMetadata> {
        Ok(DownloadFileMetadata {
            file_index,
            file_kind: file.kind.clone(),
            file_name: format!("file-{file_index}.zip"),
            expected_size: Some(10),
            final_url: file.stream_request.url.clone(),
        })
    }

    async fn download_files(
        &self,
        job: &DownloadJobRequest,
        _plan: &DownloadPlan,
        _cancellation: &dldr_download::CancellationToken,
        progress_sink: &mut (dyn FnMut(DownloadProgress) + Send),
    ) -> Result<DownloadedWork> {
        let target_dir = job.target_root.join(job.work_id.as_ref());
        let path = target_dir.join("RJ000001.txt");
        tokio::fs::create_dir_all(&target_dir).await?;
        tokio::fs::write(&path, b"downloaded").await?;
        progress_sink(DownloadProgress {
            phase: DownloadPhase::Downloading,
            file_index: Some(0),
            file_kind: Some(DownloadFileKind::Direct),
            bytes_received: 10,
            bytes_total: Some(10),
        });

        Ok(DownloadedWork {
            work_id: job.work_id.clone(),
            target_dir,
            files: vec![DownloadedFile {
                file_name: "RJ000001.txt".to_owned(),
                path,
                bytes_written: 10,
                resumed_from: 0,
            }],
            archive_extraction: None,
        })
    }
}

#[derive(Debug, Clone, Default)]
struct SerialDownloadSource;

impl_fake_auth_source!(SerialDownloadSource);

#[async_trait]
impl WorkDownloadSource for SerialDownloadSource {
    async fn download_plan(&self, work_id: &WorkId) -> Result<DownloadPlan> {
        let mut plan = FakeDownloadSource.download_plan(work_id).await?;
        plan.serial_numbers = vec![SerialNumber {
            label: "Serial number".to_owned(),
            value: "ABC-123-SECRET".to_owned(),
        }];

        Ok(plan)
    }

    async fn download_file_metadata(
        &self,
        file_index: usize,
        file: &DownloadFile,
    ) -> Result<DownloadFileMetadata> {
        FakeDownloadSource
            .download_file_metadata(file_index, file)
            .await
    }

    async fn download_files(
        &self,
        job: &DownloadJobRequest,
        plan: &DownloadPlan,
        cancellation: &dldr_download::CancellationToken,
        progress_sink: &mut (dyn FnMut(DownloadProgress) + Send),
    ) -> Result<DownloadedWork> {
        FakeDownloadSource
            .download_files(job, plan, cancellation, progress_sink)
            .await
    }
}

#[derive(Debug, Clone)]
struct WaitingDownloadSource {
    entered_download: Arc<AtomicBool>,
}

impl WaitingDownloadSource {
    fn new() -> Self {
        Self {
            entered_download: Arc::new(AtomicBool::new(false)),
        }
    }

    fn entered_download(&self) -> bool {
        self.entered_download.load(Ordering::SeqCst)
    }
}

impl_fake_auth_source!(WaitingDownloadSource);

#[async_trait]
impl WorkDownloadSource for WaitingDownloadSource {
    async fn download_plan(&self, work_id: &WorkId) -> Result<DownloadPlan> {
        Ok(DownloadPlan {
            work_id: work_id.clone(),
            files: vec![DownloadFile {
                kind: DownloadFileKind::Direct,
                stream_request: DownloadStreamRequest {
                    url: Url::parse("https://download.example.test/get/=/file/RJ000001.zip/_/1")
                        .unwrap(),
                },
            }],
            serial_numbers: Vec::new(),
        })
    }

    async fn download_file_metadata(
        &self,
        file_index: usize,
        file: &DownloadFile,
    ) -> Result<DownloadFileMetadata> {
        Ok(DownloadFileMetadata {
            file_index,
            file_kind: file.kind.clone(),
            file_name: format!("file-{file_index}.zip"),
            expected_size: Some(10),
            final_url: file.stream_request.url.clone(),
        })
    }

    async fn download_files(
        &self,
        _job: &DownloadJobRequest,
        _plan: &DownloadPlan,
        cancellation: &dldr_download::CancellationToken,
        _progress_sink: &mut (dyn FnMut(DownloadProgress) + Send),
    ) -> Result<DownloadedWork> {
        self.entered_download.store(true, Ordering::SeqCst);

        loop {
            if cancellation.is_cancelled() {
                return Err(dldr_download::DownloadError::Cancelled.into());
            }

            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

#[derive(Debug, Clone)]
struct FailingDownloadSource {
    fail_work_id: &'static str,
    failure: fn() -> LibraryError,
}

impl_fake_auth_source!(FailingDownloadSource);

#[async_trait]
impl WorkDownloadSource for FailingDownloadSource {
    async fn download_plan(&self, work_id: &WorkId) -> Result<DownloadPlan> {
        FakeDownloadSource.download_plan(work_id).await
    }

    async fn download_file_metadata(
        &self,
        file_index: usize,
        file: &DownloadFile,
    ) -> Result<DownloadFileMetadata> {
        FakeDownloadSource
            .download_file_metadata(file_index, file)
            .await
    }

    async fn download_files(
        &self,
        job: &DownloadJobRequest,
        plan: &DownloadPlan,
        cancellation: &dldr_download::CancellationToken,
        progress_sink: &mut (dyn FnMut(DownloadProgress) + Send),
    ) -> Result<DownloadedWork> {
        if job.work_id.as_ref() == self.fail_work_id {
            return Err((self.failure)());
        }

        FakeDownloadSource
            .download_files(job, plan, cancellation, progress_sink)
            .await
    }
}

#[derive(Debug, Default)]
struct RecordingProgressSink {
    events: Mutex<Vec<SyncProgress>>,
}

impl SyncProgressSink for RecordingProgressSink {
    fn emit(&self, progress: SyncProgress) {
        self.events.lock().expect("events lock").push(progress);
    }
}

#[derive(Debug, Default)]
struct RecordingDownloadProgressSink {
    events: Mutex<Vec<WorkDownloadProgress>>,
}

impl WorkDownloadProgressSink for RecordingDownloadProgressSink {
    fn emit(&self, progress: WorkDownloadProgress) {
        self.events.lock().expect("events lock").push(progress);
    }
}

#[derive(Debug, Default)]
struct RecordingBulkDownloadProgressSink {
    events: Mutex<Vec<BulkWorkDownloadProgress>>,
}

impl BulkWorkDownloadProgressSink for RecordingBulkDownloadProgressSink {
    fn emit(&self, progress: BulkWorkDownloadProgress) {
        self.events.lock().expect("events lock").push(progress);
    }
}

#[derive(Debug, Default)]
struct RecordingBulkDownloadPreviewProgressSink {
    events: Mutex<Vec<BulkWorkDownloadPreviewProgress>>,
}

impl BulkWorkDownloadPreviewProgressSink for RecordingBulkDownloadPreviewProgressSink {
    fn emit(&self, progress: BulkWorkDownloadPreviewProgress) {
        self.events.lock().expect("events lock").push(progress);
    }
}

async fn migrated_library() -> Result<Library> {
    let storage = Storage::open_in_memory().await?;
    storage.run_migrations().await?;

    Ok(Library::new(
        storage,
        Arc::new(InMemoryCredentialStore::new()),
    ))
}

fn save_account_request(remember_password: bool) -> SaveAccountRequest {
    SaveAccountRequest {
        id: Some("account-a".to_owned()),
        label: "Account A".to_owned(),
        login_name: Some("user@example.test".to_owned()),
        password: Some("secret".to_owned()),
        remember_password,
        enabled: true,
    }
}

fn purchase(work_id: &str, purchased_at: &str) -> Purchase {
    Purchase {
        id: WorkId::new(work_id),
        purchased_at: DateTime::parse_from_rfc3339(purchased_at)
            .expect("date")
            .with_timezone(&Utc),
    }
}

fn localized(value: &str) -> LocalizedText {
    BTreeMap::from([(Language::Japanese, value.to_owned())])
}

fn work(work_id: &str, title: &str, maker: &str, published_at: &str) -> Work {
    let published_at = DateTime::parse_from_rfc3339(published_at)
        .expect("date")
        .with_timezone(&Utc);

    Work {
        id: WorkId::new(work_id),
        name: localized(title),
        maker: Maker {
            id: format!("maker-{maker}"),
            name: localized(maker),
        },
        work_kind: WorkKind {
            code: "SOU".to_owned(),
        },
        age_category: AgeCategory::All,
        genre_ids: vec![1, 2],
        thumbnail: WorkThumbnail {
            full: Url::parse(&format!("https://img.example.test/{work_id}/full.jpg")).expect("url"),
            small_square: Url::parse(&format!("https://img.example.test/{work_id}/small.jpg"))
                .expect("url"),
        },
        registered_at: Some(published_at),
        published_at: Some(published_at),
        updated_at: Some(published_at),
        tags: Vec::new(),
        content_size: Some(10),
        extra: BTreeMap::new(),
    }
}

fn public_work(work_id: &str, title: &str, maker: &str) -> PublicWork {
    PublicWork {
        id: WorkId::new(work_id),
        name: Some(title.to_owned()),
        maker_id: Some(format!("maker-{maker}")),
        maker_name: Some(maker.to_owned()),
        work_kind: Some("SOU".to_owned()),
        work_kind_label: Some("ボイス・ASMR".to_owned()),
        age_category: Some(json!(3)),
        age_category_label: Some("adult".to_owned()),
        image_main: Some(PublicWorkImage::Object(PublicWorkImageObject {
            url: Some(format!("//img.example.test/{work_id}/main.jpg")),
            relative_url: None,
        })),
        image_thumb: Some(format!("//img.example.test/{work_id}/main_240x240.jpg")),
        registered_at: Some("2026-04-24 00:00:00".to_owned()),
        published_at: None,
        updated_at: Some("2026-04-25 09:09:14".to_owned()),
        content_size: Some(json!(2048)),
        extra: BTreeMap::from([
            (
                "creaters".to_owned(),
                json!({
                    "voice_by": [
                        {
                            "name": "Voice One",
                            "classification": "voice_by"
                        }
                    ]
                }),
            ),
            (
                "genres".to_owned(),
                json!([
                    {
                        "name": "ASMR",
                        "id": 497
                    }
                ]),
            ),
        ]),
    }
}

const VALID_TWO_FACTOR_CODE: &str = "123456";
const TWO_FACTOR_SESSION: &str = "{\"two-factor\":true}";

/// Sync source for a two-factor-enabled account: the password alone only ever yields a
/// challenge, and a session becomes valid once it has been issued by a correct code.
#[derive(Clone)]
struct TwoFactorSyncSource {
    inner: FakeSyncSource,
    state: Arc<Mutex<TwoFactorSourceState>>,
}

#[derive(Default)]
struct TwoFactorSourceState {
    issued_session: Option<String>,
    restored_session: Option<String>,
    submitted_codes: Vec<String>,
    login_count: u32,
}

impl TwoFactorSyncSource {
    fn new() -> Self {
        Self {
            inner: sync_source(),
            state: Arc::new(Mutex::new(TwoFactorSourceState::default())),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, TwoFactorSourceState> {
        self.state.lock().expect("two-factor state lock")
    }

    fn challenge() -> TwoFactorChallenge {
        TwoFactorChallenge {
            page_url: Url::parse("https://login.dlsite.com/login/two-factor").unwrap(),
            action: Url::parse("https://login.dlsite.com/login/two-factor?user=self").unwrap(),
            code_field: "otp".to_owned(),
            fields: vec![("_token".to_owned(), "token".to_owned())],
        }
    }
}

#[async_trait]
impl DlsiteAuthSource for TwoFactorSyncSource {
    async fn restore_session(&self, session: &SessionSnapshot) -> Result<()> {
        self.state().restored_session = Some(session.cookies_json.clone());
        Ok(())
    }

    async fn validate_session(&self) -> Result<bool> {
        let state = self.state();
        Ok(state.restored_session.is_some() && state.restored_session == state.issued_session)
    }

    async fn begin_login(&self, credentials: &Credentials) -> Result<LoginOutcome> {
        assert_fake_credentials(credentials);
        self.state().login_count += 1;
        Ok(LoginOutcome::TwoFactorRequired(Self::challenge()))
    }

    async fn complete_two_factor(
        &self,
        challenge: &TwoFactorChallenge,
        code: &str,
    ) -> Result<SessionSnapshot> {
        assert_eq!(challenge.code_field, "otp");

        let mut state = self.state();
        state.submitted_codes.push(code.to_owned());

        if code != VALID_TWO_FACTOR_CODE {
            return Err(LibraryError::Api(ApiError::InvalidTwoFactorCode));
        }

        state.issued_session = Some(TWO_FACTOR_SESSION.to_owned());

        Ok(SessionSnapshot {
            cookies_json: TWO_FACTOR_SESSION.to_owned(),
        })
    }
}

#[async_trait]
impl WorkDownloadSource for TwoFactorSyncSource {
    async fn download_plan(&self, work_id: &WorkId) -> Result<DownloadPlan> {
        FakeDownloadSource.download_plan(work_id).await
    }

    async fn download_file_metadata(
        &self,
        index: usize,
        file: &DownloadFile,
    ) -> Result<DownloadFileMetadata> {
        FakeDownloadSource.download_file_metadata(index, file).await
    }

    async fn download_files(
        &self,
        job: &DownloadJobRequest,
        plan: &DownloadPlan,
        cancellation: &dldr_download::CancellationToken,
        sink: &mut (dyn FnMut(DownloadProgress) + Send),
    ) -> Result<DownloadedWork> {
        FakeDownloadSource
            .download_files(job, plan, cancellation, sink)
            .await
    }
}

#[async_trait]
impl AccountSyncSource for TwoFactorSyncSource {
    async fn content_count(&self) -> Result<ContentCount> {
        self.inner.content_count().await
    }

    async fn purchases(&self) -> Result<Vec<Purchase>> {
        self.inner.purchases().await
    }

    async fn works(&self, ids: &[WorkId]) -> Result<Vec<Work>> {
        self.inner.works(ids).await
    }
}

/// Prompt that hands back a scripted sequence of answers and records what it was asked.
#[derive(Clone, Default)]
struct ScriptedTwoFactorPrompt {
    answers: Arc<Mutex<Vec<TwoFactorPromptResponse>>>,
    requests: Arc<Mutex<Vec<TwoFactorPromptRequest>>>,
}

impl ScriptedTwoFactorPrompt {
    fn new(answers: impl IntoIterator<Item = &'static str>) -> Self {
        Self::with_responses(
            answers
                .into_iter()
                .map(|code| TwoFactorPromptResponse::Code(code.to_owned())),
        )
    }

    fn with_responses(answers: impl IntoIterator<Item = TwoFactorPromptResponse>) -> Self {
        Self {
            answers: Arc::new(Mutex::new(answers.into_iter().collect())),
            requests: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn requests(&self) -> Vec<TwoFactorPromptRequest> {
        self.requests.lock().expect("prompt lock").clone()
    }
}

#[async_trait]
impl TwoFactorPrompt for ScriptedTwoFactorPrompt {
    async fn request_code(
        &self,
        request: TwoFactorPromptRequest,
    ) -> Result<TwoFactorPromptResponse> {
        self.requests.lock().expect("prompt lock").push(request);

        let mut answers = self.answers.lock().expect("prompt lock");

        assert!(
            !answers.is_empty(),
            "prompt was asked more often than scripted"
        );

        Ok(answers.remove(0))
    }
}

fn sync_source() -> FakeSyncSource {
    FakeSyncSource {
        content_count: ContentCount {
            user: 2,
            production: 0,
            page_limit: Some(50),
            concurrency: Some(500),
        },
        purchases: vec![
            purchase("RJ000001", "2026-01-01T00:00:00Z"),
            purchase("RJ000002", "2026-01-02T00:00:00Z"),
        ],
        works: vec![
            work(
                "RJ000001",
                "First Work",
                "Maker One",
                "2025-01-01T00:00:00Z",
            ),
            work(
                "RJ000002",
                "Second Work",
                "Maker Two",
                "2025-01-02T00:00:00Z",
            ),
        ],
        fail_at: None,
    }
}

fn test_dir(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "dldr-library-{name}-{}-{unique}",
        std::process::id()
    ));

    std::fs::create_dir_all(&dir).unwrap();
    dir
}

mod work_ids;

mod auth;

mod sync;

mod downloads;

mod bulk;
