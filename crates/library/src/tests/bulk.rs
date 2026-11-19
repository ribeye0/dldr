use super::*;

#[tokio::test]
async fn selected_bulk_download_never_includes_unselected_works() -> Result<()> {
    let library = migrated_library().await?;
    let root = test_dir("selected-bulk-download");
    let library_root = root.join("library");
    let download_root = root.join("downloads");
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;
    let ids = vec!["RJ000002".to_owned()];
    let candidates = library
        .select_bulk_candidates(&ProductListQuery::default(), true, Some(&ids))
        .await?;
    assert_eq!(candidates.work_ids, ids);
    assert_eq!(candidates.total_count, 1);
    let preview = library
        .preview_download_products_with_source(
            BulkWorkDownloadPreviewRequest {
                query: ProductListQuery::default(),
                work_ids: Some(ids.clone()),
                skip_downloaded: true,
                cancellation_token: None,
                progress_sink: None,
            },
            &FakeDownloadSource,
        )
        .await?;
    assert_eq!(preview.requested_count, 1);
    assert_eq!(preview.works[0].work_id, "RJ000002");
    let report = library
        .download_products_with_source(
            BulkWorkDownloadRequest {
                work_ids: Some(ids),
                ..BulkWorkDownloadRequest::new(
                    ProductListQuery::default(),
                    &library_root,
                    &download_root,
                )
            },
            &FakeDownloadSource,
        )
        .await?;
    assert_eq!(report.succeeded_count, 1);
    assert_eq!(report.succeeded_works[0].work_id, "RJ000002");
    assert!(!library_root.join("RJ000001").exists());

    // An unrelated downloaded work must not inflate the selected skip count.
    let candidates = library
        .select_bulk_candidates(
            &ProductListQuery::default(),
            true,
            Some(&["RJ000001".into()]),
        )
        .await?;
    assert_eq!(candidates.skipped_downloaded_count, 0);
    assert_eq!(candidates.work_ids, vec!["RJ000001"]);
    let empty = library
        .select_bulk_candidates(&ProductListQuery::default(), true, Some(&[]))
        .await?;
    assert!(empty.work_ids.is_empty());
    assert_eq!(empty.total_count, 0);
    assert_eq!(empty.skipped_downloaded_count, 0);
    std::fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn bulk_download_skips_downloaded_works() -> Result<()> {
    let library = migrated_library().await?;
    let root = test_dir("bulk-download-skips");
    let library_root = root.join("library");
    let download_root = root.join("downloads");
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;
    library
        .download_work_with_source(
            WorkDownloadRequest::new("RJ000001", &library_root, &download_root),
            &FakeDownloadSource,
        )
        .await?;
    let sink = RecordingBulkDownloadProgressSink::default();

    let report = library
        .download_products_with_source(
            BulkWorkDownloadRequest {
                progress_sink: Some(&sink),
                ..BulkWorkDownloadRequest::new(
                    ProductListQuery::default(),
                    &library_root,
                    &download_root,
                )
            },
            &FakeDownloadSource,
        )
        .await?;
    let page = library.list_products(&ProductListQuery::default()).await?;
    let events = sink.events.lock().expect("bulk events lock");

    assert_eq!(report.total_count, 2);
    assert_eq!(report.requested_count, 1);
    assert_eq!(report.skipped_downloaded_count, 1);
    assert_eq!(report.succeeded_count, 1);
    assert_eq!(report.failed_count, 0);
    assert!(events.iter().any(|event| matches!(event,
        BulkWorkDownloadProgress::Planned { works }
            if works.len() == 1 && works[0].work_id == "RJ000002" && !works[0].title.is_empty()
    )));
    assert!(events.iter().any(|event| matches!(event,
        BulkWorkDownloadProgress::WorkProgress { work_id, progress: WorkDownloadProgress::Download(p) }
            if work_id == "RJ000002" && p.bytes_received > 0
    )));
    assert!(library_root.join("RJ000002/RJ000001.txt").exists());
    assert!(page
        .products
        .iter()
        .all(|product| product.download.status == WorkDownloadStatus::Downloaded));
    assert!(matches!(
        events.first(),
        Some(BulkWorkDownloadProgress::Selecting)
    ));
    assert!(matches!(
        events.last(),
        Some(BulkWorkDownloadProgress::Completed { .. })
    ));

    std::fs::remove_dir_all(root).unwrap();

    Ok(())
}

#[tokio::test]
async fn bulk_download_preview_reports_expected_bytes() -> Result<()> {
    let library = migrated_library().await?;
    let root = test_dir("bulk-preview");
    let library_root = root.join("library");
    let download_root = root.join("downloads");
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;
    library
        .download_work_with_source(
            WorkDownloadRequest::new("RJ000001", &library_root, &download_root),
            &FakeDownloadSource,
        )
        .await?;

    let preview = library
        .preview_download_products_with_source(
            BulkWorkDownloadPreviewRequest {
                query: ProductListQuery::default(),
                work_ids: None,
                skip_downloaded: true,
                cancellation_token: None,
                progress_sink: None,
            },
            &FakeDownloadSource,
        )
        .await?;

    assert_eq!(preview.total_count, 2);
    assert_eq!(preview.requested_count, 1);
    assert_eq!(preview.skipped_downloaded_count, 1);
    assert_eq!(preview.planned_count, 1);
    assert_eq!(preview.failed_count, 0);
    assert_eq!(preview.known_expected_bytes, 10);
    assert_eq!(preview.total_expected_bytes, Some(10));
    assert_eq!(preview.unknown_size_count, 0);
    assert_eq!(preview.works[0].work_id, "RJ000002");
    assert_eq!(preview.works[0].file_count, 1);

    std::fs::remove_dir_all(root).unwrap();

    Ok(())
}

#[tokio::test]
async fn bulk_download_preview_reports_progress() -> Result<()> {
    let library = migrated_library().await?;
    let root = test_dir("bulk-preview-progress");
    let library_root = root.join("library");
    let download_root = root.join("downloads");
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;
    library
        .download_work_with_source(
            WorkDownloadRequest::new("RJ000001", &library_root, &download_root),
            &FakeDownloadSource,
        )
        .await?;
    let sink = RecordingBulkDownloadPreviewProgressSink::default();

    let preview = library
        .preview_download_products_with_source(
            BulkWorkDownloadPreviewRequest {
                query: ProductListQuery::default(),
                work_ids: None,
                skip_downloaded: true,
                cancellation_token: None,
                progress_sink: Some(&sink),
            },
            &FakeDownloadSource,
        )
        .await?;
    let events = sink.events.lock().expect("preview events lock");

    assert_eq!(preview.requested_count, 1);
    assert!(matches!(
        events.first(),
        Some(BulkWorkDownloadPreviewProgress::Selecting)
    ));
    assert!(matches!(
        events.get(1),
        Some(BulkWorkDownloadPreviewProgress::Selected {
            requested_count: 1,
            ..
        })
    ));
    assert!(events.iter().any(|event| {
        matches!(
            event,
            BulkWorkDownloadPreviewProgress::WorkStarted {
                work_id,
                current: 1,
                total: 1
            } if work_id == "RJ000002"
        )
    }));
    assert!(events.iter().any(|event| {
        matches!(
            event,
            BulkWorkDownloadPreviewProgress::WorkPlanned {
                work_id,
                current: 1,
                total: 1,
                known_expected_bytes: 10,
                total_expected_bytes: Some(10),
                unknown_size_count: 0
            } if work_id == "RJ000002"
        )
    }));
    assert!(matches!(
        events.last(),
        Some(BulkWorkDownloadPreviewProgress::Completed {
            planned_count: 1,
            failed_count: 0,
            ..
        })
    ));

    std::fs::remove_dir_all(root).unwrap();

    Ok(())
}

#[tokio::test]
async fn bulk_download_limits_to_supplied_work_ids() -> Result<()> {
    let library = migrated_library().await?;
    let root = test_dir("bulk-download-limited");
    let library_root = root.join("library");
    let download_root = root.join("downloads");
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;

    let report = library
        .download_products_with_source(
            BulkWorkDownloadRequest {
                work_ids: Some(vec!["RJ000002".to_owned()]),
                ..BulkWorkDownloadRequest::new(
                    ProductListQuery::default(),
                    &library_root,
                    &download_root,
                )
            },
            &FakeDownloadSource,
        )
        .await?;
    let first = library.storage().work_download_state("RJ000001").await?;
    let second = library.storage().work_download_state("RJ000002").await?;

    assert_eq!(report.total_count, 1);
    assert_eq!(report.requested_count, 1);
    assert_eq!(report.succeeded_count, 1);
    assert_eq!(first.status, WorkDownloadStatus::NotDownloaded);
    assert_eq!(second.status, WorkDownloadStatus::Downloaded);

    std::fs::remove_dir_all(root).unwrap();

    Ok(())
}

#[tokio::test]
async fn bulk_download_continues_after_work_failure() -> Result<()> {
    let library = migrated_library().await?;
    let root = test_dir("bulk-download-failure");
    let library_root = root.join("library");
    let download_root = root.join("downloads");
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;

    let progress = RecordingBulkDownloadProgressSink::default();
    let mut request =
        BulkWorkDownloadRequest::new(ProductListQuery::default(), &library_root, &download_root);
    request.progress_sink = Some(&progress);
    let report = library
        .download_products_with_source(
            request,
            &FailingDownloadSource {
                fail_work_id: "RJ000002",
                failure: || {
                    LibraryError::Download(dldr_download::DownloadError::Io(
                        std::io::Error::from_raw_os_error(28),
                    ))
                },
            },
        )
        .await?;
    let failed_state = library.storage().work_download_state("RJ000002").await?;

    assert_eq!(report.requested_count, 2);
    assert_eq!(report.succeeded_count, 1);
    assert_eq!(report.failed_count, 1);
    assert_eq!(report.failed_works[0].work_id, "RJ000002");
    assert_eq!(failed_state.status, WorkDownloadStatus::Failed);
    let events = progress.events.lock().unwrap();
    let failures: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            BulkWorkDownloadProgress::WorkFailed {
                work_id,
                error_code,
                error_details,
                ..
            } => Some((work_id, error_code, error_details)),
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].0, "RJ000002");
    assert_eq!(failures[0].1, "download");
    assert_eq!(failures[0].2["osCode"], 28);
    assert_eq!(failures[0].2["downloadErrorKind"], "io");
    assert!(library_root.join("RJ000001/RJ000001.txt").exists());

    std::fs::remove_dir_all(root).unwrap();

    Ok(())
}

#[tokio::test]
async fn bulk_token_cancellation_keeps_completed_report() -> Result<()> {
    struct CancelAfterFirst(CancellationToken);
    impl BulkWorkDownloadProgressSink for CancelAfterFirst {
        fn emit(&self, progress: BulkWorkDownloadProgress) {
            if matches!(progress, BulkWorkDownloadProgress::WorkCompleted { .. }) {
                self.0.cancel();
            }
        }
    }
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;
    let root = test_dir("bulk-partial-cancel");
    let token = CancellationToken::new();
    let sink = CancelAfterFirst(token.clone());
    let result = library
        .download_products_with_source(
            BulkWorkDownloadRequest {
                cancellation_token: Some(&token),
                progress_sink: Some(&sink),
                ..BulkWorkDownloadRequest::new(
                    ProductListQuery::default(),
                    &root.join("library"),
                    &root.join("staging"),
                )
            },
            &FakeDownloadSource,
        )
        .await;
    let LibraryError::BulkInterrupted {
        report,
        interrupted_work,
        ..
    } = result.unwrap_err()
    else {
        panic!("partial report required")
    };
    assert_eq!(report.succeeded_count, 1);
    assert_eq!(report.failed_count, 0);
    assert!(interrupted_work.is_none());
    assert!(report.succeeded_works[0].local_path.is_dir());
    Ok(())
}
