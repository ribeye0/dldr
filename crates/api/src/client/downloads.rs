//! Download plans, storefront pages and ranged file streams.
use super::auth::LOGIN_HOST;
use super::response::{decode_basic_html_entities, redirect_location, response_text_snippet};
use super::{DiagnosticRequest, DlsiteClient};
use crate::{
    raw::RawResponse, ApiError, DownloadByteRange, DownloadFile, DownloadFileKind, DownloadPlan,
    DownloadResolution, DownloadStreamRequest, DownloadUnavailableReason, Result,
    SerialDownloadPage, SerialNumber, SplitDownloadPage, SplitDownloadPart, WorkId,
};
use bytes::Bytes;
use futures_core::Stream;
use reqwest::{header::RANGE, Response, StatusCode};
use std::collections::BTreeMap;
use url::Url;

pub(super) const DOWNLOAD_URL: &str = "https://play.dlsite.com/api/v3/download";
const HOME_SERIAL_URL: &str = "https://www.dlsite.com/home/serial/=/product_id/";
const MAX_DOWNLOAD_REDIRECTS: usize = 8;
const DOWNLOAD_PAGE_BODY_LIMIT: usize = 512 * 1024;

impl DlsiteClient {
    pub async fn resolve_download(&self, work_id: &WorkId) -> Result<DownloadResolution> {
        Ok(self.probe_download(work_id).await?.resolution)
    }

    pub async fn probe_download(&self, work_id: &WorkId) -> Result<DownloadProbe> {
        let initial = self.raw_download_probe(work_id).await?;
        let resolution = download_resolution_from_raw_response(&initial);

        Ok(DownloadProbe {
            work_id: work_id.clone(),
            initial,
            resolution,
        })
    }

    pub async fn download_plan(&self, work_id: &WorkId) -> Result<DownloadPlan> {
        match self.resolve_download(work_id).await? {
            DownloadResolution::Direct { stream_request } => {
                let serial_page = self.optional_serial_download_page(work_id).await?;
                let (stream_request, serial_numbers) = match serial_page {
                    Some(page) => (page.stream_request, page.serial_numbers),
                    None => (stream_request, Vec::new()),
                };

                Ok(DownloadPlan {
                    work_id: work_id.clone(),
                    files: vec![DownloadFile {
                        kind: DownloadFileKind::Direct,
                        stream_request,
                    }],
                    serial_numbers,
                })
            }
            DownloadResolution::Split { location } => {
                let page = self.split_download_page(location).await?;
                Ok(DownloadPlan {
                    work_id: work_id.clone(),
                    files: page
                        .parts
                        .into_iter()
                        .map(|part| DownloadFile {
                            kind: DownloadFileKind::SplitPart {
                                number: part.number,
                            },
                            stream_request: part.stream_request,
                        })
                        .collect(),
                    serial_numbers: Vec::new(),
                })
            }
            DownloadResolution::SerialRequired { location } => {
                let page = self.serial_download_page(location).await?;
                Ok(DownloadPlan {
                    work_id: work_id.clone(),
                    files: vec![DownloadFile {
                        kind: DownloadFileKind::Direct,
                        stream_request: page.stream_request,
                    }],
                    serial_numbers: page.serial_numbers,
                })
            }
            DownloadResolution::UnknownRedirect { location } => {
                Err(ApiError::DownloadUnknownRedirect {
                    work_id: work_id.clone(),
                    location,
                })
            }
            DownloadResolution::Unavailable { reason } => Err(ApiError::DownloadUnavailable {
                work_id: work_id.clone(),
                reason,
            }),
        }
    }

    pub async fn open_download_stream(
        &self,
        request: &DownloadStreamRequest,
        range: Option<DownloadByteRange>,
    ) -> Result<DownloadStream> {
        let mut url = request.url.clone();

        for _ in 0..MAX_DOWNLOAD_REDIRECTS {
            let mut builder = self.http.get(url.clone());

            if let Some(range) = range {
                builder = builder.header(RANGE, range.header_value());
            }

            let res = builder.diagnostic_send().await?;
            let status = res.status();

            if status.is_redirection() {
                url = redirect_location(&res)?;
                // Play and the storefront can expire independently. Following this
                // redirect with Range would turn a login page into a 206 response.
                if is_download_login_url(&url) {
                    return Err(ApiError::NotAuthorized);
                }
                continue;
            }

            if status == StatusCode::UNAUTHORIZED {
                return Err(ApiError::NotAuthorized);
            }

            if status != StatusCode::OK && status != StatusCode::PARTIAL_CONTENT {
                let endpoint = res.url().clone();
                let body_snippet = response_text_snippet(res).await;
                return Err(ApiError::UnexpectedStatus {
                    endpoint,
                    status,
                    body_snippet,
                });
            }

            return Ok(DownloadStream { response: res });
        }

        Err(ApiError::RedirectLimitExceeded {
            endpoint: url,
            limit: MAX_DOWNLOAD_REDIRECTS,
        })
    }

    pub async fn optional_serial_download_page(
        &self,
        work_id: &WorkId,
    ) -> Result<Option<SerialDownloadPage>> {
        let location = serial_page_url(work_id)?;
        let raw = self
            .raw_get_with_body_limit(location.clone(), DOWNLOAD_PAGE_BODY_LIMIT)
            .await?;

        if is_absent_optional_serial_page_status(raw.status) {
            return Ok(None);
        }

        parse_serial_download_page_from_raw(location, raw).map(Some)
    }

    pub async fn split_download_page(&self, location: Url) -> Result<SplitDownloadPage> {
        let raw = self
            .raw_get_with_body_limit(location.clone(), DOWNLOAD_PAGE_BODY_LIMIT)
            .await?;
        check_download_page_authorization(&raw)?;
        let body = raw.body_snippet.as_deref().unwrap_or_default();
        let parts = parse_split_download_parts(&location, body);

        if parts.is_empty() {
            return Err(ApiError::DownloadPageLinkNotFound {
                page: location,
                kind: "split",
            });
        }

        Ok(SplitDownloadPage {
            page_url: raw.url,
            parts,
        })
    }

    pub async fn serial_download_page(&self, location: Url) -> Result<SerialDownloadPage> {
        let raw = self
            .raw_get_with_body_limit(location.clone(), DOWNLOAD_PAGE_BODY_LIMIT)
            .await?;

        parse_serial_download_page_from_raw(location, raw)
    }

    pub async fn raw_download_probe(&self, work_id: &WorkId) -> Result<RawResponse> {
        self.raw_download_probe_with_body_limit(work_id, 2048).await
    }

    pub async fn raw_download_probe_with_body_limit(
        &self,
        work_id: &WorkId,
        body_limit: usize,
    ) -> Result<RawResponse> {
        let res = self
            .http
            .get(DOWNLOAD_URL)
            .query(&[("workno", work_id.as_ref())])
            .diagnostic_send()
            .await?;

        RawResponse::from_response_with_body_limit(res, body_limit).await
    }
}

fn parse_serial_download_page_from_raw(
    location: Url,
    raw: RawResponse,
) -> Result<SerialDownloadPage> {
    check_download_page_authorization(&raw)?;
    if !(200..=299).contains(&raw.status) {
        return Err(ApiError::UnexpectedStatus {
            endpoint: raw.url,
            status: StatusCode::from_u16(raw.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            body_snippet: raw.body_snippet,
        });
    }

    let body = raw.body_snippet.as_deref().unwrap_or_default();
    let serial_numbers = parse_serial_numbers(body);

    if let Some(stream_request) = parse_serial_download_link(&location, body) {
        return Ok(SerialDownloadPage {
            page_url: raw.url,
            serial_numbers,
            stream_request,
        });
    }

    Err(ApiError::DownloadPageLinkNotFound {
        page: location,
        kind: "serial",
    })
}

pub(super) fn is_absent_optional_serial_page_status(status: u16) -> bool {
    status == StatusCode::NOT_FOUND.as_u16()
        || status == StatusCode::INTERNAL_SERVER_ERROR.as_u16()
        || (300..=399).contains(&status)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadProbe {
    pub work_id: WorkId,
    pub initial: RawResponse,
    pub resolution: DownloadResolution,
}

pub struct DownloadStream {
    response: Response,
}

impl DownloadStream {
    pub fn url(&self) -> &Url {
        self.response.url()
    }

    pub fn status(&self) -> StatusCode {
        self.response.status()
    }

    pub fn content_length(&self) -> Option<u64> {
        self.response.content_length()
    }

    pub fn headers(&self) -> BTreeMap<String, String> {
        self.response
            .headers()
            .iter()
            .filter_map(|(key, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|value| (key.as_str().to_owned(), value.to_owned()))
            })
            .collect()
    }

    pub async fn next_chunk(&mut self) -> std::result::Result<Option<Bytes>, reqwest::Error> {
        self.response.chunk().await
    }

    pub fn into_bytes_stream(
        self,
    ) -> impl Stream<Item = std::result::Result<Bytes, reqwest::Error>> {
        self.response.bytes_stream()
    }
}

pub(super) fn download_resolution_from_raw_response(raw: &RawResponse) -> DownloadResolution {
    match raw.status {
        300..=399 => match raw.location.clone() {
            Some(location) if is_download_login_url(&location) => DownloadResolution::Unavailable {
                reason: DownloadUnavailableReason::NotAuthorized,
            },
            Some(location) => DownloadResolution::from_redirect_location(location),
            None => DownloadResolution::Unavailable {
                reason: DownloadUnavailableReason::UnexpectedStatus {
                    status: raw.status,
                    body_snippet: raw.body_snippet.clone(),
                },
            },
        },
        401 => DownloadResolution::Unavailable {
            reason: DownloadUnavailableReason::NotAuthorized,
        },
        404 => DownloadResolution::Unavailable {
            reason: DownloadUnavailableReason::NotFound,
        },
        status => DownloadResolution::Unavailable {
            reason: DownloadUnavailableReason::UnexpectedStatus {
                status,
                body_snippet: raw.body_snippet.clone(),
            },
        },
    }
}

fn is_download_login_url(url: &Url) -> bool {
    url.host_str() == Some(LOGIN_HOST)
        || (url.host_str() == Some("www.dlsite.com")
            && (url.path() == "/home/login" || url.path().starts_with("/home/login/")))
}

pub(super) fn check_download_page_authorization(raw: &RawResponse) -> Result<()> {
    if raw.status == 401
        || raw.location.as_ref().is_some_and(is_download_login_url)
        || is_download_login_url(&raw.url)
    {
        return Err(ApiError::NotAuthorized);
    }
    Ok(())
}

pub(super) fn parse_split_download_parts(page_url: &Url, body: &str) -> Vec<SplitDownloadPart> {
    let mut parts = extract_download_links(page_url, body)
        .into_iter()
        .filter_map(|url| {
            let number = download_part_number(&url)?;
            Some(SplitDownloadPart {
                number,
                stream_request: DownloadStreamRequest { url },
            })
        })
        .collect::<Vec<_>>();

    parts.sort_by(|left, right| {
        left.number
            .cmp(&right.number)
            .then_with(|| left.stream_request.url.cmp(&right.stream_request.url))
    });
    parts.dedup_by(|left, right| {
        left.number == right.number && left.stream_request.url == right.stream_request.url
    });
    parts
}

pub(super) fn parse_serial_download_link(
    page_url: &Url,
    body: &str,
) -> Option<DownloadStreamRequest> {
    extract_download_links(page_url, body)
        .into_iter()
        .find(|url| {
            matches!(
                DownloadResolution::from_redirect_location(url.clone()),
                DownloadResolution::Direct { .. }
            )
        })
        .map(|url| DownloadStreamRequest { url })
}

pub(super) fn parse_serial_numbers(body: &str) -> Vec<SerialNumber> {
    let Some(section_start) = body.find("<h2>シリアル番号</h2>") else {
        return Vec::new();
    };
    let section = &body[section_start..];
    let Some(table_start) = section.find("<table") else {
        return Vec::new();
    };
    let section = &section[table_start..];
    let table_end = section.find("</table>").unwrap_or(section.len());
    let table = &section[..table_end];
    let mut serial_numbers = Vec::new();
    let mut rest = table;

    while let Some(row_start) = rest.find("<tr") {
        rest = &rest[row_start..];
        let Some(row_end) = rest.find("</tr>") else {
            break;
        };
        let row = &rest[..row_end];

        if let Some(serial_number) = parse_serial_number_row(row) {
            serial_numbers.push(serial_number);
        }

        rest = &rest[row_end + "</tr>".len()..];
    }

    serial_numbers
}

fn parse_serial_number_row(row: &str) -> Option<SerialNumber> {
    let label = extract_element_text(row, "th")?;

    if !label.contains("シリアル") && !label.to_ascii_lowercase().contains("serial") {
        return None;
    }

    let value = extract_element_text(row, "td")?;

    if value.is_empty() {
        return None;
    }

    Some(SerialNumber { label, value })
}

fn extract_element_text(html: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let start = html.find(&open)?;
    let after_open = html[start..].find('>')? + start + 1;
    let end = html[after_open..].find(&close)? + after_open;
    let text = strip_html_tags(&html[after_open..end]);
    let text = decode_basic_html_entities(&text);
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");

    Some(text)
}

fn strip_html_tags(html: &str) -> String {
    let mut output = String::new();
    let mut in_tag = false;

    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => output.push(ch),
            _ => {}
        }
    }

    output
}

fn extract_download_links(page_url: &Url, body: &str) -> Vec<Url> {
    let mut links = extract_quoted_values(body)
        .into_iter()
        .filter_map(|value| page_url.join(&value).ok())
        .filter(|url| {
            let host = url.host_str().unwrap_or_default();
            let path = url.path();
            host == "www.dlsite.com"
                && path.starts_with("/home/download")
                && path.contains("product_id")
        })
        .collect::<Vec<_>>();

    links.sort();
    links.dedup();
    links
}

fn extract_quoted_values(body: &str) -> Vec<String> {
    let mut values = Vec::new();

    for quote in ['"', '\''] {
        let mut start = None;

        for (index, value) in body.char_indices() {
            if value != quote {
                continue;
            }

            if let Some(open) = start.take() {
                values.push(body[open..index].to_owned());
            } else {
                start = Some(index + value.len_utf8());
            }
        }
    }

    values
}

fn download_part_number(url: &Url) -> Option<u32> {
    let mut segments = url.path_segments()?;

    while let Some(segment) = segments.next() {
        if segment == "number" {
            return segments.next()?.parse().ok();
        }
    }

    None
}

pub(super) fn serial_page_url(work_id: &WorkId) -> Result<Url> {
    Url::parse(&format!("{HOME_SERIAL_URL}{work_id}.html")).map_err(Into::into)
}
