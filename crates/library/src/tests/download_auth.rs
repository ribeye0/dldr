use super::*;
use std::sync::atomic::AtomicUsize;

#[derive(Clone, Copy)]
enum Failure {
    Plan,
    Metadata,
    Html,
    Transfer,
    Io,
}

struct ExpiredDownloadSession {
    failure: Failure,
    persistent: bool,
    logins: AtomicUsize,
    plans: AtomicUsize,
    transfers: AtomicUsize,
}

#[async_trait]
impl DlsiteAuthSource for ExpiredDownloadSession {
    async fn restore_session(&self, _: &SessionSnapshot) -> Result<()> {
        Ok(())
    }
    async fn validate_session(&self) -> Result<bool> {
        // Play still accepts this session, even though storefront downloads do not.
        Ok(true)
    }
    async fn begin_login(&self, credentials: &Credentials) -> Result<LoginOutcome> {
        assert_fake_credentials(credentials);
        self.logins.fetch_add(1, Ordering::SeqCst);
        Ok(LoginOutcome::Authorized(SessionSnapshot {
            cookies_json: "fresh".into(),
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

impl ExpiredDownloadSession {
    fn stale(&self) -> bool {
        self.persistent || self.logins.load(Ordering::SeqCst) == 0
    }
}

#[async_trait]
impl WorkDownloadSource for ExpiredDownloadSession {
    async fn download_plan(&self, work_id: &WorkId) -> Result<DownloadPlan> {
        self.plans.fetch_add(1, Ordering::SeqCst);
        if matches!(self.failure, Failure::Plan) && self.stale() {
            return Err(ApiError::DownloadUnavailable {
                work_id: work_id.clone(),
                reason: dldr_api::DownloadUnavailableReason::NotAuthorized,
            }
            .into());
        }
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
        self.transfers.fetch_add(1, Ordering::SeqCst);
        if self.stale() && !matches!(self.failure, Failure::Plan) {
            if matches!(self.failure, Failure::Transfer) {
                let partial = job.target_root.join(job.work_id.as_ref()).join("partial");
                tokio::fs::create_dir_all(partial.parent().unwrap()).await?;
                tokio::fs::write(partial, b"keep these bytes").await?;
                sink(DownloadProgress {
                    phase: DownloadPhase::Downloading,
                    file_index: Some(0),
                    file_kind: Some(DownloadFileKind::Direct),
                    bytes_received: 15,
                    bytes_total: Some(100),
                });
            }
            return Err(match self.failure {
                Failure::Io => std::io::Error::from_raw_os_error(13).into(),
                Failure::Html => dldr_download::DownloadError::InvalidDownloadResponse {
                    reason: dldr_download::InvalidDownloadResponseReason::HtmlContentType {
                        content_type: "text/html".into(),
                    },
                }
                .into(),
                _ => dldr_download::DownloadError::Api(ApiError::NotAuthorized).into(),
            });
        }
        FakeDownloadSource
            .download_files(job, plan, cancellation, sink)
            .await
    }
}

#[tokio::test]
async fn download_refreshes_a_play_only_session_once_before_transfer() -> Result<()> {
    for (failure, persistent, success, logins, plans, transfers) in [
        (Failure::Plan, false, true, 1, 2, 1),
        (Failure::Metadata, false, true, 1, 2, 2),
        (Failure::Html, false, true, 1, 2, 2),
        (Failure::Plan, true, false, 1, 2, 0),
        (Failure::Metadata, true, false, 1, 2, 2),
        (Failure::Html, true, false, 1, 2, 2),
        (Failure::Transfer, false, false, 0, 1, 1),
        (Failure::Io, false, false, 0, 1, 1),
    ] {
        let library = migrated_library().await?;
        library.save_account(save_account_request(true)).await?;
        library
            .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
            .await?;
        let root = tempfile::tempdir()?;
        let library_root = root.path().join("library");
        let download_root = root.path().join("downloads");
        let source = ExpiredDownloadSession {
            failure,
            persistent,
            logins: AtomicUsize::new(0),
            plans: AtomicUsize::new(0),
            transfers: AtomicUsize::new(0),
        };
        let result = library
            .download_work_with_source(
                WorkDownloadRequest::new("RJ000001", &library_root, &download_root),
                &source,
            )
            .await;
        assert_eq!(result.is_ok(), success, "{result:?}");
        assert_eq!(source.logins.load(Ordering::SeqCst), logins);
        assert_eq!(source.plans.load(Ordering::SeqCst), plans);
        assert_eq!(source.transfers.load(Ordering::SeqCst), transfers);
        if matches!(failure, Failure::Transfer) {
            assert_eq!(
                tokio::fs::read(download_root.join("RJ000001/partial")).await?,
                b"keep these bytes"
            );
        }
        if success {
            assert_eq!(
                tokio::fs::read(library_root.join("RJ000001/RJ000001.txt")).await?,
                b"downloaded"
            );
            assert_eq!(
                library
                    .credentials
                    .load_password(&CredentialRef::account_session("account-a")?)?
                    .as_deref(),
                Some("fresh")
            );
        }
    }
    Ok(())
}
