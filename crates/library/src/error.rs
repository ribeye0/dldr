use crate::{BulkWorkDownloadReport, MAX_PARALLEL_WORK_DOWNLOADS};
use dldr_api::ApiError;
use dldr_credentials::CredentialsError;
use dldr_storage::StorageError;
use serde_json::{json, Map, Value};
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum LibraryError {
    #[error("parallel downloads per batch must be between 1 and {MAX_PARALLEL_WORK_DOWNLOADS}")]
    InvalidDownloadConcurrency,
    #[error("Work paused or cancelled individually")]
    QueueStopped,
    #[error("{0}")]
    InsufficientSpace(String),
    #[error("storage error")]
    Storage(#[from] StorageError),
    #[error("credential error: {0}")]
    Credentials(#[from] CredentialsError),
    #[error("dlsite api error")]
    Api(#[from] ApiError),
    #[error("sync source error: {0}")]
    SyncSource(String),
    #[error("account not found: {0}")]
    AccountNotFound(String),
    #[error("account is disabled: {0}")]
    AccountDisabled(String),
    #[error("account has no login name: {0}")]
    MissingLoginName(String),
    #[error("account has no available password: {0}")]
    MissingPassword(String),
    #[error("two-factor verification is required but no prompt is available: {0}")]
    TwoFactorPromptUnavailable(String),
    #[error("two-factor verification was cancelled: {0}")]
    TwoFactorCancelled(String),
    #[error("two-factor verification timed out: {0}")]
    TwoFactorTimedOut(String),
    #[error("two-factor verification failed after {attempts} attempts: {account_id}")]
    TwoFactorRejected { account_id: String, attempts: u32 },
    #[error("sync was cancelled")]
    Cancelled,
    #[error("bulk download interrupted: {cause}")]
    BulkInterrupted {
        cause: Box<LibraryError>,
        report: Box<BulkWorkDownloadReport>,
        interrupted_work: Option<String>,
    },
    #[error("download error")]
    Download(#[from] dldr_download::DownloadError),
    #[error("work is not owned by an enabled account: {0}")]
    DownloadAccountNotFound(String),
    #[error("download final path already exists: {0}")]
    DownloadTargetExists(PathBuf),
    #[error("download path is outside configured roots: {0}")]
    DownloadPathOutsideRoots(PathBuf),
    #[error("download path is not a directory: {0}")]
    DownloadPathNotDirectory(PathBuf),
    #[error("download recovery required: {0}")]
    RecoveryRequired(String),
    #[error("download attempt left recoverable data at {retained_path}")]
    RetainedDownload {
        source: Box<LibraryError>,
        retained_path: PathBuf,
    },
    #[error("I/O error")]
    Io(#[from] std::io::Error),
    #[error("json error")]
    Json(#[from] serde_json::Error),
}

impl LibraryError {
    pub fn is_cancelled(&self) -> bool {
        match self {
            Self::Cancelled
            | Self::QueueStopped
            | Self::TwoFactorCancelled(_)
            | Self::Download(dldr_download::DownloadError::Cancelled) => true,
            Self::BulkInterrupted { cause, .. } => cause.is_cancelled(),
            Self::RetainedDownload { source, .. } => source.is_cancelled(),
            _ => false,
        }
    }

    pub fn failure_code(&self) -> &'static str {
        if self.is_cancelled() {
            return "cancelled";
        }
        match self {
            Self::BulkInterrupted { cause, .. } => cause.failure_code(),
            Self::RetainedDownload { source, .. } => source.failure_code(),
            Self::RecoveryRequired(_) => "download_recovery_required",
            Self::InvalidDownloadConcurrency => "invalid_download_concurrency",
            Self::QueueStopped => "cancelled",
            Self::InsufficientSpace(_) => "insufficient_space",
            Self::Storage(_) => "storage",
            Self::Credentials(_) => "credentials",
            Self::Api(_) => "api",
            Self::SyncSource(_) => "sync_source",
            Self::AccountNotFound(_) => "account_not_found",
            Self::AccountDisabled(_) => "account_disabled",
            Self::MissingLoginName(_) => "missing_login_name",
            Self::MissingPassword(_) => "missing_password",
            Self::TwoFactorPromptUnavailable(_) => "two_factor_prompt_unavailable",
            // Reported as a plain cancellation so `dldr-jobs` finishes the job as Cancelled
            // rather than Failed; `support_details` keeps the two-factor specifics.
            Self::TwoFactorCancelled(_) => "cancelled",
            Self::TwoFactorTimedOut(_) => "two_factor_timed_out",
            Self::TwoFactorRejected { .. } => "two_factor_rejected",
            Self::Cancelled => "cancelled",
            Self::Download(_) => "download",
            Self::DownloadAccountNotFound(_) => "download_account_not_found",
            Self::DownloadTargetExists(_) => "download_target_exists",
            Self::DownloadPathOutsideRoots(_) => "download_path_outside_roots",
            Self::DownloadPathNotDirectory(_) => "download_path_not_directory",
            Self::Io(_) => "io",
            Self::Json(_) => "json",
        }
    }

    pub fn support_message(&self) -> String {
        match self {
            Self::BulkInterrupted { cause, .. } => cause.support_message(),
            Self::RetainedDownload { source, retained_path } => format!("{}; recoverable data retained at {}", source.support_message(), retained_path.display()),
            Self::RecoveryRequired(message) => message.clone(),
            Self::InvalidDownloadConcurrency | Self::QueueStopped | Self::InsufficientSpace(_) => self.to_string(),
            Self::Storage(error) => format!("Storage error: {error}"),
            Self::Credentials(error) => format!("Credential error: {error}"),
            Self::Api(error) => dldr_api_error_support_message(error),
            Self::SyncSource(error) => format!("Sync source error: {error}"),
            Self::AccountNotFound(account_id) => format!("Account not found: {account_id}"),
            Self::AccountDisabled(account_id) => format!("Account is disabled: {account_id}"),
            Self::MissingLoginName(account_id) => {
                format!("Account has no login name: {account_id}")
            }
            Self::MissingPassword(account_id) => {
                format!("Account has no saved password: {account_id}")
            }
            Self::TwoFactorPromptUnavailable(account_id) => format!(
                "Account {account_id} needs a two-factor code, but this operation cannot ask for one"
            ),
            Self::TwoFactorCancelled(account_id) => {
                format!("Two-factor verification was cancelled for account {account_id}")
            }
            Self::TwoFactorTimedOut(account_id) => format!(
                "No two-factor code was entered for account {account_id} before the prompt expired"
            ),
            Self::TwoFactorRejected {
                account_id,
                attempts,
            } => format!(
                "DLsite rejected the two-factor code for account {account_id} after {attempts} attempts"
            ),
            Self::Cancelled => "Operation was cancelled".to_owned(),
            Self::Download(error) => download_error_support_message(error),
            Self::DownloadAccountNotFound(work_id) => {
                format!("Work is not owned by an enabled account: {work_id}")
            }
            Self::DownloadTargetExists(path) => {
                format!("Download final path already exists: {}", path.display())
            }
            Self::DownloadPathOutsideRoots(path) => {
                format!(
                    "Download path is outside configured roots: {}",
                    path.display()
                )
            }
            Self::DownloadPathNotDirectory(path) => {
                format!("Download path is not a directory: {}", path.display())
            }
            Self::Io(error) => format!("I/O error: {error}"),
            Self::Json(error) => format!("JSON error: {error}"),
        }
    }

    pub fn support_details(&self) -> Value {
        match self {
            Self::BulkInterrupted { cause, .. } => cause.support_details(),
            Self::RetainedDownload {
                source,
                retained_path,
            } => {
                let mut details = value_object(source.support_details());
                details.insert("retainedPath".to_owned(), json!(retained_path));
                Value::Object(details)
            }
            Self::QueueStopped | Self::InsufficientSpace(_) => json!({"message": self.to_string()}),
            Self::InvalidDownloadConcurrency => {
                json!({"maxConcurrentWorks": MAX_PARALLEL_WORK_DOWNLOADS})
            }
            Self::RecoveryRequired(message) => {
                json!({ "failureKind": "download_recovery_required", "message": message })
            }
            Self::Storage(error) => error.support_details(),
            Self::Credentials(error) => json!({
                "failureKind": "credentials",
                "message": error.to_string(),
            }),
            Self::Api(error) => dldr_api_error_support_details(error),
            Self::SyncSource(error) => json!({
                "failureKind": "sync_source",
                "message": error,
            }),
            Self::AccountNotFound(account_id) => json!({
                "failureKind": "account_not_found",
                "accountId": account_id,
            }),
            Self::AccountDisabled(account_id) => json!({
                "failureKind": "account_disabled",
                "accountId": account_id,
            }),
            Self::MissingLoginName(account_id) => json!({
                "failureKind": "missing_login_name",
                "accountId": account_id,
            }),
            Self::MissingPassword(account_id) => json!({
                "failureKind": "missing_password",
                "accountId": account_id,
            }),
            Self::TwoFactorPromptUnavailable(account_id) => json!({
                "failureKind": "two_factor_prompt_unavailable",
                "accountId": account_id,
            }),
            Self::TwoFactorCancelled(account_id) => json!({
                "failureKind": "two_factor_cancelled",
                "accountId": account_id,
            }),
            Self::TwoFactorTimedOut(account_id) => json!({
                "failureKind": "two_factor_timed_out",
                "accountId": account_id,
            }),
            Self::TwoFactorRejected {
                account_id,
                attempts,
            } => json!({
                "failureKind": "two_factor_rejected",
                "accountId": account_id,
                "attempts": attempts,
            }),
            Self::Cancelled => json!({
                "failureKind": "cancelled",
            }),
            Self::Download(error) => download_error_support_details(error),
            Self::DownloadAccountNotFound(work_id) => json!({
                "failureKind": "download_account_not_found",
                "workId": work_id,
            }),
            Self::DownloadTargetExists(path) => json!({
                "failureKind": "download_target_exists",
                "path": path.to_string_lossy(),
            }),
            Self::DownloadPathOutsideRoots(path) => json!({
                "failureKind": "download_path_outside_roots",
                "path": path.to_string_lossy(),
            }),
            Self::DownloadPathNotDirectory(path) => json!({
                "failureKind": "download_path_not_directory",
                "path": path.to_string_lossy(),
            }),
            Self::Io(error) => json!({
                "failureKind": "io",
                "osCode": error.raw_os_error(),
                "message": error.to_string(),
            }),
            Self::Json(error) => json!({
                "failureKind": "json",
                "message": error.to_string(),
            }),
        }
    }
}

pub(super) fn dldr_api_error_support_message(error: &ApiError) -> String {
    match error {
        ApiError::InvalidCredentials => "DLsite rejected the account credentials".to_owned(),
        ApiError::NotAuthorized => "DLsite session is not authorized".to_owned(),
        ApiError::TwoFactorRequired => {
            "DLsite requires a two-factor verification code for this account".to_owned()
        }
        ApiError::InvalidTwoFactorCode => {
            "DLsite rejected the two-factor verification code".to_owned()
        }
        ApiError::TwoFactorFormNotRecognized { page, .. } => {
            format!(
                "DLsite two-factor challenge page could not be read at {}",
                safe_url_for_log(page.as_str())
            )
        }
        ApiError::XsrfTokenNotFound => "DLsite session did not provide an XSRF token".to_owned(),
        ApiError::LocationHeaderMissing { endpoint } => {
            format!(
                "DLsite redirect response did not include a Location header at {}",
                safe_url_for_log(endpoint.as_str())
            )
        }
        ApiError::InvalidLocationHeader { endpoint, .. } => {
            format!(
                "DLsite returned an invalid redirect Location header at {}",
                safe_url_for_log(endpoint.as_str())
            )
        }
        ApiError::UnexpectedStatus {
            endpoint, status, ..
        } => {
            format!(
                "DLsite returned HTTP {} from {}",
                status.as_u16(),
                safe_url_for_log(endpoint.as_str())
            )
        }
        ApiError::UnexpectedResponse { endpoint, .. } => {
            format!(
                "DLsite returned an unexpected response from {}",
                safe_url_for_log(endpoint.as_str())
            )
        }
        ApiError::UnexpectedJson { endpoint, path, .. } => {
            format!(
                "DLsite returned an unexpected JSON shape at {path} from {}",
                safe_url_for_log(endpoint.as_str())
            )
        }
        ApiError::RedirectLimitExceeded { endpoint, limit } => {
            format!(
                "DLsite redirect limit exceeded after {limit} redirects from {}",
                safe_url_for_log(endpoint.as_str())
            )
        }
        ApiError::DownloadPageLinkNotFound { kind, page } => {
            format!(
                "DLsite {kind} download page did not contain an expected download link at {}",
                safe_url_for_log(page.as_str())
            )
        }
        ApiError::DownloadUnavailable { work_id, reason } => {
            format!("DLsite reports download unavailable for {work_id}: {reason:?}")
        }
        ApiError::DownloadUnknownRedirect { work_id, location } => {
            format!(
                "DLsite returned an unknown download redirect for {work_id}: {}",
                safe_url_for_log(location.as_str())
            )
        }
        ApiError::BatchLimitExceeded { limit, .. } => {
            format!("DLsite works batch limit exceeded; detected limit: {limit}")
        }
        ApiError::CookieStore(error) => format!("DLsite cookie store error: {error}"),
        ApiError::Request(error) => format!("DLsite HTTP request failed: {error}"),
        ApiError::Url(error) => format!("DLsite URL parse failed: {error}"),
        ApiError::Json(error) => format!("DLsite JSON operation failed: {error}"),
    }
}

pub(super) fn dldr_api_error_support_details(error: &ApiError) -> Value {
    match error {
        ApiError::InvalidCredentials => json!({
            "failureKind": "api",
            "apiErrorKind": "invalid_credentials",
        }),
        ApiError::NotAuthorized => json!({
            "failureKind": "api",
            "apiErrorKind": "not_authorized",
        }),
        ApiError::TwoFactorRequired => json!({
            "failureKind": "api",
            "apiErrorKind": "two_factor_required",
        }),
        ApiError::InvalidTwoFactorCode => json!({
            "failureKind": "api",
            "apiErrorKind": "invalid_two_factor_code",
        }),
        ApiError::TwoFactorFormNotRecognized { page, .. } => json!({
            "failureKind": "api",
            "apiErrorKind": "two_factor_form_not_recognized",
            "page": safe_url_for_log(page.as_str()),
        }),
        ApiError::XsrfTokenNotFound => json!({
            "failureKind": "api",
            "apiErrorKind": "xsrf_token_not_found",
        }),
        ApiError::LocationHeaderMissing { endpoint } => json!({
            "failureKind": "api",
            "apiErrorKind": "location_header_missing",
            "endpoint": safe_url_for_log(endpoint.as_str()),
        }),
        ApiError::InvalidLocationHeader {
            endpoint,
            location,
            source,
        } => json!({
            "failureKind": "api",
            "apiErrorKind": "invalid_location_header",
            "endpoint": safe_url_for_log(endpoint.as_str()),
            "location": safe_url_for_log(location),
            "parseError": source.to_string(),
        }),
        ApiError::UnexpectedStatus {
            endpoint,
            status,
            body_snippet,
        } => {
            let mut details = Map::new();
            details.insert("failureKind".to_owned(), json!("api"));
            details.insert("apiErrorKind".to_owned(), json!("unexpected_status"));
            details.insert(
                "endpoint".to_owned(),
                json!(safe_url_for_log(endpoint.as_str())),
            );
            details.insert("httpStatus".to_owned(), json!(status.as_u16()));
            details.insert(
                "httpStatusText".to_owned(),
                json!(status.canonical_reason().unwrap_or("unknown")),
            );

            if let Some(snippet) = body_snippet.as_deref().and_then(safe_body_snippet_for_log) {
                details.insert("bodySnippet".to_owned(), json!(snippet));
            }

            Value::Object(details)
        }
        ApiError::UnexpectedResponse { endpoint, source } => json!({
            "failureKind": "api",
            "apiErrorKind": "unexpected_response",
            "endpoint": safe_url_for_log(endpoint.as_str()),
            "source": source.to_string(),
        }),
        ApiError::UnexpectedJson {
            endpoint,
            path,
            source,
        } => json!({
            "failureKind": "api",
            "apiErrorKind": "unexpected_json",
            "endpoint": safe_url_for_log(endpoint.as_str()),
            "jsonPath": path,
            "source": source.to_string(),
        }),
        ApiError::RedirectLimitExceeded { endpoint, limit } => json!({
            "failureKind": "api",
            "apiErrorKind": "redirect_limit_exceeded",
            "endpoint": safe_url_for_log(endpoint.as_str()),
            "limit": limit,
        }),
        ApiError::DownloadPageLinkNotFound { page, kind } => json!({
            "failureKind": "api",
            "apiErrorKind": "download_page_link_not_found",
            "page": safe_url_for_log(page.as_str()),
            "pageKind": kind,
        }),
        ApiError::DownloadUnavailable { work_id, reason } => json!({
            "failureKind": "api",
            "apiErrorKind": "download_unavailable",
            "workId": work_id.as_ref(),
            "reason": format!("{reason:?}"),
        }),
        ApiError::DownloadUnknownRedirect { work_id, location } => json!({
            "failureKind": "api",
            "apiErrorKind": "download_unknown_redirect",
            "workId": work_id.as_ref(),
            "location": safe_url_for_log(location.as_str()),
        }),
        ApiError::BatchLimitExceeded {
            limit,
            body_snippet,
        } => {
            let mut details = Map::new();
            details.insert("failureKind".to_owned(), json!("api"));
            details.insert("apiErrorKind".to_owned(), json!("batch_limit_exceeded"));
            details.insert("limit".to_owned(), json!(limit));

            if let Some(snippet) = body_snippet.as_deref().and_then(safe_body_snippet_for_log) {
                details.insert("bodySnippet".to_owned(), json!(snippet));
            }

            Value::Object(details)
        }
        ApiError::CookieStore(error) => json!({
            "failureKind": "api",
            "apiErrorKind": "cookie_store",
            "source": error,
        }),
        ApiError::Request(error) => {
            let mut details = Map::new();
            details.insert("failureKind".to_owned(), json!("api"));
            details.insert("apiErrorKind".to_owned(), json!("request"));
            details.insert("source".to_owned(), json!(error.to_string()));
            details.insert("isTimeout".to_owned(), json!(error.is_timeout()));
            details.insert("isConnect".to_owned(), json!(error.is_connect()));
            details.insert("isStatus".to_owned(), json!(error.is_status()));

            if let Some(status) = error.status() {
                details.insert("httpStatus".to_owned(), json!(status.as_u16()));
            }

            if let Some(url) = error.url() {
                details.insert("endpoint".to_owned(), json!(safe_url_for_log(url.as_str())));
            }

            Value::Object(details)
        }
        ApiError::Url(error) => json!({
            "failureKind": "api",
            "apiErrorKind": "url",
            "source": error.to_string(),
        }),
        ApiError::Json(error) => json!({
            "failureKind": "api",
            "apiErrorKind": "json",
            "source": error.to_string(),
        }),
    }
}

pub(super) fn download_error_support_message(error: &dldr_download::DownloadError) -> String {
    match error {
        dldr_download::DownloadError::Api(error) => {
            format!(
                "Download failed while calling DLsite: {}",
                dldr_api_error_support_message(error)
            )
        }
        dldr_download::DownloadError::Archive(error) => {
            format!("Archive extraction error: {error}")
        }
        dldr_download::DownloadError::Io(error) => format!("Download I/O error: {error}"),
        other => format!("Download error: {other}"),
    }
}

pub(super) fn download_error_support_details(error: &dldr_download::DownloadError) -> Value {
    match error {
        dldr_download::DownloadError::Cancelled => json!({
            "failureKind": "download",
            "downloadErrorKind": "cancelled",
        }),
        dldr_download::DownloadError::InvalidDownloadResponse { reason } => json!({
            "failureKind": "download",
            "downloadErrorKind": "invalid_response",
            "responseKind": match reason {
                dldr_download::InvalidDownloadResponseReason::HtmlContentType { .. } => "html_content_type",
                dldr_download::InvalidDownloadResponseReason::HtmlDocumentBody => "html_document_body",
            },
            "reason": reason.to_string(),
        }),
        dldr_download::DownloadError::FileNameUnknown { file_index } => json!({
            "failureKind": "download",
            "downloadErrorKind": "file_name_unknown",
            "fileIndex": file_index,
        }),
        dldr_download::DownloadError::InvalidFileName { file_name } => json!({
            "failureKind": "download",
            "downloadErrorKind": "invalid_file_name",
            "fileName": file_name,
        }),
        dldr_download::DownloadError::TargetAlreadyExists { path } => json!({
            "failureKind": "download",
            "downloadErrorKind": "target_already_exists",
            "path": path.to_string_lossy(),
        }),
        dldr_download::DownloadError::PlanWorkMismatch {
            job_work_id,
            plan_work_id,
        } => json!({
            "failureKind": "download",
            "downloadErrorKind": "plan_work_mismatch",
            "jobWorkId": job_work_id.as_ref(),
            "planWorkId": plan_work_id.as_ref(),
        }),
        dldr_download::DownloadError::IncompleteDownload { expected, actual } => json!({
            "failureKind": "download",
            "downloadErrorKind": "incomplete_download",
            "expectedBytes": expected,
            "actualBytes": actual,
        }),
        dldr_download::DownloadError::SizeExceeded { expected, actual } => json!({
            "failureKind": "download",
            "downloadErrorKind": "size_exceeded",
            "expectedBytes": expected,
            "actualBytes": actual,
        }),
        dldr_download::DownloadError::Stream(error) => json!({
            "failureKind": "download",
            "downloadErrorKind": "stream",
            "source": error,
        }),
        dldr_download::DownloadError::Api(error) => {
            let mut details = value_object(dldr_api_error_support_details(error));
            details.insert("downloadErrorKind".to_owned(), json!("api"));
            Value::Object(details)
        }
        dldr_download::DownloadError::Archive(error) => json!({
            "failureKind": "download",
            "downloadErrorKind": "archive",
            "archiveErrorKind": error.diagnostic_kind(),
            "osCode": error.os_error_code(),
            "source": error.to_string(),
        }),
        dldr_download::DownloadError::Io(error) => json!({
            "failureKind": "download",
            "downloadErrorKind": "io",
            "osCode": error.raw_os_error(),
            "source": error.to_string(),
        }),
    }
}

pub(super) fn value_object(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        other => {
            let mut map = Map::new();
            map.insert("detail".to_owned(), other);
            map
        }
    }
}

pub(super) fn safe_url_for_log(value: &str) -> String {
    let without_fragment = value.split_once('#').map_or(value, |(prefix, _)| prefix);
    let without_query = without_fragment
        .split_once('?')
        .map_or(without_fragment, |(prefix, _)| prefix);

    if without_query.contains("download.dlsite")
        || without_query.contains("/get/")
        || without_query.contains("/file/")
    {
        if let Some((origin, _)) = without_query.split_once("/get/") {
            return format!("{origin}/<download-url-redacted>");
        }

        if let Some((origin, _)) = without_query.split_once("/file/") {
            return format!("{origin}/<download-url-redacted>");
        }

        return "<download-url-redacted>".to_owned();
    }

    without_query.to_owned()
}

pub(super) fn safe_body_snippet_for_log(snippet: &str) -> Option<String> {
    let lower = snippet.to_ascii_lowercase();
    if lower.contains("serial") || snippet.contains("\u{30b7}\u{30ea}\u{30a2}\u{30eb}") {
        return Some("[redacted because response may contain serial information]".to_owned());
    }

    let collapsed = snippet.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return None;
    }

    let sanitized = redact_urls_in_text(&collapsed);

    Some(truncate_chars(&sanitized, 300))
}

pub(super) fn redact_urls_in_text(value: &str) -> String {
    let mut output = String::new();
    let mut rest = value;

    loop {
        let Some((start, marker_len)) = earliest_url_marker(rest) else {
            output.push_str(rest);
            return output;
        };

        output.push_str(&rest[..start]);
        output.push_str("[redacted-url]");

        let url_start = start + marker_len;
        let url_end = rest[url_start..]
            .char_indices()
            .find_map(|(index, ch)| {
                (ch.is_whitespace() || matches!(ch, '"' | '\'' | '<' | '>' | ')' | ']' | '}'))
                    .then_some(url_start + index)
            })
            .unwrap_or(rest.len());

        rest = &rest[url_end..];
    }
}

pub(super) fn earliest_url_marker(value: &str) -> Option<(usize, usize)> {
    [("https://", 8usize), ("http://", 7usize)]
        .into_iter()
        .filter_map(|(marker, len)| value.find(marker).map(|index| (index, len)))
        .min_by_key(|(index, _)| *index)
}

pub(super) fn truncate_chars(value: &str, max_chars: usize) -> String {
    let mut output = value.chars().take(max_chars).collect::<String>();

    if value.chars().count() > max_chars {
        output.push_str("...");
    }

    output
}
