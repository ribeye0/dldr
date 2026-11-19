//! DLsite adapters for the service ports; authentication has one implementation.
use crate::{DlsiteAuthSource, Result};
use async_trait::async_trait;
use dldr_api::{
    ContentCount, ContentQuery, Credentials, DlsiteClient, DownloadFile, DownloadPlan,
    LoginOutcome, PublicWork, Purchase, SessionSnapshot, SessionStatus, TwoFactorChallenge, Work,
    WorkId,
};
use dldr_download::{DownloadFileMetadata, DownloadJobRequest, DownloadProgress, DownloadedWork};

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
pub struct DlsiteSource {
    client: DlsiteClient,
    rate_limiter: Option<dldr_download::DownloadRateLimiter>,
}

impl DlsiteSource {
    pub fn new(client: DlsiteClient) -> Self {
        Self {
            client,
            rate_limiter: None,
        }
    }

    pub fn client(&self) -> &DlsiteClient {
        &self.client
    }

    pub fn with_rate_limiter(mut self, limiter: dldr_download::DownloadRateLimiter) -> Self {
        self.rate_limiter = Some(limiter);
        self
    }
}

#[async_trait]
impl DlsiteAuthSource for DlsiteSource {
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
impl WorkDownloadSource for DlsiteSource {
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

#[async_trait]
pub trait AccountSyncSource: DlsiteAuthSource {
    async fn content_count(&self) -> Result<ContentCount>;
    async fn purchases(&self) -> Result<Vec<Purchase>>;
    async fn works(&self, ids: &[WorkId]) -> Result<Vec<Work>>;
}

#[async_trait]
impl AccountSyncSource for DlsiteSource {
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
