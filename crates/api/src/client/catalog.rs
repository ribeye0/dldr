//! Purchased library queries and public product metadata.
use super::response::{parse_json_body, parse_json_response, response_text_snippet};
use super::{DiagnosticRequest, DlsiteClient};
use crate::{
    raw::RawResponse, ApiError, ContentCount, ContentQuery, PublicWork, Purchase, Result, Work,
    WorkId, WorksResponse, DEFAULT_WORKS_BATCH_LIMIT,
};
use reqwest::{
    header::{HeaderMap, HeaderValue},
    StatusCode,
};
use serde_json::Value;
use url::Url;

pub(super) const CONTENT_COUNT_URL: &str = "https://play.dlsite.com/api/v3/content/count";
const CONTENT_SALES_URL: &str = "https://play.dlsite.com/api/v3/content/sales";
const CONTENT_WORKS_URL: &str = "https://play.dlsite.com/api/v3/content/works";
pub(super) const PUBLIC_PRODUCT_URL: &str = "https://www.dlsite.com/home/api/=/product.json";

impl DlsiteClient {
    pub async fn content_count(&self, query: ContentQuery) -> Result<ContentCount> {
        let endpoint = Url::parse(CONTENT_COUNT_URL)?;
        let mut request = self.http.get(endpoint.clone());

        if let Some(last) = query.last {
            request = request.query(&[("last", last)]);
        }

        let count = parse_json_response(request.diagnostic_send().await?).await?;
        self.cache_limits_from_count(&count).await;

        Ok(count)
    }

    pub async fn sales(&self, query: ContentQuery) -> Result<Vec<Purchase>> {
        let endpoint = Url::parse(CONTENT_SALES_URL)?;
        let mut request = self.http.get(endpoint.clone());

        if let Some(last) = query.last {
            request = request.query(&[("last", last)]);
        }

        parse_json_response(request.diagnostic_send().await?).await
    }

    pub async fn works(&self, ids: &[WorkId]) -> Result<Vec<Work>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        'load: loop {
            let limit = self.works_batch_limit().await;
            let mut works = Vec::new();

            for chunk in ids.chunks(limit) {
                match self.works_batch(chunk).await {
                    Ok(chunk_works) => works.extend(chunk_works),
                    Err(ApiError::BatchLimitExceeded { limit, .. }) if limit > 0 => {
                        self.set_works_batch_limit(limit).await;
                        continue 'load;
                    }
                    Err(err) => return Err(err),
                }
            }

            return Ok(works);
        }
    }

    pub async fn works_batch(&self, ids: &[WorkId]) -> Result<Vec<Work>> {
        let endpoint = Url::parse(CONTENT_WORKS_URL)?;
        let ids = ids
            .iter()
            .map(|id| id.as_ref().to_owned())
            .collect::<Vec<_>>();

        let res = self
            .http
            .post(endpoint)
            .json(&ids)
            .diagnostic_send()
            .await?;
        let status = res.status();

        if status == StatusCode::UNAUTHORIZED {
            return Err(ApiError::NotAuthorized);
        }

        if !status.is_success() {
            let endpoint = res.url().clone();
            let headers = res.headers().clone();
            let body_snippet = response_text_snippet(res).await;

            if let Some(limit) =
                detect_works_batch_limit(&headers, body_snippet.as_deref().unwrap_or_default())
            {
                return Err(ApiError::BatchLimitExceeded {
                    limit,
                    body_snippet,
                });
            }

            return Err(ApiError::UnexpectedStatus {
                endpoint,
                status,
                body_snippet,
            });
        }

        let endpoint = res.url().clone();
        let body = res.text().await?;
        let response = parse_json_body::<WorksResponse>(endpoint, &body)?;

        Ok(response.works)
    }

    pub async fn raw_works_batch(&self, ids: &[WorkId]) -> Result<RawResponse> {
        self.raw_works_batch_with_body_limit(ids, 2048).await
    }

    pub async fn public_work(&self, id: &WorkId) -> Result<Option<PublicWork>> {
        let endpoint = Url::parse(PUBLIC_PRODUCT_URL)?;
        let products = parse_json_response::<Vec<PublicWork>>(
            self.http
                .get(endpoint)
                .query(&[("workno", id.as_ref())])
                .diagnostic_send()
                .await?,
        )
        .await?;

        Ok(products
            .into_iter()
            .find(|product| product.id.as_ref() == id.as_ref()))
    }

    pub async fn public_works(&self, ids: &[WorkId]) -> Result<Vec<PublicWork>> {
        let mut works = Vec::new();

        for id in ids {
            if let Some(work) = self.public_work(id).await? {
                works.push(work);
            }
        }

        Ok(works)
    }

    pub async fn raw_works_batch_with_body_limit(
        &self,
        ids: &[WorkId],
        body_limit: usize,
    ) -> Result<RawResponse> {
        let endpoint = Url::parse(CONTENT_WORKS_URL)?;
        let ids = ids
            .iter()
            .map(|id| id.as_ref().to_owned())
            .collect::<Vec<_>>();
        let res = self
            .http
            .post(endpoint)
            .json(&ids)
            .diagnostic_send()
            .await?;

        RawResponse::from_response_with_body_limit(res, body_limit).await
    }

    async fn works_batch_limit(&self) -> usize {
        self.works_batch_limit
            .lock()
            .await
            .unwrap_or(DEFAULT_WORKS_BATCH_LIMIT)
    }

    async fn set_works_batch_limit(&self, limit: usize) {
        *self.works_batch_limit.lock().await = Some(limit);
    }

    pub(super) async fn cache_limits_from_count(&self, count: &ContentCount) {
        if let Some(limit) = count.page_limit.filter(|limit| *limit > 0) {
            self.set_works_batch_limit(limit).await;
        }
    }
}

pub fn detect_works_batch_limit(headers: &HeaderMap, body: &str) -> Option<usize> {
    const HEADER_KEYS: &[&str] = &[
        "x-batch-limit",
        "x-max-batch-size",
        "x-works-batch-limit",
        "x-dlsite-batch-limit",
    ];

    for key in HEADER_KEYS {
        if let Some(limit) = headers.get(*key).and_then(header_value_to_usize) {
            return Some(limit);
        }
    }

    let body = body.trim();
    if body.is_empty() {
        return None;
    }

    let json = serde_json::from_str::<Value>(body).ok()?;
    find_batch_limit_in_json(&json)
}

fn find_batch_limit_in_json(value: &Value) -> Option<usize> {
    const JSON_KEYS: &[&str] = &[
        "expected_batch_size",
        "batch_size",
        "batchSize",
        "max_batch_size",
        "maxBatchSize",
        "batch_limit",
        "batchLimit",
        "page_limit",
        "pageLimit",
    ];

    match value {
        Value::Object(map) => {
            for key in JSON_KEYS {
                if let Some(limit) = map
                    .get(*key)
                    .and_then(|value| value.as_u64())
                    .and_then(|value| usize::try_from(value).ok())
                {
                    return Some(limit);
                }
            }

            map.values().find_map(find_batch_limit_in_json)
        }
        Value::Array(values) => values.iter().find_map(find_batch_limit_in_json),
        _ => None,
    }
}

fn header_value_to_usize(value: &HeaderValue) -> Option<usize> {
    value.to_str().ok()?.parse().ok()
}
