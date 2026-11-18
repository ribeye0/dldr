use super::*;

#[tokio::test]
async fn verification_marks_only_confirmed_missing_paths_and_never_deletes_files() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;
    let root = tempfile::tempdir()?;
    let library_root = root.path().join("library");
    let staging = root.path().join("staging");
    for id in ["RJ000001", "RJ000002"] {
        library
            .download_work_with_source(
                WorkDownloadRequest::new(id, &library_root, &staging),
                &FakeDownloadSource,
            )
            .await?;
    }
    std::fs::remove_dir_all(library_root.join("RJ000001"))?;
    let report = library.verify_downloaded_library(&library_root).await?;
    assert_eq!((report.checked, report.present, report.missing), (2, 1, 1));
    assert_eq!(
        library
            .storage
            .work_download_state("RJ000001")
            .await?
            .error_code
            .as_deref(),
        Some("local_missing")
    );
    assert_eq!(
        library
            .storage
            .work_download_state("RJ000002")
            .await?
            .status,
        WorkDownloadStatus::Downloaded
    );
    assert_eq!(
        std::fs::read(library_root.join("RJ000002/RJ000001.txt"))?,
        b"downloaded"
    );
    library.remove_account("account-a").await?;
    assert_eq!(
        library
            .list_products(&ProductListQuery::default())
            .await?
            .total_count,
        2,
        "missing local metadata must remain manageable without an account"
    );
    assert_eq!(
        library
            .list_products(&ProductListQuery {
                downloaded_only: true,
                ..Default::default()
            })
            .await?
            .total_count,
        1
    );
    assert!(library
        .verify_downloaded_library(&root.path().join("offline-volume"))
        .await
        .is_err());
    assert_eq!(
        library
            .storage
            .work_download_state("RJ000002")
            .await?
            .status,
        WorkDownloadStatus::Downloaded
    );
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn verification_reports_symlinks_without_changing_the_record_or_target() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;
    let root = tempfile::tempdir()?;
    let library_root = root.path().join("library");
    let staging = root.path().join("staging");
    library
        .download_work_with_source(
            WorkDownloadRequest::new("RJ000001", &library_root, &staging),
            &FakeDownloadSource,
        )
        .await?;
    let outside = root.path().join("outside");
    std::fs::rename(library_root.join("RJ000001"), &outside)?;
    std::os::unix::fs::symlink(&outside, library_root.join("RJ000001"))?;
    let before = library.storage.work_download_state("RJ000001").await?;
    let report = library.verify_downloaded_library(&library_root).await?;
    assert_eq!(
        (report.missing, report.issues[0].status.as_str()),
        (0, "symlink")
    );
    assert_eq!(
        library.storage.work_download_state("RJ000001").await?,
        before
    );
    assert_eq!(std::fs::read(outside.join("RJ000001.txt"))?, b"downloaded");
    Ok(())
}
