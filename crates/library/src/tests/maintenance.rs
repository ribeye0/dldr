use super::*;

#[tokio::test]
async fn local_export_preserves_state_and_completed_deletion_refuses_busy_or_missing_works(
) -> Result<()> {
    let library = migrated_library().await?;
    let root = tempfile::tempdir()?;
    let library_root = root.path().join("library");
    let staging = root.path().join("staging");
    let destination = root.path().join("export");
    let source = library_root.join("RJ000001 local");
    std::fs::create_dir_all(&source)?;
    std::fs::create_dir(&destination)?;
    std::fs::write(source.join("track.wav"), b"audio")?;
    library
        .import_local_work_downloads(LocalWorkImportRequest::new(&library_root))
        .await?;
    let before = library.storage.work_download_state("RJ000001").await?;
    let exported = library
        .export_work_download("RJ000001", &library_root, &destination)
        .await?;
    assert_eq!(std::fs::read(exported.join("track.wav"))?, b"audio");
    assert_eq!(
        library.storage.work_download_state("RJ000001").await?,
        before
    );
    let lock = library.work_lock("RJ000001");
    let guard = lock.lock().await;
    assert!(library
        .export_work_download("RJ000001", &library_root, &destination)
        .await
        .is_err());
    assert!(library
        .remove_work_download(
            WorkDownloadRemovalRequest::new("RJ000001", &library_root, &staging).completed_only()
        )
        .await
        .is_err());
    assert!(source.is_dir());
    drop(guard);
    library
        .remove_work_download(
            WorkDownloadRemovalRequest::new("RJ000001", &library_root, &staging).completed_only(),
        )
        .await?;
    assert!(!source.exists());
    assert_eq!(std::fs::read(exported.join("track.wav"))?, b"audio");
    assert!(library
        .export_work_download("RJ000001", &library_root, &destination)
        .await
        .is_err());
    assert!(library
        .remove_work_download(
            WorkDownloadRemovalRequest::new("RJ000001", &library_root, &staging).completed_only()
        )
        .await
        .is_err());
    Ok(())
}

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

#[tokio::test]
async fn imports_local_work_folders_as_downloaded_products() -> Result<()> {
    let library = migrated_library().await?;
    let root = test_dir("import-local-work-folders");
    let library_root = root.join("library");
    let local_path = library_root.join("[RJ123456] Local Work");
    std::fs::create_dir_all(&local_path).unwrap();
    std::fs::create_dir_all(library_root.join("No Work Id")).unwrap();
    std::fs::create_dir_all(library_root.join("RJ000001 VJ000001 ambiguous")).unwrap();

    let report = library
        .import_local_work_downloads(LocalWorkImportRequest::new(&library_root))
        .await?;
    let page = library.list_products(&ProductListQuery::default()).await?;

    assert_eq!(report.scanned_directories, 3);
    assert_eq!(report.imported_count, 1);
    assert_eq!(report.skipped_no_id, 1);
    assert_eq!(report.skipped_ambiguous, 1);
    assert_eq!(report.skipped_existing, 0);
    assert_eq!(report.imported_works[0].work_id, "RJ123456");
    assert_eq!(
        report.imported_works[0].local_path,
        local_path.canonicalize().unwrap()
    );

    assert_eq!(page.total_count, 1);
    assert_eq!(page.products[0].work_id, "RJ123456");
    assert_eq!(page.products[0].title, "[RJ123456] Local Work");
    assert_eq!(page.products[0].owners[0].label, "Local");
    assert_eq!(
        page.products[0].download.status,
        WorkDownloadStatus::Downloaded
    );
    assert_eq!(
        page.products[0].download.local_path,
        Some(
            local_path
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        )
    );

    std::fs::remove_dir_all(root).unwrap();

    Ok(())
}

#[tokio::test]
async fn imports_local_work_folders_with_public_metadata() -> Result<()> {
    let library = migrated_library().await?;
    let root = test_dir("import-local-work-folders-metadata");
    let library_root = root.join("library");
    let local_path = library_root.join("[RJ123456] Local Work");
    let source = FakeLocalWorkMetadataSource {
        works: vec![public_work(
            "RJ123456",
            "Public Metadata Work",
            "Public Maker",
        )],
        fail: false,
    };
    std::fs::create_dir_all(&local_path).unwrap();

    let report = library
        .import_local_work_downloads_with_metadata_source(
            LocalWorkImportRequest::new(&library_root),
            &source,
        )
        .await?;
    let page = library.list_products(&ProductListQuery::default()).await?;

    assert_eq!(report.imported_count, 1);
    assert_eq!(report.metadata_candidate_count, 1);
    assert_eq!(report.metadata_updated_count, 1);
    assert_eq!(report.metadata_missing_count, 0);
    assert_eq!(report.metadata_error, None);
    assert_eq!(page.products[0].work_id, "RJ123456");
    assert_eq!(page.products[0].title, "Public Metadata Work");
    assert_eq!(page.products[0].maker_name.as_deref(), Some("Public Maker"));
    assert_eq!(page.products[0].work_type.as_deref(), Some("SOU"));
    assert_eq!(page.products[0].age_category.as_deref(), Some("r18"));
    assert_eq!(
        page.products[0].thumbnail_url.as_deref(),
        Some("https://img.example.test/RJ123456/main.jpg")
    );
    assert_eq!(page.products[0].content_size_bytes, Some(2048));
    assert_eq!(page.products[0].credit_groups[0].names, vec!["Voice One"]);
    assert_eq!(
        page.products[0].download.local_path,
        Some(
            local_path
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        )
    );

    std::fs::remove_dir_all(root).unwrap();

    Ok(())
}

#[tokio::test]
async fn local_work_import_hydrates_existing_placeholder_metadata() -> Result<()> {
    let library = migrated_library().await?;
    let root = test_dir("import-local-work-existing-metadata");
    let library_root = root.join("library");
    let local_path = library_root.join("[RJ123456] Local Work");
    std::fs::create_dir_all(&local_path).unwrap();

    library
        .import_local_work_downloads(LocalWorkImportRequest::new(&library_root))
        .await?;

    let source = FakeLocalWorkMetadataSource {
        works: vec![public_work(
            "RJ123456",
            "Public Metadata Work",
            "Public Maker",
        )],
        fail: false,
    };
    let report = library
        .import_local_work_downloads_with_metadata_source(
            LocalWorkImportRequest::new(&library_root),
            &source,
        )
        .await?;
    let page = library.list_products(&ProductListQuery::default()).await?;

    assert_eq!(report.imported_count, 0);
    assert_eq!(report.skipped_existing, 1);
    assert_eq!(report.metadata_candidate_count, 1);
    assert_eq!(report.metadata_updated_count, 1);
    assert_eq!(page.products[0].title, "Public Metadata Work");
    assert_eq!(page.products[0].maker_name.as_deref(), Some("Public Maker"));

    std::fs::remove_dir_all(root).unwrap();

    Ok(())
}

#[tokio::test]
async fn local_work_import_skips_existing_download_records() -> Result<()> {
    let library = migrated_library().await?;
    let root = test_dir("import-local-skips-existing");
    let library_root = root.join("library");
    let local_path = library_root.join("[RJ000001] Local Duplicate");
    std::fs::create_dir_all(&local_path).unwrap();
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;
    library
        .mark_work_downloaded(WorkDownloadMarkRequest::new(
            "RJ000001",
            &library_root,
            &local_path,
        ))
        .await?;

    let report = library
        .import_local_work_downloads(LocalWorkImportRequest::new(&library_root))
        .await?;

    assert_eq!(report.scanned_directories, 1);
    assert_eq!(report.imported_count, 0);
    assert_eq!(report.skipped_existing, 1);
    assert!(report.imported_works.is_empty());

    std::fs::remove_dir_all(root).unwrap();

    Ok(())
}
