use super::*;

#[tokio::test]
async fn syncs_account_into_unified_product_cache() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;
    let sink = RecordingProgressSink::default();

    let report = library
        .sync_account_with_source(
            AccountSyncRequest {
                account_id: "account-a",
                password: None,
                cancellation_token: None,
                progress_sink: Some(&sink),
                two_factor_prompt: None,
            },
            &sync_source(),
        )
        .await?;
    let page = library
        .list_products(&ProductListQuery {
            sort: ProductSort::TitleAsc,
            ..ProductListQuery::default()
        })
        .await?;
    let sync_runs = library.storage().sync_runs_for_account("account-a").await?;
    let events = sink.events.lock().expect("events lock");

    assert_eq!(report.purchased_count, 2);
    assert_eq!(report.cached_work_count, 2);
    assert_eq!(report.page_limit, Some(50));
    assert_eq!(page.total_count, 2);
    assert_eq!(page.products[0].work_id, "RJ000001");
    assert_eq!(
        page.products[0].thumbnail_url,
        Some("https://img.example.test/RJ000001/full.jpg".to_owned())
    );
    assert_eq!(page.products[0].owners[0].account_id, "account-a");
    assert_eq!(sync_runs.len(), 1);
    assert_eq!(sync_runs[0].status, SyncRunStatus::Completed);
    assert!(matches!(events.first(), Some(SyncProgress::LoggingIn)));
    assert!(matches!(
        events.last(),
        Some(SyncProgress::Completed {
            cached_work_count: 2,
            ..
        })
    ));

    Ok(())
}

#[tokio::test]
async fn sync_can_use_one_shot_password_without_remembering() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(false)).await?;

    let report = library
        .sync_account_with_source(
            AccountSyncRequest {
                account_id: "account-a",
                password: Some("secret"),
                cancellation_token: None,
                progress_sink: None,
                two_factor_prompt: None,
            },
            &sync_source(),
        )
        .await?;

    assert_eq!(report.cached_work_count, 2);

    Ok(())
}

#[tokio::test]
async fn failed_sync_is_recorded_without_staling_cache() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;
    library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &sync_source())
        .await?;
    let mut failing_source = sync_source();
    failing_source.fail_at = Some(FakeFailurePoint::Works);

    assert!(matches!(
        library
            .sync_account_with_source(AccountSyncRequest::new("account-a"), &failing_source)
            .await,
        Err(LibraryError::SyncSource(_))
    ));

    let page = library.list_products(&ProductListQuery::default()).await?;
    let sync_runs = library.storage().sync_runs_for_account("account-a").await?;

    assert_eq!(page.total_count, 2);
    assert_eq!(sync_runs.len(), 2);
    assert_eq!(sync_runs[0].status, SyncRunStatus::Failed);
    assert_eq!(sync_runs[0].error_code, Some("sync_source".to_owned()));

    Ok(())
}

#[tokio::test]
async fn missing_work_details_are_cached_but_hidden_from_product_list() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;
    let mut source = sync_source();
    source.works.pop();

    let report = library
        .sync_account_with_source(AccountSyncRequest::new("account-a"), &source)
        .await?;

    let page = library.list_products(&ProductListQuery::default()).await?;
    let sync_runs = library.storage().sync_runs_for_account("account-a").await?;

    assert_eq!(report.purchased_count, 2);
    assert_eq!(report.cached_work_count, 2);
    assert_eq!(report.missing_detail_count, 1);
    assert_eq!(page.total_count, 1);
    assert_eq!(page.products.len(), 1);
    assert_eq!(page.products[0].work_id, "RJ000001");
    assert_eq!(sync_runs.len(), 1);
    assert_eq!(sync_runs[0].status, SyncRunStatus::Completed);

    Ok(())
}

#[tokio::test]
async fn cancellation_records_failed_sync() -> Result<()> {
    let library = migrated_library().await?;
    library.save_account(save_account_request(true)).await?;
    let token = CancellationToken::new();
    token.cancel();

    assert!(matches!(
        library
            .sync_account_with_source(
                AccountSyncRequest {
                    account_id: "account-a",
                    password: None,
                    cancellation_token: Some(&token),
                    progress_sink: None,
                    two_factor_prompt: None,
                },
                &sync_source(),
            )
            .await,
        Err(LibraryError::Cancelled)
    ));

    let sync_runs = library.storage().sync_runs_for_account("account-a").await?;

    assert_eq!(sync_runs.len(), 1);
    assert_eq!(sync_runs[0].status, SyncRunStatus::Cancelled);
    assert_eq!(sync_runs[0].error_code, Some("cancelled".to_owned()));

    Ok(())
}
