use super::*;
use crate::*;
#[test]
fn wrapped_download_and_archive_causes_survive_persistence_and_export() {
    use dldr_archive::ArchiveError;
    use dldr_audit::{AuditEvent, AuditLogger, ExportScope};
    use std::io::Read;
    let dir = tempfile::tempdir().unwrap();
    let logger = AuditLogger::new(dir.path()).unwrap();
    let failures = [
        dldr_download::DownloadError::Io(std::io::Error::from_raw_os_error(13)),
        dldr_download::DownloadError::Io(std::io::Error::from_raw_os_error(28)),
        dldr_download::DownloadError::Archive(ArchiveError::Io(std::io::Error::from_raw_os_error(
            28,
        ))),
        dldr_download::DownloadError::Archive(ArchiveError::UnsafeArchiveEntry {
            entry: "PRIVATE-ENTRY".into(),
        }),
        dldr_download::DownloadError::Archive(ArchiveError::Zip(
            zip::result::ZipError::InvalidArchive("PRIVATE-ARCHIVE-DETAIL".into()),
        )),
        dldr_download::DownloadError::Archive(ArchiveError::Zip(zip::result::ZipError::Io(
            std::io::Error::from_raw_os_error(13),
        ))),
    ];
    for error in failures {
        logger.record_now(
            AuditEvent::failed("work.download", "")
                .with_details(LibraryError::Download(error).support_details()),
        );
    }
    assert!(logger.flush(std::time::Duration::from_secs(2)));
    let persisted = std::fs::read_to_string(
        dir.path()
            .join("diagnostics")
            .join(logger.run_id())
            .join("events-0001.jsonl"),
    )
    .unwrap();
    let destination = dir.path().join("support.zip");
    logger.export(&destination, ExportScope::default()).unwrap();
    let mut archive = zip::ZipArchive::new(std::fs::File::open(destination).unwrap()).unwrap();
    let mut exported = String::new();
    archive
        .by_name("events.jsonl")
        .unwrap()
        .read_to_string(&mut exported)
        .unwrap();
    for data in [persisted, exported] {
        assert!(!data.contains("PRIVATE-"));
        let events: Vec<dldr_audit::AuditEvent> = data
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(events.len(), 6);
        assert_eq!(events[0].details["osCode"], 13);
        assert_eq!(events[1].details["osCode"], 28);
        assert_eq!(events[2].details["archiveErrorKind"], "io");
        assert_eq!(events[2].details["osCode"], 28);
        assert_eq!(events[3].details["archiveErrorKind"], "unsafe_entry");
        assert_eq!(events[4].details["archiveErrorKind"], "zip_invalid");
        assert_eq!(events[5].details["archiveErrorKind"], "zip_io");
        assert_eq!(events[5].details["osCode"], 13);
    }
}

#[test]
fn html_download_failures_keep_a_safe_reason_and_actionable_hint() {
    use dldr_audit::{AuditEvent, AuditLogger};
    use dldr_download::{DownloadError, InvalidDownloadResponseReason};
    let directory = tempfile::tempdir().unwrap();
    let logger = AuditLogger::new(directory.path()).unwrap();
    logger.record_now(AuditEvent::failed("work.download", "PRIVATE-MESSAGE").with_details(
        serde_json::json!({"errorCode":"download", "errorDetails": LibraryError::Download(DownloadError::InvalidDownloadResponse {
            reason: InvalidDownloadResponseReason::HtmlContentType {content_type:"text/html; PRIVATE-HEADER".into()},
        }).support_details()})
    ));
    let event = logger
        .events_for_run(None, 500, None)
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(
        event.details["errorDetails"]["responseKind"],
        "html_content_type"
    );
    assert!(event.error_message.unwrap().contains("HTML page"));
    assert!(event.hint.unwrap().contains("Sign in again"));
    assert!(!event.details.to_string().contains("PRIVATE-"));
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
