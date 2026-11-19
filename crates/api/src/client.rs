//! Shared HTTP client; endpoint families own their request and parsing logic.
mod auth;
mod catalog;
mod downloads;
mod response;

use crate::Result;
use cookie_store::CookieStore;
pub use downloads::{DownloadProbe, DownloadStream};
use reqwest::{redirect::Policy, Client, Response};
use reqwest_cookie_store::CookieStoreMutex;
pub(crate) use response::limited_text_snippet;
use std::sync::Arc;
use tokio::sync::Mutex;
use url::Url;

// A narrow HTTP boundary: never emit URLs, headers, bodies, or error strings.
trait DiagnosticRequest {
    async fn diagnostic_send(self) -> std::result::Result<Response, reqwest::Error>;
}
impl DiagnosticRequest for reqwest::RequestBuilder {
    async fn diagnostic_send(self) -> std::result::Result<Response, reqwest::Error> {
        let started = std::time::Instant::now();
        let (client, request) = self.build_split();
        let request = request?;
        let role = endpoint_role(request.url());
        tracing::info!(target:"dldr_diagnostic", phase="network", kind=role, "HTTP request started");
        let result = client.execute(request).await;
        let duration = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
        match &result {
            Ok(response) => {
                tracing::info!(target:"dldr_diagnostic", phase="network", kind=role, httpStatus=u64::from(response.status().as_u16()),durationMs=duration,"HTTP response received");
            }
            Err(error) => {
                tracing::error!(target:"dldr_diagnostic", phase="network",kind=role,durationMs=duration,isTimeout=error.is_timeout(),isConnect=error.is_connect(),failureKind="transport","HTTP transport failed");
            }
        }
        result
    }
}
fn endpoint_role(url: &Url) -> &'static str {
    match (url.host_str(), url.path()) {
        (Some("login.dlsite.com"), _) => "login",
        (Some("play.dlsite.com"), "/api/v3/content/count") => "loadingCount",
        (Some("play.dlsite.com"), "/api/v3/content/sales") => "purchases",
        (Some("play.dlsite.com"), "/api/v3/content/works") => "works",
        (Some("play.dlsite.com"), "/api/v3/download") => "download",
        (Some("www.dlsite.com"), "/home/api/=/product.json") => "metadata",
        _ => "other",
    }
}

#[derive(Debug, Clone)]
pub struct DlsiteClientConfig {
    pub user_agent: String,
}

impl Default for DlsiteClientConfig {
    fn default() -> Self {
        Self {
            user_agent: concat!("dldr/", env!("CARGO_PKG_VERSION")).to_owned(),
        }
    }
}

#[derive(Clone)]
pub struct DlsiteClient {
    http: Client,
    cookie_store: Arc<CookieStoreMutex>,
    works_batch_limit: Arc<Mutex<Option<usize>>>,
}

impl DlsiteClient {
    pub fn new(config: DlsiteClientConfig) -> Result<Self> {
        let cookie_store = Arc::new(CookieStoreMutex::new(CookieStore::default()));
        let http = Client::builder()
            .cookie_provider(cookie_store.clone())
            .redirect(Policy::none())
            .user_agent(config.user_agent)
            .build()?;

        Ok(Self {
            http,
            cookie_store,
            works_batch_limit: Arc::new(Mutex::new(None)),
        })
    }
}

#[cfg(test)]
mod tests;
