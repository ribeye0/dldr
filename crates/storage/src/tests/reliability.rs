use super::*;

#[tokio::test]
async fn metadata_filters_apply_before_count_pagination_and_facets() -> Result<()> {
    let storage = migrated_storage().await?;
    storage.save_account(&account("a", "A")).await?;
    let raws = [
        r#"{"genre_ids":[503,500],"tags":[{"class":"voice_by","name":"伊倉える"}]}"#,
        r#"{"genre_ids":[503],"public_credit_tags":[{"class":"voice_by","name":{"ja_JP":"浅木式","zh_TW":"中文別名"}}]}"#,
        r#"{"tags":[{"class":"genre","name":"耳舐め"},{"class":"voice_by","name":"Other"}]}"#,
    ];
    storage
        .commit_account_sync(&sync_commit(
            "a",
            "filters",
            raws.iter()
                .enumerate()
                .map(|(i, raw)| {
                    work_with_raw_json(
                        &format!("RJ{:06}", i + 1),
                        &format!("Work {i}"),
                        "Circle",
                        "2026-01-01",
                        raw,
                    )
                })
                .collect(),
            (1..=3)
                .map(|i| account_work(&format!("RJ{i:06}"), "2026-01-01"))
                .collect(),
        ))
        .await?;
    let query = ProductListQuery {
        genre_names: vec!["低語".into(), "舔耳".into()],
        voice_names: vec!["伊倉える".into(), "浅木式".into()],
        limit: 1,
        ..Default::default()
    };
    let page = storage.list_products(&query).await?;
    assert_eq!(page.total_count, 1);
    assert_eq!(page.products[0].work_id, "RJ000001");
    assert_eq!(
        storage
            .list_products(&ProductListQuery {
                excluded_genre_names: vec!["舔耳".into()],
                ..Default::default()
            })
            .await?
            .total_count,
        1
    );
    assert_eq!(
        storage
            .list_products(&ProductListQuery {
                voice_names: vec!["中文別名".into()],
                ..Default::default()
            })
            .await?
            .products[0]
            .work_id,
        "RJ000002"
    );
    assert_eq!(
        storage
            .list_products(&ProductListQuery {
                excluded_voice_names: vec!["浅木式".into()],
                ..Default::default()
            })
            .await?
            .total_count,
        2
    );
    assert_eq!(
        storage
            .list_products(&ProductListQuery {
                voice_names: vec!["' OR 1=1 --".into()],
                ..Default::default()
            })
            .await?
            .total_count,
        0
    );
    let facets = storage.product_filter_facets(&query).await?;
    assert_eq!(
        facets
            .genres
            .iter()
            .find(|f| f.name == "舔耳")
            .unwrap()
            .count,
        2
    );
    assert!(facets.voices.iter().any(|f| f.name == "浅木式"));
    Ok(())
}

#[tokio::test]
async fn backup_round_trip_excludes_credentials_and_restore_failure_rolls_back() -> Result<()> {
    let source = migrated_storage().await?;
    source.save_account(&account("a", "A")).await?;
    source
        .commit_account_sync(&sync_commit(
            "a",
            "backup",
            vec![work_with_raw_json(
                "RJ000001",
                "Title",
                "Circle",
                "2026-01-01",
                r#"{"genre_ids":[503]}"#,
            )],
            vec![account_work("RJ000001", "2026-01-01")],
        ))
        .await?;
    sqlx::query("INSERT INTO work_custom_tags (work_id, name, normalized_name) VALUES ('RJ000001', '收藏', '收藏')")
        .execute(&source.pool).await?;
    source.save_download_concurrency(3).await?;
    source.save_download_rate_limit(Some(1024)).await?;
    source
        .enqueue_downloads(&[DownloadQueueEntry {
            work_id: "RJ000001".into(),
            title: String::new(),
            thumbnail_url: None,
            batch_id: "original".into(),
            position: 0,
            account_id: Some("a".into()),
            library_root: "/library".into(),
            download_root: "/staging".into(),
            unpack_policy: "keep_archives".into(),
            replace_existing: false,
            status: "queued".into(),
            error_message: None,
        }])
        .await?;
    let mut backup = source.export_library_backup().await?;
    let text = serde_json::to_string(&backup).unwrap();
    assert!(!text.contains("account:a:password"));
    assert!(!backup.tables.contains_key("sync_runs"));
    let target = migrated_storage().await?;
    let mut local = account("a", "Local");
    local.credential_ref = Some("local-secret-reference".into());
    target.save_account(&local).await?;
    target.restore_library_backup(&backup).await?;
    assert_eq!(
        target.accounts().await?[0].credential_ref.as_deref(),
        Some("local-secret-reference")
    );
    assert_eq!(
        target.list_products(&Default::default()).await?.total_count,
        1
    );
    let legacy: String =
        sqlx::query_scalar("SELECT name FROM work_custom_tags WHERE work_id = 'RJ000001'")
            .fetch_one(&target.pool)
            .await?;
    assert_eq!(legacy, "收藏");
    assert_eq!(target.download_queue().await?[0].status, "paused");
    assert_eq!(target.download_concurrency().await?, 3);
    assert_eq!(target.download_rate_limit().await?, Some(1024));
    target.save_download_concurrency(1).await?;
    backup.tables.get_mut("accounts").unwrap()[0].insert("enabled".into(), serde_json::json!(9));
    assert!(target.restore_library_backup(&backup).await.is_err());
    assert_eq!(
        target.download_concurrency().await?,
        1,
        "settings merge must roll back too"
    );
    source.save_download_rate_limit(None).await?;
    let unlimited = source.export_library_backup().await?;
    target.restore_library_backup(&unlimited).await?;
    assert_eq!(
        target.download_rate_limit().await?,
        None,
        "omitted optional setting restores unlimited/default"
    );
    backup.version = 999;
    assert!(target.restore_library_backup(&backup).await.is_err());
    Ok(())
}

#[tokio::test]
async fn queue_survives_reopening_and_recovers_active_entries_as_paused() -> Result<()> {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("library.sqlite");
    let storage = Storage::open(&path).await?;
    storage.run_migrations().await?;
    let mut tx = storage.begin_write().await?;
    tx.upsert_work(&work("RJ000001", "Title", "Circle", "2026-01-01"))
        .await?;
    tx.commit().await?;
    storage
        .enqueue_downloads(&[DownloadQueueEntry {
            work_id: "RJ000001".into(),
            title: String::new(),
            thumbnail_url: None,
            batch_id: "before".into(),
            position: 0,
            account_id: None,
            library_root: "/library".into(),
            download_root: "/staging".into(),
            unpack_policy: "keep_archives".into(),
            replace_existing: false,
            status: "queued".into(),
            error_message: None,
        }])
        .await?;
    storage
        .set_queue_status("RJ000001", "before", "running", None)
        .await?;
    storage.pool.close().await;
    let reopened = Storage::open(&path).await?;
    reopened.run_migrations().await?;
    reopened.recover_download_queue().await?;
    let queue = reopened.download_queue().await?;
    assert_eq!(queue[0].status, "paused");
    assert_eq!(queue[0].title, "Title");
    let mut entry = queue[0].clone();
    entry.batch_id = "new".into();
    reopened.enqueue_downloads(&[entry]).await?;
    reopened
        .set_queue_status("RJ000001", "before", "completed", None)
        .await?;
    assert_eq!(
        reopened.download_queue().await?[0].status,
        "queued",
        "old generation must not modify new request"
    );
    Ok(())
}

#[tokio::test]
async fn archive_preference_defaults_to_extract_and_survives_patches_reopen_and_backup(
) -> Result<()> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.sqlite");
    let source = Storage::open(&path).await?;
    source.run_migrations().await?;
    assert!(source.app_settings().await?.auto_extract);
    source
        .save_app_settings_patch(&AppSettingsPatch {
            auto_extract: Some(false),
            ..Default::default()
        })
        .await?;
    source
        .save_app_settings_patch(&AppSettingsPatch {
            ui_locale: Some(UiLocale::English),
            ..Default::default()
        })
        .await?;
    assert!(!source.app_settings().await?.auto_extract);
    let reopened = Storage::open(&path).await?;
    assert!(!reopened.app_settings().await?.auto_extract);
    let backup = source.export_library_backup().await?;
    let target = migrated_storage().await?;
    target.restore_library_backup(&backup).await?;
    assert!(!target.app_settings().await?.auto_extract);
    source
        .save_app_settings_patch(&AppSettingsPatch {
            auto_extract: Some(true),
            ..Default::default()
        })
        .await?;
    target
        .restore_library_backup(&source.export_library_backup().await?)
        .await?;
    assert!(target.app_settings().await?.auto_extract);
    Ok(())
}
