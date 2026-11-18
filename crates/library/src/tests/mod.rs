mod diagnostics;
use super::*;
mod language_priority;
mod maintenance;
mod public_credits;
use dldr_api::{
    AgeCategory, DownloadFile, DownloadFileKind, DownloadPlan, DownloadStreamRequest, Language,
    LocalizedText, Maker, PublicWorkImage, PublicWorkImageObject, WorkKind, WorkThumbnail,
};
use dldr_credentials::InMemoryCredentialStore;
use dldr_download::{DownloadPhase, DownloadedFile};
use dldr_storage::{ProductSort, SyncRunStatus, WorkDownloadStatus};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use url::Url;

#[test]
fn cached_names_preserve_unknown_languages_and_unlabeled_sources() -> Result<()> {
    let mut source = work("RJ000001", "作品", "制作元", "2026-01-01T00:00:00Z");
    source
        .name
        .insert(Language::Unknown("pt_BR".into()), "Título".into());
    source.maker.name.insert(Language::Korean, "제작자".into());
    let cached = cached_work_from_api(source, "2026-10-19")?;
    let names: Value = serde_json::from_str(&cached.title_json)?;
    assert_eq!(names["ja_JP"], "作品");
    assert_eq!(names["pt_BR"], "Título");
    let makers: Value = serde_json::from_str(cached.maker_json.as_deref().unwrap())?;
    assert_eq!(makers["ko_KR"], "제작자");
    let public =
        cached_work_from_public_api(public_work("RJ000002", "Public", "Maker"), "2026-10-19")?;
    assert_eq!(public.title_json, r#"{"und":"Public"}"#);
    assert_eq!(public.maker_json.as_deref(), Some(r#"{"und":"Maker"}"#));
    let local = cached_work_from_local_folder("RJ000003", "Local folder", "2026-10-19")?;
    assert_eq!(local.title_json, r#"{"und":"Local folder"}"#);
    Ok(())
}

#[test]
fn support_details_preserve_safe_api_failure_context() {
    let error = LibraryError::Api(ApiError::DownloadUnknownRedirect {
        work_id: WorkId::from("RJ123456"),
        location: Url::parse("https://download.dlsite.com/get/=/file/RJ123456.zip?token=secret")
            .unwrap(),
    });

    let details = error.support_details();

    assert_eq!(error.failure_code(), "api");
    assert!(error
        .support_message()
        .contains("unknown download redirect for RJ123456"));
    assert_eq!(details["failureKind"], "api");
    assert_eq!(details["apiErrorKind"], "download_unknown_redirect");
    assert_eq!(details["workId"], "RJ123456");
    assert_eq!(
        details["location"],
        "https://download.dlsite.com/<download-url-redacted>"
    );
    assert!(!details.to_string().contains("secret"));
}

#[test]
fn support_details_preserve_download_api_failure_context() {
    let error = LibraryError::Download(dldr_download::DownloadError::Api(ApiError::NotAuthorized));

    let details = error.support_details();

    assert_eq!(error.failure_code(), "download");
    assert!(error
        .support_message()
        .contains("DLsite session is not authorized"));
    assert_eq!(details["failureKind"], "api");
    assert_eq!(details["downloadErrorKind"], "api");
    assert_eq!(details["apiErrorKind"], "not_authorized");
}

#[test]
fn support_body_snippets_redact_urls_and_serial_material() {
    let html = r#"<a href="https://download.dlsite.com/get/secret?token=value">download</a>"#;

    let snippet = safe_body_snippet_for_log(html).unwrap();

    assert!(snippet.contains("[redacted-url]"));
    assert!(!snippet.contains("download.dlsite.com"));
    assert!(!snippet.contains("token=value"));

    let serial_snippet = safe_body_snippet_for_log("serial number: ABCD-1234").unwrap();
    assert_eq!(
        serial_snippet,
        "[redacted because response may contain serial information]"
    );
}

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

#[test]
fn detects_work_ids_in_flexible_folder_names() {
    assert_eq!(
        detect_work_ids_in_text("[RJ01005844] Soothing Voice")
            .into_iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>(),
        vec!["RJ01005844"]
    );
    assert_eq!(
        detect_work_ids_in_text("title rj123456 extra")
            .into_iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>(),
        vec!["RJ123456"]
    );
    assert_eq!(
        detect_work_ids_in_text("commercial VJ01001165 and book BJ123456")
            .into_iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>(),
        vec!["VJ01001165", "BJ123456"]
    );
}

#[test]
fn work_id_detection_requires_boundaries_and_known_lengths() {
    for value in [
        "XRJ123456",
        "RJ123456A",
        "RJ12345",
        "RJ123456789",
        "AB123456",
    ] {
        assert_eq!(detect_work_ids_in_text(value), Vec::<WorkId>::new());
    }
}

#[test]
fn work_id_detection_deduplicates_in_first_seen_order() {
    assert_eq!(
        detect_work_ids_in_text("RJ000001 / rj000001 / VJ000001")
            .into_iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>(),
        vec!["RJ000001", "VJ000001"]
    );
}

#[test]
fn detects_unique_work_id_from_path_file_name() {
    let path = Path::new("/library/[RJ01005844] Soothing Voice");

    assert_eq!(
        detect_unique_work_id_in_path(path).map(|id| id.to_string()),
        Some("RJ01005844".to_owned())
    );
    assert_eq!(
        detect_work_ids_in_path("/library/no-id"),
        Vec::<WorkId>::new()
    );
    assert_eq!(
        detect_unique_work_id_in_path("/library/RJ000001 VJ000001"),
        None
    );
}

#[tokio::test]
async fn saves_account_and_password_reference() -> Result<()> {
    let library = migrated_library().await?;

    let account = library.save_account(save_account_request(true)).await?;

    assert_eq!(account.id, "account-a");
    assert_eq!(
        account.credential_ref,
        Some("account:account-a:password".to_owned())
    );
    assert!(library.account_has_saved_password(&account)?);

    Ok(())
}

#[tokio::test]
async fn save_account_preserves_enabled_state_for_existing_accounts() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;
    library.set_account_enabled("account-a", false).await?;

    let account = library.save_account(save_account_request(true)).await?;
    assert!(
        !account.enabled,
        "saving details must not re-enable an account"
    );

    library.set_account_enabled("account-a", true).await?;
    let mut request = save_account_request(true);
    request.enabled = false;
    let account = library.save_account(request).await?;
    assert!(
        account.enabled,
        "only the dedicated toggle changes existing state"
    );
    Ok(())
}

#[tokio::test]
async fn save_account_uses_initial_enabled_state_for_new_accounts() -> Result<()> {
    let library = migrated_library().await?;
    let account = library
        .save_account(SaveAccountRequest::new("New account"))
        .await?;
    assert!(account.enabled);

    let mut request = SaveAccountRequest::new("Initially disabled");
    request.enabled = false;
    assert!(!library.save_account(request).await?.enabled);
    Ok(())
}

#[tokio::test]
async fn save_account_queued_after_disable_during_auth_keeps_account_disabled() -> Result<()> {
    use std::future::Future;
    use std::task::Poll;

    #[derive(Default)]
    struct GatedPrompt {
        entered: tokio::sync::Notify,
        release: tokio::sync::Notify,
    }

    #[async_trait]
    impl TwoFactorPrompt for GatedPrompt {
        async fn request_code(&self, _: TwoFactorPromptRequest) -> Result<TwoFactorPromptResponse> {
            self.entered.notify_one();
            self.release.notified().await;
            Ok(TwoFactorPromptResponse::Code(
                VALID_TWO_FACTOR_CODE.to_owned(),
            ))
        }
    }

    tokio::time::timeout(Duration::from_secs(2), async {
        let library = migrated_library().await?;
        let account = library.save_account(save_account_request(true)).await?;
        let source = TwoFactorSyncSource::new();
        let prompt = GatedPrompt::default();
        let mut authentication = Box::pin(library.authenticate_account(
            &account,
            None,
            Some(&prompt),
            None,
            &source,
        ));
        tokio::select! {
            result = &mut authentication => panic!("authentication finished before gate: {result:?}"),
            () = prompt.entered.notified() => {}
        }

        // Poll each mutation once while auth holds the lock, establishing FIFO order
        // without sleeps or assumptions about task scheduling.
        let mut disable = Box::pin(library.set_account_enabled(&account.id, false));
        std::future::poll_fn(|cx| {
            assert!(disable.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;
        let mut request = save_account_request(true);
        request.password = None;
        request.label = "Updated label".to_owned();
        assert!(request.enabled); // The adapter's creation default, not current state.
        let mut save = Box::pin(library.save_account(request));
        std::future::poll_fn(|cx| {
            assert!(save.as_mut().poll(cx).is_pending());
            Poll::Ready(())
        })
        .await;

        prompt.release.notify_one();
        authentication.await?;
        let (disabled, saved) = tokio::join!(disable, save);
        disabled?;
        let saved = saved?;
        assert_eq!(saved.label, "Updated label");
        assert!(!saved.enabled, "queued save undid the completed disable");
        assert!(!library.find_account(&account.id).await?.enabled);
        Ok(())
    })
    .await
    .expect("gated account mutations must finish")
}

#[tokio::test]
async fn removes_account_and_saved_credential() -> Result<()> {
    let storage = Storage::open_in_memory().await?;
    storage.run_migrations().await?;
    let credentials = Arc::new(InMemoryCredentialStore::new());
    let library = Library::new(storage, credentials.clone());

    let account = library.save_account(save_account_request(true)).await?;
    let credential_ref = CredentialRef::account_password(&account.id)?;

    assert!(credentials.load_password(&credential_ref)?.is_some());

    let report = library.remove_account(&account.id).await?;

    assert_eq!(report.account_id, "account-a");
    assert_eq!(report.label, "Account A");
    assert!(report.credential_deleted);
    assert!(credentials.load_password(&credential_ref)?.is_none());
    assert_eq!(library.accounts().await?, Vec::new());

    Ok(())
}

#[tokio::test]
async fn syncs_account_into_unified_product_cache() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;
    let sink = RecordingProgressSink::default();

    let report = library
        .sync_account_with_source(
            AccountSyncRequest {
                account_id: "account-a",
                password: None,
                cancellation_token: None,
                progress_sink: Some(&sink),
                two_factor_prompt: None,
            },
            &sync_source(),
        )
        .await?;
    let page = library
        .list_products(&ProductListQuery {
            sort: ProductSort::TitleAsc,
            ..ProductListQuery::default()
        })
        .await?;
    let sync_runs = library.storage().sync_runs_for_account("account-a").await?;
    let events = sink.events.lock().expect("events lock");

    assert_eq!(report.purchased_count, 2);
    assert_eq!(report.cached_work_count, 2);
    assert_eq!(report.page_limit, Some(50));
    assert_eq!(page.total_count, 2);
    assert_eq!(page.products[0].work_id, "RJ000001");
    assert_eq!(
        page.products[0].thumbnail_url,
        Some("https://img.example.test/RJ000001/full.jpg".to_owned())
    );
    assert_eq!(page.products[0].owners[0].account_id, "account-a");
    assert_eq!(sync_runs.len(), 1);
    assert_eq!(sync_runs[0].status, SyncRunStatus::Completed);
    assert!(matches!(events.first(), Some(SyncProgress::LoggingIn)));
    assert!(matches!(
        events.last(),
        Some(SyncProgress::Completed {
            cached_work_count: 2,
            ..
        })
    ));

    Ok(())
}

#[tokio::test]
async fn downloads_owned_work_and_records_local_path() -> Result<()> {
    let library = migrated_library().await?;
    let root = test_dir("download-owned-work");
    let library_root = root.join("library");
    let download_root = root.join("downloads");
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;
    let sink = RecordingDownloadProgressSink::default();

    let report = library
        .download_work_with_source(
            WorkDownloadRequest {
                progress_sink: Some(&sink),
                ..WorkDownloadRequest::new("RJ000001", &library_root, &download_root)
            },
            &FakeDownloadSource,
        )
        .await?;
    let page = library.list_products(&ProductListQuery::default()).await?;
    let product = page
        .products
        .iter()
        .find(|product| product.work_id == "RJ000001")
        .expect("downloaded work in product list");
    let events = sink.events.lock().expect("download events lock");

    assert_eq!(report.work_id, "RJ000001");
    assert_eq!(report.account_id, "account-a");
    assert_eq!(report.file_count, 1);
    assert_eq!(report.download_state.status, WorkDownloadStatus::Downloaded);
    assert!(library_root.join("RJ000001/RJ000001.txt").exists());
    assert!(!library_root
        .join(format!("RJ000001/{SERIAL_INFORMATION_FILE_NAME}"))
        .exists());
    assert!(!download_root.join("RJ000001").exists());
    assert_eq!(product.download.status, WorkDownloadStatus::Downloaded);
    assert_eq!(
        product.download.local_path,
        Some(
            library_root
                .join("RJ000001")
                .canonicalize()?
                .to_string_lossy()
                .into_owned()
        )
    );
    assert!(matches!(
        events.first(),
        Some(WorkDownloadProgress::LoggingIn)
    ));
    assert!(matches!(
        events.last(),
        Some(WorkDownloadProgress::Completed)
    ));

    std::fs::remove_dir_all(root).unwrap();

    Ok(())
}

#[tokio::test]
async fn downloads_serial_work_and_writes_serial_information_file() -> Result<()> {
    let library = migrated_library().await?;
    let root = test_dir("download-serial-work");
    let library_root = root.join("library");
    let download_root = root.join("downloads");
    let serial_path = library_root
        .join("RJ000001")
        .join(SERIAL_INFORMATION_FILE_NAME);
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;

    let report = library
        .download_work_with_source(
            WorkDownloadRequest::new("RJ000001", &library_root, &download_root),
            &SerialDownloadSource,
        )
        .await?;
    let serial_content = tokio::fs::read_to_string(&serial_path).await?;

    assert_eq!(report.download_state.status, WorkDownloadStatus::Downloaded);
    assert!(serial_content.starts_with(SERIAL_INFORMATION_MARKER));
    assert!(serial_content.contains("Work ID: RJ000001"));
    assert!(serial_content.contains("Serial number"));
    assert!(serial_content.contains("ABC-123-SECRET"));
    assert!(!download_root.join("RJ000001").exists());

    std::fs::remove_dir_all(root).unwrap();

    Ok(())
}

#[tokio::test]
async fn download_cancellation_reaches_download_source_without_progress() -> Result<()> {
    let library = migrated_library().await?;
    let root = test_dir("download-cancellation-forwarding");
    let library_root = root.join("library");
    let download_root = root.join("downloads");
    let source = WaitingDownloadSource::new();
    let cancellation = CancellationToken::new();
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;

    let result = tokio::time::timeout(Duration::from_secs(2), async {
        let download = library.download_work_with_source(
            WorkDownloadRequest {
                cancellation_token: Some(&cancellation),
                ..WorkDownloadRequest::new("RJ000001", &library_root, &download_root)
            },
            &source,
        );
        let cancel = async {
            while !source.entered_download() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }

            cancellation.cancel();
        };
        let (result, ()) = tokio::join!(download, cancel);
        result
    })
    .await
    .expect("download cancellation should complete promptly");
    let err = match result {
        Ok(_) => panic!("download should have been cancelled"),
        Err(error) => error,
    };

    assert!(matches!(
        err,
        LibraryError::Cancelled | LibraryError::Download(dldr_download::DownloadError::Cancelled)
    ));

    std::fs::remove_dir_all(root).unwrap();

    Ok(())
}

#[tokio::test]
async fn removes_downloaded_work_and_clears_state() -> Result<()> {
    let library = migrated_library().await?;
    let root = test_dir("remove-downloaded-work");
    let library_root = root.join("library");
    let download_root = root.join("downloads");
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;
    library
        .download_work_with_source(
            WorkDownloadRequest::new("RJ000001", &library_root, &download_root),
            &FakeDownloadSource,
        )
        .await?;

    let state = library
        .remove_work_download(WorkDownloadRemovalRequest::new(
            "RJ000001",
            &library_root,
            &download_root,
        ))
        .await?;

    assert_eq!(state.status, WorkDownloadStatus::NotDownloaded);
    assert!(!library_root.join("RJ000001").exists());
    assert!(!download_root.join("RJ000001").exists());

    std::fs::remove_dir_all(root).unwrap();

    Ok(())
}

#[tokio::test]
async fn marks_existing_library_folder_as_downloaded() -> Result<()> {
    let library = migrated_library().await?;
    let root = test_dir("mark-downloaded-work");
    let library_root = root.join("library");
    let local_path = library_root.join("manual").join("RJ000001");
    std::fs::create_dir_all(&local_path).unwrap();
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;

    let state = library
        .mark_work_downloaded(WorkDownloadMarkRequest::new(
            "RJ000001",
            &library_root,
            &local_path,
        ))
        .await?;

    assert_eq!(state.status, WorkDownloadStatus::Downloaded);
    assert_eq!(
        state.local_path,
        Some(
            local_path
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        )
    );
    assert_eq!(state.staging_path, None);
    assert_eq!(state.unpack_policy, Some("manual".to_owned()));

    let page = library.list_products(&ProductListQuery::default()).await?;
    let product = page
        .products
        .iter()
        .find(|product| product.work_id == "RJ000001")
        .expect("marked work in product list");
    assert_eq!(product.download.status, WorkDownloadStatus::Downloaded);

    std::fs::remove_dir_all(root).unwrap();

    Ok(())
}

#[tokio::test]
async fn rejects_manual_download_folder_outside_library_root() -> Result<()> {
    let library = migrated_library().await?;
    let root = test_dir("mark-downloaded-outside-root");
    let library_root = root.join("library");
    let outside_path = root.join("outside").join("RJ000001");
    std::fs::create_dir_all(&library_root).unwrap();
    std::fs::create_dir_all(&outside_path).unwrap();
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;

    let err = library
        .mark_work_downloaded(WorkDownloadMarkRequest::new(
            "RJ000001",
            &library_root,
            &outside_path,
        ))
        .await
        .unwrap_err();

    assert!(matches!(err, LibraryError::DownloadPathOutsideRoots(_)));

    std::fs::remove_dir_all(root).unwrap();

    Ok(())
}

#[tokio::test]
async fn imports_local_work_folders_as_downloaded_products() -> Result<()> {
    let library = migrated_library().await?;
    let root = test_dir("import-local-work-folders");
    let library_root = root.join("library");
    let local_path = library_root.join("[RJ123456] Local Work");
    std::fs::create_dir_all(&local_path).unwrap();
    std::fs::create_dir_all(library_root.join("No Work Id")).unwrap();
    std::fs::create_dir_all(library_root.join("RJ000001 VJ000001 ambiguous")).unwrap();

    let report = library
        .import_local_work_downloads(LocalWorkImportRequest::new(&library_root))
        .await?;
    let page = library.list_products(&ProductListQuery::default()).await?;

    assert_eq!(report.scanned_directories, 3);
    assert_eq!(report.imported_count, 1);
    assert_eq!(report.skipped_no_id, 1);
    assert_eq!(report.skipped_ambiguous, 1);
    assert_eq!(report.skipped_existing, 0);
    assert_eq!(report.imported_works[0].work_id, "RJ123456");
    assert_eq!(
        report.imported_works[0].local_path,
        local_path.canonicalize().unwrap()
    );

    assert_eq!(page.total_count, 1);
    assert_eq!(page.products[0].work_id, "RJ123456");
    assert_eq!(page.products[0].title, "[RJ123456] Local Work");
    assert_eq!(page.products[0].owners[0].label, "Local");
    assert_eq!(
        page.products[0].download.status,
        WorkDownloadStatus::Downloaded
    );
    assert_eq!(
        page.products[0].download.local_path,
        Some(
            local_path
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        )
    );

    std::fs::remove_dir_all(root).unwrap();

    Ok(())
}

#[tokio::test]
async fn imports_local_work_folders_with_public_metadata() -> Result<()> {
    let library = migrated_library().await?;
    let root = test_dir("import-local-work-folders-metadata");
    let library_root = root.join("library");
    let local_path = library_root.join("[RJ123456] Local Work");
    let source = FakeLocalWorkMetadataSource {
        works: vec![public_work(
            "RJ123456",
            "Public Metadata Work",
            "Public Maker",
        )],
        fail: false,
    };
    std::fs::create_dir_all(&local_path).unwrap();

    let report = library
        .import_local_work_downloads_with_metadata_source(
            LocalWorkImportRequest::new(&library_root),
            &source,
        )
        .await?;
    let page = library.list_products(&ProductListQuery::default()).await?;

    assert_eq!(report.imported_count, 1);
    assert_eq!(report.metadata_candidate_count, 1);
    assert_eq!(report.metadata_updated_count, 1);
    assert_eq!(report.metadata_missing_count, 0);
    assert_eq!(report.metadata_error, None);
    assert_eq!(page.products[0].work_id, "RJ123456");
    assert_eq!(page.products[0].title, "Public Metadata Work");
    assert_eq!(page.products[0].maker_name.as_deref(), Some("Public Maker"));
    assert_eq!(page.products[0].work_type.as_deref(), Some("SOU"));
    assert_eq!(page.products[0].age_category.as_deref(), Some("r18"));
    assert_eq!(
        page.products[0].thumbnail_url.as_deref(),
        Some("https://img.example.test/RJ123456/main.jpg")
    );
    assert_eq!(page.products[0].content_size_bytes, Some(2048));
    assert_eq!(page.products[0].credit_groups[0].names, vec!["Voice One"]);
    assert_eq!(
        page.products[0].download.local_path,
        Some(
            local_path
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        )
    );

    std::fs::remove_dir_all(root).unwrap();

    Ok(())
}

#[tokio::test]
async fn local_work_import_hydrates_existing_placeholder_metadata() -> Result<()> {
    let library = migrated_library().await?;
    let root = test_dir("import-local-work-existing-metadata");
    let library_root = root.join("library");
    let local_path = library_root.join("[RJ123456] Local Work");
    std::fs::create_dir_all(&local_path).unwrap();

    library
        .import_local_work_downloads(LocalWorkImportRequest::new(&library_root))
        .await?;

    let source = FakeLocalWorkMetadataSource {
        works: vec![public_work(
            "RJ123456",
            "Public Metadata Work",
            "Public Maker",
        )],
        fail: false,
    };
    let report = library
        .import_local_work_downloads_with_metadata_source(
            LocalWorkImportRequest::new(&library_root),
            &source,
        )
        .await?;
    let page = library.list_products(&ProductListQuery::default()).await?;

    assert_eq!(report.imported_count, 0);
    assert_eq!(report.skipped_existing, 1);
    assert_eq!(report.metadata_candidate_count, 1);
    assert_eq!(report.metadata_updated_count, 1);
    assert_eq!(page.products[0].title, "Public Metadata Work");
    assert_eq!(page.products[0].maker_name.as_deref(), Some("Public Maker"));

    std::fs::remove_dir_all(root).unwrap();

    Ok(())
}

#[tokio::test]
async fn local_work_import_skips_existing_download_records() -> Result<()> {
    let library = migrated_library().await?;
    let root = test_dir("import-local-skips-existing");
    let library_root = root.join("library");
    let local_path = library_root.join("[RJ000001] Local Duplicate");
    std::fs::create_dir_all(&local_path).unwrap();
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;
    library
        .mark_work_downloaded(WorkDownloadMarkRequest::new(
            "RJ000001",
            &library_root,
            &local_path,
        ))
        .await?;

    let report = library
        .import_local_work_downloads(LocalWorkImportRequest::new(&library_root))
        .await?;

    assert_eq!(report.scanned_directories, 1);
    assert_eq!(report.imported_count, 0);
    assert_eq!(report.skipped_existing, 1);
    assert!(report.imported_works.is_empty());

    std::fs::remove_dir_all(root).unwrap();

    Ok(())
}

#[tokio::test]
async fn redownload_replaces_existing_local_work_after_staging() -> Result<()> {
    let library = migrated_library().await?;
    let root = test_dir("redownload-replaces-work");
    let library_root = root.join("library");
    let download_root = root.join("downloads");
    let local_file = library_root.join("RJ000001/RJ000001.txt");
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;
    library
        .download_work_with_source(
            WorkDownloadRequest::new("RJ000001", &library_root, &download_root),
            &FakeDownloadSource,
        )
        .await?;
    tokio::fs::write(&local_file, b"user edit").await?;

    let report = library
        .download_work_with_source(
            WorkDownloadRequest {
                replace_existing: true,
                ..WorkDownloadRequest::new("RJ000001", &library_root, &download_root)
            },
            &FakeDownloadSource,
        )
        .await?;

    assert_eq!(report.download_state.status, WorkDownloadStatus::Downloaded);
    assert_eq!(tokio::fs::read(&local_file).await?, b"downloaded");
    assert!(!download_root.join("RJ000001").exists());

    std::fs::remove_dir_all(root).unwrap();

    Ok(())
}

#[tokio::test]
async fn selected_bulk_download_never_includes_unselected_works() -> Result<()> {
    let library = migrated_library().await?;
    let root = test_dir("selected-bulk-download");
    let library_root = root.join("library");
    let download_root = root.join("downloads");
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;
    let ids = vec!["RJ000002".to_owned()];
    let candidates = library
        .select_bulk_candidates(&ProductListQuery::default(), true, Some(&ids))
        .await?;
    assert_eq!(candidates.work_ids, ids);
    assert_eq!(candidates.total_count, 1);
    let preview = library
        .preview_download_products_with_source(
            BulkWorkDownloadPreviewRequest {
                query: ProductListQuery::default(),
                work_ids: Some(ids.clone()),
                skip_downloaded: true,
                cancellation_token: None,
                progress_sink: None,
            },
            &FakeDownloadSource,
        )
        .await?;
    assert_eq!(preview.requested_count, 1);
    assert_eq!(preview.works[0].work_id, "RJ000002");
    let report = library
        .download_products_with_source(
            BulkWorkDownloadRequest {
                work_ids: Some(ids),
                ..BulkWorkDownloadRequest::new(
                    ProductListQuery::default(),
                    &library_root,
                    &download_root,
                )
            },
            &FakeDownloadSource,
        )
        .await?;
    assert_eq!(report.succeeded_count, 1);
    assert_eq!(report.succeeded_works[0].work_id, "RJ000002");
    assert!(!library_root.join("RJ000001").exists());

    // An unrelated downloaded work must not inflate the selected skip count.
    let candidates = library
        .select_bulk_candidates(
            &ProductListQuery::default(),
            true,
            Some(&["RJ000001".into()]),
        )
        .await?;
    assert_eq!(candidates.skipped_downloaded_count, 0);
    assert_eq!(candidates.work_ids, vec!["RJ000001"]);
    let empty = library
        .select_bulk_candidates(&ProductListQuery::default(), true, Some(&[]))
        .await?;
    assert!(empty.work_ids.is_empty());
    assert_eq!(empty.total_count, 0);
    assert_eq!(empty.skipped_downloaded_count, 0);
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn bulk_download_skips_downloaded_works() -> Result<()> {
    let library = migrated_library().await?;
    let root = test_dir("bulk-download-skips");
    let library_root = root.join("library");
    let download_root = root.join("downloads");
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;
    library
        .download_work_with_source(
            WorkDownloadRequest::new("RJ000001", &library_root, &download_root),
            &FakeDownloadSource,
        )
        .await?;
    let sink = RecordingBulkDownloadProgressSink::default();

    let report = library
        .download_products_with_source(
            BulkWorkDownloadRequest {
                progress_sink: Some(&sink),
                ..BulkWorkDownloadRequest::new(
                    ProductListQuery::default(),
                    &library_root,
                    &download_root,
                )
            },
            &FakeDownloadSource,
        )
        .await?;
    let page = library.list_products(&ProductListQuery::default()).await?;
    let events = sink.events.lock().expect("bulk events lock");

    assert_eq!(report.total_count, 2);
    assert_eq!(report.requested_count, 1);
    assert_eq!(report.skipped_downloaded_count, 1);
    assert_eq!(report.succeeded_count, 1);
    assert_eq!(report.failed_count, 0);
    assert!(events.iter().any(|event| matches!(event,
        BulkWorkDownloadProgress::Planned { works }
            if works.len() == 1 && works[0].work_id == "RJ000002" && !works[0].title.is_empty()
    )));
    assert!(events.iter().any(|event| matches!(event,
        BulkWorkDownloadProgress::WorkProgress { work_id, progress: WorkDownloadProgress::Download(p) }
            if work_id == "RJ000002" && p.bytes_received > 0
    )));
    assert!(library_root.join("RJ000002/RJ000001.txt").exists());
    assert!(page
        .products
        .iter()
        .all(|product| product.download.status == WorkDownloadStatus::Downloaded));
    assert!(matches!(
        events.first(),
        Some(BulkWorkDownloadProgress::Selecting)
    ));
    assert!(matches!(
        events.last(),
        Some(BulkWorkDownloadProgress::Completed { .. })
    ));

    std::fs::remove_dir_all(root).unwrap();

    Ok(())
}

#[tokio::test]
async fn bulk_download_preview_reports_expected_bytes() -> Result<()> {
    let library = migrated_library().await?;
    let root = test_dir("bulk-preview");
    let library_root = root.join("library");
    let download_root = root.join("downloads");
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;
    library
        .download_work_with_source(
            WorkDownloadRequest::new("RJ000001", &library_root, &download_root),
            &FakeDownloadSource,
        )
        .await?;

    let preview = library
        .preview_download_products_with_source(
            BulkWorkDownloadPreviewRequest {
                query: ProductListQuery::default(),
                work_ids: None,
                skip_downloaded: true,
                cancellation_token: None,
                progress_sink: None,
            },
            &FakeDownloadSource,
        )
        .await?;

    assert_eq!(preview.total_count, 2);
    assert_eq!(preview.requested_count, 1);
    assert_eq!(preview.skipped_downloaded_count, 1);
    assert_eq!(preview.planned_count, 1);
    assert_eq!(preview.failed_count, 0);
    assert_eq!(preview.known_expected_bytes, 10);
    assert_eq!(preview.total_expected_bytes, Some(10));
    assert_eq!(preview.unknown_size_count, 0);
    assert_eq!(preview.works[0].work_id, "RJ000002");
    assert_eq!(preview.works[0].file_count, 1);

    std::fs::remove_dir_all(root).unwrap();

    Ok(())
}

#[tokio::test]
async fn bulk_download_preview_reports_progress() -> Result<()> {
    let library = migrated_library().await?;
    let root = test_dir("bulk-preview-progress");
    let library_root = root.join("library");
    let download_root = root.join("downloads");
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;
    library
        .download_work_with_source(
            WorkDownloadRequest::new("RJ000001", &library_root, &download_root),
            &FakeDownloadSource,
        )
        .await?;
    let sink = RecordingBulkDownloadPreviewProgressSink::default();

    let preview = library
        .preview_download_products_with_source(
            BulkWorkDownloadPreviewRequest {
                query: ProductListQuery::default(),
                work_ids: None,
                skip_downloaded: true,
                cancellation_token: None,
                progress_sink: Some(&sink),
            },
            &FakeDownloadSource,
        )
        .await?;
    let events = sink.events.lock().expect("preview events lock");

    assert_eq!(preview.requested_count, 1);
    assert!(matches!(
        events.first(),
        Some(BulkWorkDownloadPreviewProgress::Selecting)
    ));
    assert!(matches!(
        events.get(1),
        Some(BulkWorkDownloadPreviewProgress::Selected {
            requested_count: 1,
            ..
        })
    ));
    assert!(events.iter().any(|event| {
        matches!(
            event,
            BulkWorkDownloadPreviewProgress::WorkStarted {
                work_id,
                current: 1,
                total: 1
            } if work_id == "RJ000002"
        )
    }));
    assert!(events.iter().any(|event| {
        matches!(
            event,
            BulkWorkDownloadPreviewProgress::WorkPlanned {
                work_id,
                current: 1,
                total: 1,
                known_expected_bytes: 10,
                total_expected_bytes: Some(10),
                unknown_size_count: 0
            } if work_id == "RJ000002"
        )
    }));
    assert!(matches!(
        events.last(),
        Some(BulkWorkDownloadPreviewProgress::Completed {
            planned_count: 1,
            failed_count: 0,
            ..
        })
    ));

    std::fs::remove_dir_all(root).unwrap();

    Ok(())
}

#[tokio::test]
async fn bulk_download_limits_to_supplied_work_ids() -> Result<()> {
    let library = migrated_library().await?;
    let root = test_dir("bulk-download-limited");
    let library_root = root.join("library");
    let download_root = root.join("downloads");
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;

    let report = library
        .download_products_with_source(
            BulkWorkDownloadRequest {
                work_ids: Some(vec!["RJ000002".to_owned()]),
                ..BulkWorkDownloadRequest::new(
                    ProductListQuery::default(),
                    &library_root,
                    &download_root,
                )
            },
            &FakeDownloadSource,
        )
        .await?;
    let first = library.storage().work_download_state("RJ000001").await?;
    let second = library.storage().work_download_state("RJ000002").await?;

    assert_eq!(report.total_count, 1);
    assert_eq!(report.requested_count, 1);
    assert_eq!(report.succeeded_count, 1);
    assert_eq!(first.status, WorkDownloadStatus::NotDownloaded);
    assert_eq!(second.status, WorkDownloadStatus::Downloaded);

    std::fs::remove_dir_all(root).unwrap();

    Ok(())
}

#[tokio::test]
async fn bulk_download_continues_after_work_failure() -> Result<()> {
    let library = migrated_library().await?;
    let root = test_dir("bulk-download-failure");
    let library_root = root.join("library");
    let download_root = root.join("downloads");
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;

    let progress = RecordingBulkDownloadProgressSink::default();
    let mut request =
        BulkWorkDownloadRequest::new(ProductListQuery::default(), &library_root, &download_root);
    request.progress_sink = Some(&progress);
    let report = library
        .download_products_with_source(
            request,
            &FailingDownloadSource {
                fail_work_id: "RJ000002",
                failure: || {
                    LibraryError::Download(dldr_download::DownloadError::Io(
                        std::io::Error::from_raw_os_error(28),
                    ))
                },
            },
        )
        .await?;
    let failed_state = library.storage().work_download_state("RJ000002").await?;

    assert_eq!(report.requested_count, 2);
    assert_eq!(report.succeeded_count, 1);
    assert_eq!(report.failed_count, 1);
    assert_eq!(report.failed_works[0].work_id, "RJ000002");
    assert_eq!(failed_state.status, WorkDownloadStatus::Failed);
    let events = progress.events.lock().unwrap();
    let failures: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            BulkWorkDownloadProgress::WorkFailed {
                work_id,
                error_code,
                error_details,
                ..
            } => Some((work_id, error_code, error_details)),
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].0, "RJ000002");
    assert_eq!(failures[0].1, "download");
    assert_eq!(failures[0].2["osCode"], 28);
    assert_eq!(failures[0].2["downloadErrorKind"], "io");
    assert!(library_root.join("RJ000001/RJ000001.txt").exists());

    std::fs::remove_dir_all(root).unwrap();

    Ok(())
}

#[test]
fn download_path_root_check_rejects_root_and_sibling_prefixes() {
    let root = PathBuf::from("/tmp/dlsite/library");

    assert!(path_is_download_child_of_any_root(
        &root.join("RJ000001"),
        std::slice::from_ref(&root)
    ));
    assert!(!path_is_download_child_of_any_root(
        &root,
        std::slice::from_ref(&root)
    ));
    assert!(!path_is_download_child_of_any_root(
        &PathBuf::from("/tmp/dlsite/library-other/RJ000001"),
        &[root]
    ));
}

#[tokio::test]
async fn sync_can_use_one_shot_password_without_remembering() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(false)).await?;

    let report = library
        .sync_account_with_source(
            AccountSyncRequest {
                account_id: "account-a",
                password: Some("secret"),
                cancellation_token: None,
                progress_sink: None,
                two_factor_prompt: None,
            },
            &sync_source(),
        )
        .await?;

    assert_eq!(report.cached_work_count, 2);

    Ok(())
}

#[tokio::test]
async fn cancelled_two_factor_download_records_cancellation_and_stops_bulk() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;
    let root = test_dir("two-factor-cancel-download");
    let source = TwoFactorSyncSource::new();
    let prompt = ScriptedTwoFactorPrompt::with_responses([TwoFactorPromptResponse::Cancelled]);
    let error = library
        .download_products_with_source(
            BulkWorkDownloadRequest {
                two_factor_prompt: Some(&prompt),
                ..BulkWorkDownloadRequest::new(
                    ProductListQuery::default(),
                    &root.join("library"),
                    &root.join("staging"),
                )
            },
            &source,
        )
        .await
        .expect_err("cancelled bulk must stop");
    assert_eq!(error.failure_code(), "cancelled");
    assert_eq!(prompt.requests().len(), 1);
    let page = library.list_products(&ProductListQuery::default()).await?;
    assert_eq!(
        page.products
            .iter()
            .filter(|p| p.download.status == WorkDownloadStatus::Cancelled)
            .count(),
        1
    );
    assert_eq!(
        page.products
            .iter()
            .filter(|p| p.download.status == WorkDownloadStatus::NotDownloaded)
            .count(),
        1
    );
    assert_eq!(std::fs::read_dir(root.join("library"))?.count(), 0);
    Ok(())
}

#[tokio::test]
async fn authentication_http_waits_cancel_and_release_account_lock() -> Result<()> {
    struct PendingSource {
        phase: &'static str,
        entered: tokio::sync::Notify,
    }
    impl PendingSource {
        async fn wait(&self, phase: &str) {
            if self.phase == phase {
                self.entered.notify_one();
                std::future::pending::<()>().await;
            }
        }
    }
    #[async_trait]
    impl DlsiteAuthSource for PendingSource {
        async fn restore_session(&self, _: &SessionSnapshot) -> Result<()> {
            self.wait("restore").await;
            Ok(())
        }
        async fn validate_session(&self) -> Result<bool> {
            self.wait("validate").await;
            Ok(false)
        }
        async fn begin_login(&self, _: &Credentials) -> Result<LoginOutcome> {
            self.wait("login").await;
            Ok(LoginOutcome::TwoFactorRequired(
                TwoFactorSyncSource::challenge(),
            ))
        }
        async fn complete_two_factor(
            &self,
            _: &TwoFactorChallenge,
            _: &str,
        ) -> Result<SessionSnapshot> {
            self.wait("submit").await;
            panic!("unexpected code submission")
        }
    }
    for phase in ["restore", "validate", "login", "submit"] {
        let library = migrated_library().await?;
        let account = library.save_account(save_account_request(true)).await?;
        let session_ref = CredentialRef::account_session(&account.id)?;
        library
            .credentials
            .save_password(&session_ref, "old-session")?;
        let token = CancellationToken::new();
        let source = PendingSource {
            phase,
            entered: tokio::sync::Notify::new(),
        };
        let prompt = ScriptedTwoFactorPrompt::new([VALID_TWO_FACTOR_CODE]);
        let result = tokio::time::timeout(Duration::from_secs(2), async {
            tokio::join!(
                library.authenticate_account(&account, None, Some(&prompt), Some(&token), &source),
                async {
                    source.entered.notified().await;
                    token.cancel();
                }
            )
            .0
        })
        .await
        .expect("authentication must observe cancellation");
        assert!(result
            .expect_err("pending request must cancel")
            .is_cancelled());
        assert!(library.login_lock(&account.id).try_lock().is_ok());
        assert!(prompt.requests().len() <= 1);
        if matches!(phase, "restore" | "validate") {
            assert_eq!(
                library.credentials.load_password(&session_ref)?.as_deref(),
                Some("old-session")
            );
        } else {
            assert!(library.credentials.load_password(&session_ref)?.is_none());
        }
        library
            .authenticate_account(&account, None, None, None, &sync_source())
            .await?;
    }
    Ok(())
}

#[tokio::test]
async fn bulk_token_cancellation_keeps_completed_report() -> Result<()> {
    struct CancelAfterFirst(CancellationToken);
    impl BulkWorkDownloadProgressSink for CancelAfterFirst {
        fn emit(&self, progress: BulkWorkDownloadProgress) {
            if matches!(progress, BulkWorkDownloadProgress::WorkCompleted { .. }) {
                self.0.cancel();
            }
        }
    }
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;
    let root = test_dir("bulk-partial-cancel");
    let token = CancellationToken::new();
    let sink = CancelAfterFirst(token.clone());
    let result = library
        .download_products_with_source(
            BulkWorkDownloadRequest {
                cancellation_token: Some(&token),
                progress_sink: Some(&sink),
                ..BulkWorkDownloadRequest::new(
                    ProductListQuery::default(),
                    &root.join("library"),
                    &root.join("staging"),
                )
            },
            &FakeDownloadSource,
        )
        .await;
    let LibraryError::BulkInterrupted {
        report,
        interrupted_work,
        ..
    } = result.unwrap_err()
    else {
        panic!("partial report required")
    };
    assert_eq!(report.succeeded_count, 1);
    assert_eq!(report.failed_count, 0);
    assert!(interrupted_work.is_none());
    assert!(report.succeeded_works[0].local_path.is_dir());
    Ok(())
}

#[tokio::test]
async fn replacement_db_failure_restores_old_bytes_and_complete_record() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;
    let root = test_dir("replacement-db-rollback");
    let library_root = root.join("library");
    let staging_root = root.join("staging");
    library
        .download_work_with_source(
            WorkDownloadRequest::new("RJ000001", &library_root, &staging_root),
            &FakeDownloadSource,
        )
        .await?;
    tokio::fs::write(library_root.join("RJ000001/RJ000001.txt"), b"old bytes!").await?;
    let previous = library.storage.work_download_state("RJ000001").await?;
    let mut tx = library.storage.begin_write().await?;
    tx.execute("CREATE TRIGGER reject_download BEFORE UPDATE ON work_downloads BEGIN SELECT RAISE(ABORT, 'injected DB failure'); END").await?;
    tx.commit().await?;
    let error = library
        .download_work_with_source(
            WorkDownloadRequest {
                replace_existing: true,
                ..WorkDownloadRequest::new("RJ000001", &library_root, &staging_root)
            },
            &FakeDownloadSource,
        )
        .await
        .unwrap_err();
    assert_eq!(error.failure_code(), "storage");
    assert_eq!(
        library.storage.work_download_state("RJ000001").await?,
        previous
    );
    assert_eq!(
        tokio::fs::read(library_root.join("RJ000001/RJ000001.txt")).await?,
        b"old bytes!"
    );
    assert!(staging_root.join("RJ000001/RJ000001.txt").is_file());
    assert!(library
        .storage
        .download_finalization("RJ000001")
        .await?
        .is_none());
    Ok(())
}

#[tokio::test]
async fn replacement_prompt_cancel_preserves_download_and_concurrent_consumers_share_login(
) -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;
    let root = test_dir("replacement-auth-cancel");
    let library_root = root.join("library");
    let staging = root.join("staging");
    library
        .download_work_with_source(
            WorkDownloadRequest::new("RJ000001", &library_root, &staging),
            &FakeDownloadSource,
        )
        .await?;
    let previous = library.storage.work_download_state("RJ000001").await?;
    let source = TwoFactorSyncSource::new();
    let cancel = ScriptedTwoFactorPrompt::with_responses([TwoFactorPromptResponse::Cancelled]);
    let result = library
        .download_work_with_source(
            WorkDownloadRequest {
                replace_existing: true,
                two_factor_prompt: Some(&cancel),
                ..WorkDownloadRequest::new("RJ000001", &library_root, &staging)
            },
            &source,
        )
        .await;
    assert!(result.unwrap_err().is_cancelled());
    assert_eq!(
        library.storage.work_download_state("RJ000001").await?,
        previous
    );
    assert_eq!(
        std::fs::read(library_root.join("RJ000001/RJ000001.txt"))?,
        b"downloaded"
    );

    let prompt = ScriptedTwoFactorPrompt::new([VALID_TWO_FACTOR_CODE]);
    let source = TwoFactorSyncSource::new();
    let (sync, download) = tokio::join!(
        library.sync_account_with_source(
            AccountSyncRequest {
                two_factor_prompt: Some(&prompt),
                ..AccountSyncRequest::new("account-a")
            },
            &source
        ),
        library.download_work_with_source(
            WorkDownloadRequest {
                two_factor_prompt: Some(&prompt),
                ..WorkDownloadRequest::new("RJ000002", &library_root, &staging)
            },
            &source
        )
    );
    sync?;
    download?;
    assert_eq!(source.state().login_count, 1);
    assert_eq!(prompt.requests().len(), 1);
    Ok(())
}

#[tokio::test]
async fn exhausted_two_factor_retries_remain_failed() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;
    let source = TwoFactorSyncSource::new();
    let prompt = ScriptedTwoFactorPrompt::new(["000000", "000000", "000000"]);
    let error = library
        .sync_account_with_source(
            AccountSyncRequest {
                two_factor_prompt: Some(&prompt),
                ..AccountSyncRequest::new("account-a")
            },
            &source,
        )
        .await
        .unwrap_err();
    assert!(!error.is_cancelled());
    assert_eq!(error.failure_code(), "two_factor_rejected");
    assert_eq!(
        library.storage.sync_runs_for_account("account-a").await?[0].status,
        dldr_storage::SyncRunStatus::Failed
    );
    assert_eq!(prompt.requests().len(), 3);
    Ok(())
}

#[tokio::test]
async fn replacement_rejects_staging_overlap_with_manual_content_before_deletion() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;
    let root = test_dir("manual-staging-overlap");
    let library_root = root.join("library");
    let staging = library_root.join("staging");
    let old = staging.join("RJ000001");
    std::fs::create_dir_all(&old)?;
    std::fs::write(old.join("content"), b"keep")?;
    library
        .mark_work_downloaded(WorkDownloadMarkRequest::new(
            "RJ000001",
            &library_root,
            &old,
        ))
        .await?;
    let previous = library.storage.work_download_state("RJ000001").await?;
    let result = library
        .download_work_with_source(
            WorkDownloadRequest {
                replace_existing: true,
                ..WorkDownloadRequest::new("RJ000001", &library_root, &staging)
            },
            &FakeDownloadSource,
        )
        .await;
    assert!(matches!(
        result,
        Err(LibraryError::DownloadPathOutsideRoots(_))
    ));
    assert_eq!(std::fs::read(old.join("content"))?, b"keep");
    assert_eq!(
        library.storage.work_download_state("RJ000001").await?,
        previous
    );
    Ok(())
}

#[tokio::test]
async fn manual_replacement_interrupted_before_intent_tracks_staging_for_later_delete() -> Result<()>
{
    struct InterruptedTransfer {
        cancel: bool,
    }
    impl_fake_auth_source!(InterruptedTransfer);
    #[async_trait]
    impl WorkDownloadSource for InterruptedTransfer {
        async fn download_plan(&self, id: &WorkId) -> Result<DownloadPlan> {
            FakeDownloadSource.download_plan(id).await
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
                .await?;
            if self.cancel {
                Err(LibraryError::Download(
                    dldr_download::DownloadError::Cancelled,
                ))
            } else {
                Err(LibraryError::SyncSource(
                    "injected transfer failure".to_owned(),
                ))
            }
        }
    }
    for cancel in [false, true] {
        let root = test_dir("manual-transfer-staging");
        std::fs::create_dir_all(&root)?;
        let storage_path = root.join("library.sqlite");
        let storage = Storage::open(&storage_path).await?;
        storage.run_migrations().await?;
        let library = Library::new(storage, Arc::new(InMemoryCredentialStore::new()));
        library.save_account(save_account_request(true)).await?;
        library
            .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
            .await?;
        let library_root = root.join("library");
        let staging_root = root.join("staging");
        let old = library_root.join("RJ000001");
        std::fs::create_dir_all(&old)?;
        std::fs::write(old.join("content"), b"manual old")?;
        let mut previous = library
            .mark_work_downloaded(WorkDownloadMarkRequest::new(
                "RJ000001",
                &library_root,
                &old,
            ))
            .await?;
        assert!(previous.staging_path.is_none());
        let error = library
            .download_work_with_source(
                WorkDownloadRequest {
                    replace_existing: true,
                    ..WorkDownloadRequest::new("RJ000001", &library_root, &staging_root)
                },
                &InterruptedTransfer { cancel },
            )
            .await
            .unwrap_err();
        assert_eq!(error.is_cancelled(), cancel);
        assert!(library
            .storage
            .download_finalization("RJ000001")
            .await?
            .is_none());
        assert_eq!(std::fs::read(old.join("content"))?, b"manual old");
        let staging = staging_root.join("RJ000001").canonicalize()?;
        previous.staging_path = Some(staging.to_string_lossy().into_owned());
        assert_eq!(
            library.storage.work_download_state("RJ000001").await?,
            previous
        );
        drop(library);
        let reopened = Library::new(
            Storage::open(&storage_path).await?,
            Arc::new(InMemoryCredentialStore::new()),
        );
        reopened
            .remove_work_download(WorkDownloadRemovalRequest::new(
                "RJ000001",
                &library_root,
                &staging_root,
            ))
            .await?;
        assert!(!staging.exists());
        assert!(!old.exists());
    }
    Ok(())
}

#[tokio::test]
async fn cancelling_replacement_during_transfer_or_preparation_keeps_old_state() -> Result<()> {
    struct CancelProgress {
        token: CancellationToken,
        at_finalization: bool,
    }
    impl WorkDownloadProgressSink for CancelProgress {
        fn emit(&self, progress: WorkDownloadProgress) {
            if (self.at_finalization && matches!(progress, WorkDownloadProgress::Finalizing))
                || (!self.at_finalization && matches!(progress, WorkDownloadProgress::Download(_)))
            {
                self.token.cancel();
            }
        }
    }
    for at_finalization in [false, true] {
        let library = migrated_library().await?;
        library.save_account(save_account_request(true)).await?;
        library
            .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
            .await?;
        let root = test_dir("replacement-cancel-boundary");
        let library_root = root.join("library");
        let staging = root.join("staging");
        library
            .download_work_with_source(
                WorkDownloadRequest::new("RJ000001", &library_root, &staging),
                &FakeDownloadSource,
            )
            .await?;
        std::fs::write(library_root.join("RJ000001/RJ000001.txt"), b"old content")?;
        let old = library.storage.work_download_state("RJ000001").await?;
        let token = CancellationToken::new();
        let sink = CancelProgress {
            token: token.clone(),
            at_finalization,
        };
        let error = library
            .download_work_with_source(
                WorkDownloadRequest {
                    replace_existing: true,
                    cancellation_token: Some(&token),
                    progress_sink: Some(&sink),
                    ..WorkDownloadRequest::new("RJ000001", &library_root, &staging)
                },
                &FakeDownloadSource,
            )
            .await
            .unwrap_err();
        assert!(error.is_cancelled());
        assert_eq!(library.storage.work_download_state("RJ000001").await?, old);
        assert_eq!(
            std::fs::read(library_root.join("RJ000001/RJ000001.txt"))?,
            b"old content"
        );
        assert!(staging.join("RJ000001/RJ000001.txt").is_file());
    }
    Ok(())
}

#[tokio::test]
async fn session_restore_distinguishes_invalid_data_from_validation_network_errors() -> Result<()> {
    struct Probe {
        outcome: &'static str,
        logins: std::sync::atomic::AtomicUsize,
    }
    #[async_trait]
    impl DlsiteAuthSource for Probe {
        async fn restore_session(&self, _: &SessionSnapshot) -> Result<()> {
            if self.outcome == "malformed" {
                return Err(LibraryError::Json(
                    serde_json::from_str::<Value>("invalid").unwrap_err(),
                ));
            }
            Ok(())
        }
        async fn validate_session(&self) -> Result<bool> {
            if self.outcome == "network" {
                return Err(LibraryError::SyncSource("offline".to_owned()));
            }
            Ok(self.outcome == "valid")
        }
        async fn begin_login(&self, _: &Credentials) -> Result<LoginOutcome> {
            self.logins
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(LoginOutcome::Authorized(SessionSnapshot {
                cookies_json: "fresh".to_owned(),
            }))
        }
        async fn complete_two_factor(
            &self,
            _: &TwoFactorChallenge,
            _: &str,
        ) -> Result<SessionSnapshot> {
            unreachable!()
        }
    }
    for outcome in ["valid", "expired", "malformed", "network"] {
        let library = migrated_library().await?;
        let account = library.save_account(save_account_request(true)).await?;
        let session_ref = CredentialRef::account_session(&account.id)?;
        library.credentials.save_password(&session_ref, "stored")?;
        // New Library instance, same durable stores; no process-local auth state reused.
        let library = Library::new(library.storage.clone(), library.credentials.clone());
        let source = Probe {
            outcome,
            logins: std::sync::atomic::AtomicUsize::new(0),
        };
        let result = library
            .authenticate_account(&account, None, None, None, &source)
            .await;
        let should_login = matches!(outcome, "expired" | "malformed");
        assert_eq!(result.is_err(), outcome == "network");
        assert_eq!(
            source.logins.load(std::sync::atomic::Ordering::SeqCst),
            usize::from(should_login)
        );
        assert_eq!(
            library.credentials.load_password(&session_ref)?.as_deref(),
            Some(if should_login { "fresh" } else { "stored" })
        );
    }
    Ok(())
}

#[tokio::test]
async fn cli_login_verifies_before_saving_reuses_session_and_logout_keeps_works() -> Result<()> {
    let library = migrated_library().await?;
    let mut request = save_account_request(true);
    request.id = None;
    let source = TwoFactorSyncSource::new();
    let prompt = ScriptedTwoFactorPrompt::new(["000000", VALID_TWO_FACTOR_CODE]);
    let account = library
        .login_account_with_source(request, Some(&prompt), None, &source)
        .await?;
    assert!(account.last_login_at.is_some());
    assert!(account.enabled);
    let password_ref = CredentialRef::account_password(&account.id)?;
    let session_ref = CredentialRef::account_session(&account.id)?;
    assert_eq!(
        library.credentials.load_password(&password_ref)?.as_deref(),
        Some("secret")
    );
    assert_eq!(
        library.credentials.load_password(&session_ref)?.as_deref(),
        Some(TWO_FACTOR_SESSION)
    );
    library
        .sync_account_with_source(AccountSyncRequest::new(&account.id), &source)
        .await?;
    assert_eq!(source.state().login_count, 1);
    let work = library.product_detail("RJ000001").await?;
    library.logout_account(&account.id).await?;
    let logged_out = library.find_account(&account.id).await?;
    assert!(!logged_out.enabled);
    assert!(logged_out.credential_ref.is_none());
    assert!(library.credentials.load_password(&password_ref)?.is_none());
    assert!(library.credentials.load_password(&session_ref)?.is_none());
    assert_eq!(
        library.product_detail("RJ000001").await?.download,
        work.download
    );
    let mut again = save_account_request(true);
    again.id = Some(account.id.clone());
    let prompt = ScriptedTwoFactorPrompt::new([VALID_TWO_FACTOR_CODE]);
    let account = library
        .login_account_with_source(again, Some(&prompt), None, &source)
        .await?;
    assert!(account.enabled);
    assert_eq!(
        library.product_detail("RJ000001").await?.owners[0].account_id,
        account.id
    );
    Ok(())
}

#[tokio::test]
async fn cli_rejected_or_cancelled_login_preserves_existing_identity_and_vault() -> Result<()> {
    let library = migrated_library().await?;
    let account = library.save_account(save_account_request(true)).await?;
    let session_ref = CredentialRef::account_session(&account.id)?;
    library
        .credentials
        .save_password(&session_ref, "previous-session")?;
    for responses in [
        vec![TwoFactorPromptResponse::Code("bad".into()); 3],
        vec![TwoFactorPromptResponse::Cancelled],
    ] {
        let mut request = save_account_request(true);
        request.label = "must not overwrite".into();
        let prompt = ScriptedTwoFactorPrompt::with_responses(responses);
        assert!(library
            .login_account_with_source(request, Some(&prompt), None, &TwoFactorSyncSource::new())
            .await
            .is_err());
        assert_eq!(library.find_account(&account.id).await?, account);
        assert_eq!(
            library.credentials.load_password(&session_ref)?.as_deref(),
            Some("previous-session")
        );
        assert_eq!(
            library
                .credentials
                .load_password(&CredentialRef::account_password(&account.id)?)?
                .as_deref(),
            Some("secret")
        );
    }
    let mut request = save_account_request(true);
    request.id = None;
    let token = CancellationToken::new();
    token.cancel();
    let source = TwoFactorSyncSource::new();
    assert!(library
        .login_account_with_source(request, None, Some(&token), &source)
        .await
        .unwrap_err()
        .is_cancelled());
    assert_eq!(source.state().login_count, 0);
    assert_eq!(library.accounts().await?.len(), 1);
    Ok(())
}

#[tokio::test]
async fn two_factor_sync_completes_with_a_prompted_code() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;

    let source = TwoFactorSyncSource::new();
    let prompt = ScriptedTwoFactorPrompt::new([VALID_TWO_FACTOR_CODE]);
    let report = library
        .sync_account_with_source(
            AccountSyncRequest {
                two_factor_prompt: Some(&prompt),
                ..AccountSyncRequest::new("account-a")
            },
            &source,
        )
        .await?;

    assert_eq!(report.cached_work_count, 2);
    assert_eq!(source.state().submitted_codes, vec![VALID_TWO_FACTOR_CODE]);

    let requests = prompt.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].account_id, "account-a");
    assert_eq!(requests[0].attempt, 1);
    assert!(!requests[0].previous_code_rejected);

    Ok(())
}

#[tokio::test]
async fn rejected_two_factor_code_is_prompted_again() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;

    let source = TwoFactorSyncSource::new();
    let prompt = ScriptedTwoFactorPrompt::new(["000000", VALID_TWO_FACTOR_CODE]);

    library
        .sync_account_with_source(
            AccountSyncRequest {
                two_factor_prompt: Some(&prompt),
                ..AccountSyncRequest::new("account-a")
            },
            &source,
        )
        .await?;

    assert_eq!(
        source.state().submitted_codes,
        vec!["000000", VALID_TWO_FACTOR_CODE]
    );

    let requests = prompt.requests();
    assert_eq!(requests.len(), 2);
    assert!(!requests[0].previous_code_rejected);
    assert_eq!(requests[1].attempt, 2);
    assert!(requests[1].previous_code_rejected);

    Ok(())
}

#[tokio::test]
async fn cancelled_two_factor_prompt_reports_a_cancelled_sync() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;

    let source = TwoFactorSyncSource::new();
    let prompt = ScriptedTwoFactorPrompt::with_responses([TwoFactorPromptResponse::Cancelled]);
    let result = library
        .sync_account_with_source(
            AccountSyncRequest {
                two_factor_prompt: Some(&prompt),
                ..AccountSyncRequest::new("account-a")
            },
            &source,
        )
        .await;

    let error = result.expect_err("cancelling the prompt must not sync");

    assert!(matches!(
        &error,
        LibraryError::TwoFactorCancelled(account_id) if account_id == "account-a"
    ));
    // `dldr-jobs` recognizes only this exact code as a cancellation, so the job it belongs
    // to finishes as Cancelled rather than Failed.
    assert_eq!(error.failure_code(), "cancelled");
    assert_eq!(
        error.support_details()["failureKind"],
        "two_factor_cancelled"
    );
    assert!(source.state().submitted_codes.is_empty());

    let runs = library.storage().sync_runs_for_account("account-a").await?;
    assert_eq!(runs[0].status, dldr_storage::SyncRunStatus::Cancelled);

    Ok(())
}

#[tokio::test]
async fn timed_out_two_factor_prompt_fails_sync() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;

    let source = TwoFactorSyncSource::new();
    let prompt = ScriptedTwoFactorPrompt::with_responses([TwoFactorPromptResponse::TimedOut]);
    let result = library
        .sync_account_with_source(
            AccountSyncRequest {
                two_factor_prompt: Some(&prompt),
                ..AccountSyncRequest::new("account-a")
            },
            &source,
        )
        .await;

    let error = result.expect_err("an unanswered prompt must not sync");

    assert!(matches!(
        &error,
        LibraryError::TwoFactorTimedOut(account_id) if account_id == "account-a"
    ));
    // Unlike cancellation, nobody chose this outcome, so it stays a visible failure.
    assert_eq!(error.failure_code(), "two_factor_timed_out");
    assert!(source.state().submitted_codes.is_empty());

    Ok(())
}

#[tokio::test]
async fn cancelling_a_job_waiting_on_the_login_lock_does_not_wait_for_the_dialog() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;

    // Hold the account's login lock the way a job sitting on an open dialog would.
    let login_lock = library.login_lock("account-a");
    let held = login_lock.lock().await;

    let cancellation = CancellationToken::new();
    let source = TwoFactorSyncSource::new();
    let prompt = ScriptedTwoFactorPrompt::new([VALID_TWO_FACTOR_CODE]);
    let queued = library.sync_account_with_source(
        AccountSyncRequest {
            two_factor_prompt: Some(&prompt),
            cancellation_token: Some(&cancellation),
            ..AccountSyncRequest::new("account-a")
        },
        &source,
    );

    let result = tokio::join!(queued, async {
        // Let the queued sync reach the lock before cancelling it.
        tokio::time::sleep(Duration::from_millis(50)).await;
        cancellation.cancel();
    })
    .0;

    assert!(matches!(result, Err(LibraryError::Cancelled)));
    // The lock was never released, so the queued job gave up on its own.
    assert!(prompt.requests().is_empty());
    drop(held);

    Ok(())
}

#[tokio::test]
async fn two_factor_challenge_without_a_prompt_is_reported() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;

    let result = library
        .sync_account_with_source(
            AccountSyncRequest::new("account-a"),
            &TwoFactorSyncSource::new(),
        )
        .await;

    assert!(matches!(
        result,
        Err(LibraryError::TwoFactorPromptUnavailable(account_id)) if account_id == "account-a"
    ));

    Ok(())
}

#[tokio::test]
async fn stored_session_skips_the_two_factor_prompt() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;

    let source = TwoFactorSyncSource::new();
    let prompt = ScriptedTwoFactorPrompt::new([VALID_TWO_FACTOR_CODE]);
    let request = || AccountSyncRequest {
        two_factor_prompt: Some(&prompt),
        ..AccountSyncRequest::new("account-a")
    };

    library.sync_account_with_source(request(), &source).await?;
    library.sync_account_with_source(request(), &source).await?;

    // The second sync reused the stored session instead of logging in again.
    assert_eq!(prompt.requests().len(), 1);
    assert_eq!(source.state().login_count, 1);
    assert_eq!(
        source.state().restored_session.as_deref(),
        Some(TWO_FACTOR_SESSION)
    );

    Ok(())
}

#[tokio::test]
async fn session_is_not_stored_when_the_account_does_not_remember_its_password() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(false)).await?;

    let source = TwoFactorSyncSource::new();
    let prompt = ScriptedTwoFactorPrompt::new([VALID_TWO_FACTOR_CODE, VALID_TWO_FACTOR_CODE]);
    let request = || AccountSyncRequest {
        password: Some("secret"),
        two_factor_prompt: Some(&prompt),
        ..AccountSyncRequest::new("account-a")
    };

    library.sync_account_with_source(request(), &source).await?;

    // The session cookie is as good as the password, so an account that keeps neither
    // leaves nothing behind for the next job to reuse.
    let session_ref = CredentialRef::account_session("account-a")?;

    assert!(library.credentials.load_password(&session_ref)?.is_none());

    library.sync_account_with_source(request(), &source).await?;

    assert_eq!(prompt.requests().len(), 2);
    assert_eq!(source.state().login_count, 2);

    Ok(())
}

#[tokio::test]
async fn session_is_stored_when_the_account_remembers_its_password() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;

    let source = TwoFactorSyncSource::new();
    let prompt = ScriptedTwoFactorPrompt::new([VALID_TWO_FACTOR_CODE]);

    library
        .sync_account_with_source(
            AccountSyncRequest {
                two_factor_prompt: Some(&prompt),
                ..AccountSyncRequest::new("account-a")
            },
            &source,
        )
        .await?;

    let session_ref = CredentialRef::account_session("account-a")?;

    assert_eq!(
        library.credentials.load_password(&session_ref)?.as_deref(),
        Some(TWO_FACTOR_SESSION)
    );

    Ok(())
}

#[tokio::test]
async fn changing_the_password_forgets_the_stored_session() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;

    let source = TwoFactorSyncSource::new();
    let prompt = ScriptedTwoFactorPrompt::new([VALID_TWO_FACTOR_CODE, VALID_TWO_FACTOR_CODE]);
    let request = || AccountSyncRequest {
        two_factor_prompt: Some(&prompt),
        ..AccountSyncRequest::new("account-a")
    };

    library.sync_account_with_source(request(), &source).await?;
    library.save_account(save_account_request(true)).await?;
    library.sync_account_with_source(request(), &source).await?;

    assert_eq!(prompt.requests().len(), 2);
    assert_eq!(source.state().login_count, 2);

    Ok(())
}

#[tokio::test]
async fn missing_password_blocks_sync() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(false)).await?;

    assert!(matches!(
        library
            .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
            .await,
        Err(LibraryError::MissingPassword(account_id)) if account_id == "account-a"
    ));

    Ok(())
}

#[tokio::test]
async fn failed_sync_is_recorded_without_staling_cache() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;
    let mut failing_source = sync_source();
    failing_source.fail_at = Some(FakeFailurePoint::Works);

    assert!(matches!(
        library
            .sync_account_with_source(AccountSyncRequest::new("account-a"), &failing_source)
            .await,
        Err(LibraryError::SyncSource(_))
    ));

    let page = library.list_products(&ProductListQuery::default()).await?;
    let sync_runs = library.storage().sync_runs_for_account("account-a").await?;

    assert_eq!(page.total_count, 2);
    assert_eq!(sync_runs.len(), 2);
    assert_eq!(sync_runs[0].status, SyncRunStatus::Failed);
    assert_eq!(sync_runs[0].error_code, Some("sync_source".to_owned()));

    Ok(())
}

#[tokio::test]
async fn missing_work_details_are_cached_but_hidden_from_product_list() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;
    let mut source = sync_source();
    source.works.pop();

    let report = library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &source)
        .await?;

    let page = library.list_products(&ProductListQuery::default()).await?;
    let sync_runs = library.storage().sync_runs_for_account("account-a").await?;

    assert_eq!(report.purchased_count, 2);
    assert_eq!(report.cached_work_count, 2);
    assert_eq!(report.missing_detail_count, 1);
    assert_eq!(page.total_count, 1);
    assert_eq!(page.products.len(), 1);
    assert_eq!(page.products[0].work_id, "RJ000001");
    assert_eq!(sync_runs.len(), 1);
    assert_eq!(sync_runs[0].status, SyncRunStatus::Completed);

    Ok(())
}

#[tokio::test]
async fn cancellation_records_failed_sync() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;
    let token = CancellationToken::new();
    token.cancel();

    assert!(matches!(
        library
            .sync_account_with_source(
                AccountSyncRequest {
                    account_id: "account-a",
                    password: None,
                    cancellation_token: Some(&token),
                    progress_sink: None,
                    two_factor_prompt: None,
                },
                &sync_source(),
            )
            .await,
        Err(LibraryError::Cancelled)
    ));

    let sync_runs = library.storage().sync_runs_for_account("account-a").await?;

    assert_eq!(sync_runs.len(), 1);
    assert_eq!(sync_runs[0].status, SyncRunStatus::Cancelled);
    assert_eq!(sync_runs[0].error_code, Some("cancelled".to_owned()));

    Ok(())
}
