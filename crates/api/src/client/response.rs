//! Shared response decoding and bounded diagnostic formatting.
use super::{DiagnosticRequest, DlsiteClient};
use crate::{raw::RawResponse, ApiError, Result};
use reqwest::{
    header::{CONTENT_TYPE, LOCATION},
    Response, StatusCode,
};
use serde::de::DeserializeOwned;
use url::Url;

impl DlsiteClient {
    pub async fn raw_get(&self, url: Url) -> Result<RawResponse> {
        self.raw_get_with_body_limit(url, 2048).await
    }

    pub async fn raw_get_with_body_limit(
        &self,
        url: Url,
        body_limit: usize,
    ) -> Result<RawResponse> {
        let res = self.http.get(url).diagnostic_send().await?;

        RawResponse::from_response_with_body_limit(res, body_limit).await
    }
}

pub(super) async fn parse_json_response<T>(res: Response) -> Result<T>
where
    T: DeserializeOwned,
{
    let status = res.status();

    if status == StatusCode::UNAUTHORIZED {
        return Err(ApiError::NotAuthorized);
    }

    if !status.is_success() {
        let endpoint = res.url().clone();
        let body_snippet = response_text_snippet(res).await;
        return Err(ApiError::UnexpectedStatus {
            endpoint,
            status,
            body_snippet,
        });
    }

    let endpoint = res.url().clone();
    let body = res.text().await?;
    parse_json_body(endpoint, &body)
}

pub(super) fn parse_json_body<T>(endpoint: Url, body: &str) -> Result<T>
where
    T: DeserializeOwned,
{
    let mut deserializer = serde_json::Deserializer::from_str(body);
    serde_path_to_error::deserialize(&mut deserializer).map_err(|err| ApiError::UnexpectedJson {
        endpoint,
        path: err.path().to_string(),
        source: err.into_inner(),
    })
}

pub(crate) fn redirect_location(res: &Response) -> Result<Url> {
    let endpoint = res.url().clone();
    let location = res
        .headers()
        .get(LOCATION)
        .ok_or_else(|| ApiError::LocationHeaderMissing {
            endpoint: endpoint.clone(),
        })?
        .to_str()
        .map_err(|err| ApiError::CookieStore(format!("invalid Location header: {err:?}")))?;

    endpoint
        .join(location)
        .map_err(|source| ApiError::InvalidLocationHeader {
            endpoint,
            location: location.to_owned(),
            source,
        })
}

pub(crate) fn text_snippet(value: &str) -> String {
    limited_text_snippet(value, 2048)
}

pub(crate) fn limited_text_snippet(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

pub(super) async fn response_text_snippet(res: Response) -> Option<String> {
    let content_type = res
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if !content_type.is_empty()
        && !content_type.contains("json")
        && !content_type.contains("text")
        && !content_type.contains("html")
    {
        return None;
    }

    res.text().await.ok().map(|body| text_snippet(&body))
}

pub(super) fn decode_basic_html_entities(value: &str) -> String {
    value
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#039;", "'")
}
