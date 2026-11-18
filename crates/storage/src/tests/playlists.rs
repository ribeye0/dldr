use super::*;

async fn populate(storage: &Storage) -> Result<()> {
    storage.save_account(&account("a", "Account")).await?;
    storage
        .commit_account_sync(&sync_commit(
            "a",
            "playlists",
            (1..=3)
                .map(|id| {
                    work_with_raw_json(
                        &format!("RJ{id:06}"),
                        &format!("Work {id}"),
                        "Circle",
                        "2026-01-01",
                        r#"{"genre_ids":[503]}"#,
                    )
                })
                .collect(),
            (1..=3)
                .map(|id| account_work(&format!("RJ{id:06}"), "2026-01-01"))
                .collect(),
        ))
        .await?;
    for id in 1..=2 {
        storage
            .save_work_download(&WorkDownloadUpdate {
                work_id: format!("RJ{id:06}"),
                status: WorkDownloadStatus::Downloaded,
                local_path: Some(format!("/library/{id}")),
                staging_path: None,
                unpack_policy: "manual".into(),
                bytes_received: 0,
                bytes_total: None,
                error_code: None,
                error_message: None,
                started_at: None,
                completed_at: None,
                updated_at: "2026-11-17".into(),
            })
            .await?;
    }
    Ok(())
}

#[tokio::test]
async fn playlists_keep_default_identity_order_duplicates_and_validate_admission() -> Result<()> {
    let storage = migrated_storage().await?;
    populate(&storage).await?;
    let lists = storage.playlists().await?;
    assert_eq!(lists.len(), 1);
    assert_eq!(lists[0].id, "0");
    assert_eq!(lists[0].name, "");
    storage.rename_playlist("0", " 晚安の音聲 ").await?;
    storage.set_playlist_work("0", "RJ000002", true).await?;
    storage.set_playlist_work("0", "RJ000001", true).await?;
    let (a, b) = tokio::join!(
        storage.set_playlist_work("0", "RJ000002", true),
        storage.set_playlist_work("0", "RJ000002", true)
    );
    a?;
    b?;
    let lists = storage.playlists().await?;
    assert_eq!(lists[0].name, "晚安の音聲");
    assert_eq!(lists[0].work_ids, ["RJ000002", "RJ000001"]);
    storage.set_playlist_work("0", "RJ000003", true).await?;
    assert_eq!(
        storage.playlists().await?[0].work_ids,
        ["RJ000002", "RJ000001", "RJ000003"]
    );
    storage.set_playlist_work("0", "RJ000003", false).await?;
    assert!(storage
        .set_playlist_work("0", "RJ999999", true)
        .await
        .is_err());
    assert!(storage
        .set_playlist_work("missing", "RJ000001", true)
        .await
        .is_err());
    for invalid in ["".into(), "  ".into(), "bad\nname".into(), "字".repeat(81)] {
        assert!(storage.create_playlist(&invalid).await.is_err());
    }
    assert!(storage.rename_playlist("absent", "Name").await.is_err());
    storage.set_playlist_work("0", "RJ000002", false).await?;
    storage.set_playlist_work("0", "RJ000002", false).await?;
    assert_eq!(storage.playlists().await?[0].work_ids, ["RJ000001"]);
    Ok(())
}

#[tokio::test]
async fn playlist_scope_applies_before_pagination_search_and_facets() -> Result<()> {
    let storage = migrated_storage().await?;
    populate(&storage).await?;
    let id = storage.create_playlist("Two works").await?;
    for work in ["RJ000001", "RJ000002"] {
        storage.set_playlist_work(&id, work, true).await?;
    }
    let query = ProductListQuery {
        playlist_id: Some(id.clone()),
        limit: 1,
        offset: 1,
        sort: ProductSort::TitleAsc,
        ..Default::default()
    };
    let page = storage.list_products(&query).await?;
    assert_eq!(page.total_count, 2);
    assert_eq!(page.products[0].work_id, "RJ000002");
    assert_eq!(
        storage.product_filter_facets(&query).await?.makers[0].count,
        2
    );
    let searched = ProductListQuery {
        search: Some("Work 1".into()),
        offset: 0,
        ..query.clone()
    };
    assert_eq!(storage.list_products(&searched).await?.total_count, 1);
    storage.set_playlist_work(&id, "RJ000003", true).await?;
    let all = storage
        .list_products(&ProductListQuery {
            offset: 0,
            limit: 100,
            ..query.clone()
        })
        .await?;
    assert_eq!(all.total_count, 3);
    let pending = all
        .products
        .iter()
        .find(|work| work.work_id == "RJ000003")
        .expect("undownloaded playlist entry");
    assert_eq!(pending.download.status, WorkDownloadStatus::NotDownloaded);
    assert_eq!(pending.download.local_path, None);
    assert_eq!(
        storage
            .list_products(&ProductListQuery {
                downloaded_only: true,
                offset: 0,
                ..query.clone()
            })
            .await?
            .total_count,
        2
    );
    storage.set_playlist_work(&id, "RJ000003", false).await?;
    assert_eq!(
        storage
            .list_products(&ProductListQuery {
                playlist_id: Some("0".into()),
                ..query.clone()
            })
            .await?
            .total_count,
        0
    );
    // Keep membership if a download becomes unavailable; it stays visible for
    // removal/repair but is excluded when callers request downloaded-only works.
    sqlx::query("UPDATE work_downloads SET status='failed' WHERE work_id='RJ000002'")
        .execute(&storage.pool)
        .await?;
    assert_eq!(storage.list_products(&query).await?.total_count, 2);
    assert_eq!(
        storage
            .list_products(&ProductListQuery {
                downloaded_only: true,
                offset: 0,
                ..query
            })
            .await?
            .total_count,
        1
    );
    Ok(())
}

#[tokio::test]
async fn playlists_survive_reopen_and_backup_v2_accepts_legacy_v1() -> Result<()> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.sqlite");
    let storage = Storage::open(&path).await?;
    storage.run_migrations().await?;
    populate(&storage).await?;
    let id = storage.create_playlist("おやすみ").await?;
    storage.rename_playlist("0", "預設").await?;
    storage.set_playlist_work(&id, "RJ000001", true).await?;
    storage.set_playlist_work(&id, "RJ000003", true).await?;
    let expected = storage.playlists().await?;
    let reopened = Storage::open(&path).await?;
    reopened.run_migrations().await?;
    assert_eq!(reopened.playlists().await?, expected);
    let mut backup = storage.export_library_backup().await?;
    assert_eq!(backup.version, 2);
    let target = migrated_storage().await?;
    target.restore_library_backup(&backup).await?;
    assert_eq!(target.playlists().await?, expected);
    backup.version = 1;
    backup.tables.remove("playlists");
    backup.tables.remove("playlist_works");
    target.restore_library_backup(&backup).await?;
    assert_eq!(target.playlists().await?, expected);
    backup.tables.remove("works");
    assert!(target.restore_library_backup(&backup).await.is_err());
    assert_eq!(target.playlists().await?, expected);
    Ok(())
}
