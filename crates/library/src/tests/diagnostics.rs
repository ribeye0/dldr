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
