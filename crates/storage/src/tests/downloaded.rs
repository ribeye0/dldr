use super::*;

#[tokio::test]
async fn chinese_genres_resolve_cached_ids_and_search_before_pagination() -> Result<()> {
    let storage = migrated_storage().await?;
    storage.save_account(&account("a", "Account A")).await?;
    let raw = [
        r#"{"genre_ids":[503,503],"tags":[{"class":"voice_by","name":"Voice A"}]}"#,
        r#"{"genre_ids":[500]}"#,
        r#"{"tags":[{"class":"genre","name":"ささやき"}]}"#,
        r#"{"genres":[{"id":"999999"}]}"#,
    ];
    storage
        .commit_account_sync(&sync_commit(
            "a",
            "sync-genres",
            raw.iter()
                .enumerate()
                .map(|(i, raw)| {
                    work_with_raw_json(
                        &format!("RJ{:06}", i + 1),
                        &format!("Work {i}"),
                        "Maker",
                        "2026-01-01T00:00:00Z",
                        raw,
                    )
                })
                .collect(),
            (1..=4)
                .map(|id| account_work(&format!("RJ{id:06}"), "2026-02-01T00:00:00Z"))
                .collect(),
        ))
        .await?;
    storage
        .save_work_download(&WorkDownloadUpdate {
            work_id: "RJ000001".into(),
            status: WorkDownloadStatus::Downloaded,
            local_path: Some("/library/one".into()),
            staging_path: None,
            unpack_policy: "manual".into(),
            bytes_received: 0,
            bytes_total: None,
            error_code: None,
            error_message: None,
            started_at: None,
            completed_at: None,
            updated_at: "2026-11-15T00:00:00Z".into(),
        })
        .await?;
    let detail = storage
        .product_detail("RJ000001", &LanguagePriority::default())
        .await?;
    assert_eq!(
        detail
            .tags
            .iter()
            .filter(|t| t.class == "genre")
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>(),
        vec!["低語"]
    );
    assert!(detail
        .tags
        .iter()
        .any(|t| t.class == "voice_by" && t.name == "Voice A"));
    for search in ["低語", "低", "ささやき"] {
        let query = ProductListQuery {
            search: Some(search.into()),
            sort: ProductSort::TitleAsc,
            limit: 1,
            offset: 1,
            ..Default::default()
        };
        let page = storage.list_products(&query).await?;
        assert_eq!(page.total_count, 2);
        assert_eq!(page.products[0].work_id, "RJ000003");
        assert_eq!(page.products[0].tags[0].name, "低語");
        let local = ProductListQuery {
            downloaded_only: true,
            offset: 0,
            ..query
        };
        assert_eq!(storage.list_products(&local).await?.total_count, 1);
        assert_eq!(
            storage.product_filter_facets(&local).await?.makers[0].count,
            1
        );
    }
    let query = ProductListQuery {
        search: Some("舔耳".into()),
        ..Default::default()
    };
    let page = storage.list_products(&query).await?;
    assert_eq!(page.total_count, 1);
    assert_eq!(page.products[0].tags[0].name, "舔耳");
    assert_eq!(
        storage
            .list_products(&ProductListQuery {
                downloaded_only: true,
                ..query
            })
            .await?
            .total_count,
        0
    );
    assert_eq!(
        storage
            .list_products(&ProductListQuery {
                search: Some("分類 #999999".into()),
                ..Default::default()
            })
            .await?
            .total_count,
        1
    );
    Ok(())
}

#[tokio::test]
async fn downloaded_gallery_scopes_count_pages_facets_and_tag_search() -> Result<()> {
    let storage = migrated_storage().await?;
    storage.save_account(&account("a", "Account A")).await?;
    storage
        .commit_account_sync(&sync_commit(
            "a",
            "sync-a",
            (1..=8)
                .map(|id| {
                    work_with_raw_json(
                        &format!("RJ{id:06}"),
                        &format!("Work {id}"),
                        if id == 1 || id == 8 { "Local maker" } else { "Other maker" },
                        "2026-01-01T00:00:00Z",
                        r#"{"tags":[{"class":"genre","name":"ささやき"},{"class":"genre","name":"ささやき"},{"class":"voice_by","name":"Voice A"}]}"#,
                    )
                })
                .collect(),
            (1..=7).map(|id| account_work(&format!("RJ{id:06}"), "2026-02-01T00:00:00Z")).collect(),
        ))
        .await?;
    for (id, status, path) in [
        (1, WorkDownloadStatus::Downloaded, Some("/library/one")),
        (3, WorkDownloadStatus::Downloading, Some("/library/partial")),
        (4, WorkDownloadStatus::Failed, Some("/library/failed")),
        (5, WorkDownloadStatus::Cancelled, Some("/library/cancelled")),
        (6, WorkDownloadStatus::Downloaded, None),
        (7, WorkDownloadStatus::Downloaded, Some("  ")),
        (8, WorkDownloadStatus::Downloaded, Some("/library/imported")),
    ] {
        let work_id = format!("RJ{id:06}");
        storage
            .save_work_download(&WorkDownloadUpdate {
                work_id: work_id.clone(),
                status,
                local_path: path.map(str::to_owned),
                staging_path: None,
                unpack_policy: "manual".into(),
                bytes_received: 0,
                bytes_total: None,
                error_code: None,
                error_message: None,
                started_at: None,
                completed_at: None,
                updated_at: "2026-11-15T00:00:00Z".into(),
            })
            .await?;
        let name = if id == 1 || id == 8 { "Keep" } else { "Other" };
        sqlx::query("INSERT INTO work_custom_tags (work_id, name, normalized_name) VALUES (?1, ?2, lower(?2))")
            .bind(&work_id).bind(name).execute(&storage.pool).await?;
    }
    let mut query = ProductListQuery {
        downloaded_only: true,
        sort: ProductSort::TitleAsc,
        limit: 1,
        offset: 1,
        ..Default::default()
    };
    let page = storage.list_products(&query).await?;
    assert_eq!(page.total_count, 2);
    assert_eq!(page.products.len(), 1);
    assert_eq!(page.products[0].work_id, "RJ000008");
    assert_eq!(
        page.products[0].owners[0].account_id,
        LOCAL_PRODUCT_OWNER_ID
    );
    assert!(page.products[0].thumbnail_url.is_some());
    assert_eq!(page.products[0].tags.len(), 2); // Same parser/deduplication as details.
    assert_eq!(
        page.products[0].tags,
        storage
            .product_detail("RJ000008", &LanguagePriority::default())
            .await?
            .tags
    );
    let facets = storage.product_filter_facets(&query).await?;
    assert_eq!(facets.makers.len(), 1);
    assert_eq!(facets.makers[0].count, 2);

    query.offset = 0;
    query.search = Some("ささやき".into()); // A Japanese source label remains searchable.
    assert_eq!(storage.list_products(&query).await?.total_count, 2);
    query.search = Some("keep".into()); // Legacy custom tags no longer participate in search.
    assert_eq!(storage.list_products(&query).await?.total_count, 0);
    query.account_ids = vec!["a".into()];
    assert_eq!(storage.list_products(&query).await?.total_count, 0);
    query.account_ids.clear();
    query.search = Some("Other".into());
    assert_eq!(storage.list_products(&query).await?.total_count, 0);

    // Completed local works remain available after their account is removed.
    storage.delete_account("a").await?;
    query.search = None;
    assert_eq!(storage.list_products(&query).await?.total_count, 2);
    Ok(())
}
