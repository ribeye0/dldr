use super::*;

async fn search_storage() -> Result<Storage> {
    let storage = migrated_storage().await?;
    storage.save_account(&account("a", "A")).await?;
    let mut first = work_with_raw_json(
        "RJ000001",
        "雨夜 NTR",
        "バイコーンの森",
        "2026-01-01",
        r#"{"genre_ids":[536,503],"tags":[{"class":"voice_by","name":" 小花衣こっこ "}],"public_credit_tags":[{"class":"voice_by","name":{"ja_JP":"Actor Two","zh_TW":" 聲優二 "}}]}"#,
    );
    first.maker_json =
        serde_json::json!({"ja_JP":"バイコーンの森","zh_TW":"雙角之森","en_US":"Bicorn Woods"})
            .to_string()
            .into();
    first.title_json = serde_json::json!({"ja_JP":"雨夜 NTR","en_US":"A Long Evening"}).to_string();
    storage.commit_account_sync(&sync_commit("a","search", vec![
        first,
        work_with_raw_json("RJ000002", "午後", "バイコーンの森 Extra", "2026-01-02", r#"{"genre_ids":[48,222],"tags":[{"class":"voice_by","name":"小花衣こっこ"}]}"#),
        work_with_raw_json("RJ000003", "雨夜 100%_Literal", "Other", "2026-01-03", r#"{"tags":[{"class":"genre","name":"女性優位"},{"class":"voice_by","name":"Actor Two"}]}"#),
        work_with_raw_json("RJ000004", "雨夜 Other", "Other", "2026-01-04", r#"{"tags":[{"class":"voice_by","name":"NTR"}]}"#),
    ],(1..=4).map(|i|account_work(&format!("RJ{i:06}"),"2026-01-01")).collect())).await?;
    // Old custom-tag records must neither leak into search nor be deleted.
    sqlx::query("INSERT INTO work_custom_tags (work_id, name, normalized_name) VALUES ('RJ000004', '女性主导', '女性主导')")
        .execute(&storage.pool).await?;
    Ok(storage)
}

async fn ids(storage: &Storage, search: &str) -> Result<Vec<String>> {
    Ok(storage
        .list_products(&ProductListQuery {
            search: Some(search.into()),
            sort: ProductSort::TitleAsc,
            ..Default::default()
        })
        .await?
        .products
        .into_iter()
        .map(|p| p.work_id)
        .collect())
}

#[tokio::test]
async fn exact_fields_combine_and_never_leak_into_other_credit_or_custom_tag_fields() -> Result<()>
{
    let storage = search_storage().await?;
    assert_eq!(
        ids(
            &storage,
            "$circle:バイコーンの森$ $va:小花衣こっこ$ $tag:女性主导$"
        )
        .await?,
        vec!["RJ000001"]
    );
    assert_eq!(
        ids(&storage, "$circle:雙角之森$ $va:聲優二$").await?,
        vec!["RJ000001"]
    );
    assert_eq!(
        ids(&storage, "$va:小花衣こっこ$$va:Actor Two$").await?,
        vec!["RJ000001"]
    );
    assert_eq!(ids(&storage, "$circle:バイコーンの$").await?.len(), 0);
    assert_eq!(ids(&storage, "$va:Actor$").await?.len(), 0);
    assert_eq!(ids(&storage, "$tag:NTR$").await?, vec!["RJ000002"]);
    assert_eq!(ids(&storage, "$tag:女性主导$").await?.len(), 2);
    assert_eq!(
        ids(&storage, "$tag:女性主導$ $-tag:低語$").await?,
        vec!["RJ000003"]
    );
    assert_eq!(ids(&storage, "$tag:青梅竹马$").await?, vec!["RJ000002"]);
    assert_eq!(ids(&storage, "$tag:幼なじみ$").await?, vec!["RJ000002"]);
    assert_eq!(ids(&storage, "$va:バイコーンの森$").await?.len(), 0);
    assert_eq!(ids(&storage, "$circle:小花衣こっこ$").await?.len(), 0);
    Ok(())
}

#[tokio::test]
async fn plain_keywords_keep_localized_names_genre_ids_and_literal_like_characters() -> Result<()> {
    let storage = search_storage().await?;
    assert_eq!(ids(&storage, "NTR").await?.len(), 3);
    assert_eq!(ids(&storage, "青梅竹馬").await?, vec!["RJ000002"]);
    assert_eq!(ids(&storage, "女性主导").await?.len(), 2); // Official genres only; ignore legacy custom tags.
    assert_eq!(ids(&storage, "小花衣こっこ").await?.len(), 2);
    assert_eq!(ids(&storage, "Bicorn").await?, vec!["RJ000001"]);
    assert_eq!(ids(&storage, "\"A Long Evening\"").await?, vec!["RJ000001"]);
    assert_eq!(ids(&storage, "雨夜 $va:聲優二$").await?, vec!["RJ000001"]);
    assert_eq!(ids(&storage, "100%_Literal").await?, vec!["RJ000003"]);
    assert!(ids(&storage, "$tag:' OR 1=1 --$").await?.is_empty());
    assert!(ids(&storage, "$circle:%$").await?.is_empty());
    assert!(ids(&storage, "ja_JP").await?.is_empty());
    assert!(ids(&storage, "zh_TW").await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn counts_facets_paging_downloaded_scope_and_invalid_bulk_queries_share_search_semantics(
) -> Result<()> {
    let storage = search_storage().await?;
    let query = ProductListQuery {
        search: Some("$tag:女性主导$".into()),
        limit: 1,
        offset: 1,
        ..Default::default()
    };
    let page = storage.list_products(&query).await?;
    assert_eq!(page.total_count, 2);
    assert_eq!(page.products.len(), 1);
    let facets = storage.product_filter_facets(&query).await?;
    assert_eq!(facets.makers.iter().map(|f| f.count).sum::<u64>(), 2);
    assert!(facets.voices.iter().any(|f| f.name == "小花衣こっこ"));
    assert_eq!(
        storage
            .list_products(&ProductListQuery {
                downloaded_only: true,
                ..query.clone()
            })
            .await?
            .total_count,
        0
    );
    for search in ["$va:$", "$tag:NTR", "$price:100$", "$tagw:NTR$"] {
        let query = ProductListQuery {
            search: Some(search.into()),
            ..Default::default()
        };
        assert!(matches!(
            storage.list_products(&query).await,
            Err(StorageError::InvalidSearch(_))
        ));
        assert!(matches!(
            storage.product_filter_facets(&query).await,
            Err(StorageError::InvalidSearch(_))
        ));
    }
    Ok(())
}

#[tokio::test]
async fn every_locale_finds_the_same_genre_before_count_page_and_facets() -> Result<()> {
    let storage = search_storage().await?;
    for name in ["強勢女", "女性主导", "女性優位", "Femdom"] {
        let search = format!("$tag:{name}$");
        let found = ids(&storage, &search).await?;
        assert_eq!(found.len(), 2, "{name}"); // ID-backed and legacy name-only tags
        let query = ProductListQuery {
            search: Some(search),
            limit: 1,
            ..Default::default()
        };
        let page = storage.list_products(&query).await?;
        assert_eq!(page.total_count, 2);
        assert_eq!(page.products.len(), 1);
        assert!(!storage
            .product_filter_facets(&query)
            .await?
            .makers
            .is_empty());
    }
    for name in ["低語", "低语", "ささやき", "Whispering"] {
        assert_eq!(
            ids(&storage, &format!("$tag:{name}$")).await?,
            vec!["RJ000001"]
        );
        assert_eq!(
            ids(&storage, &format!("$tag:Femdom$ $-tag:{name}$")).await?,
            vec!["RJ000003"]
        );
    }
    assert_eq!(
        ids(&storage, "$tag:Childhood Friend$").await?,
        vec!["RJ000002"]
    );
    assert_eq!(ids(&storage, "Whispering").await?, vec!["RJ000001"]);
    Ok(())
}

#[tokio::test]
async fn corrected_genre_names_search_ids_and_legacy_prefixed_metadata() -> Result<()> {
    let storage = migrated_storage().await?;
    storage.save_account(&account("a", "A")).await?;
    storage
        .commit_account_sync(&sync_commit(
            "a",
            "corrected-genres",
            vec![
                work_with_raw_json(
                    "RJ000001",
                    "One",
                    "Circle",
                    "2026-01-01",
                    r#"{"genre_ids":[207]}"#,
                ),
                work_with_raw_json(
                    "RJ000002",
                    "Two",
                    "Circle",
                    "2026-01-01",
                    r#"{"tags":[{"class":"genre","name":"6 萝"}]}"#,
                ),
                work_with_raw_json(
                    "RJ000003",
                    "Three",
                    "Circle",
                    "2026-01-01",
                    r#"{"genre_ids":[115]}"#,
                ),
                work_with_raw_json(
                    "RJ000004",
                    "Four",
                    "Circle",
                    "2026-01-01",
                    r#"{"tags":[{"class":"genre","name":"7 反向奸"}]}"#,
                ),
            ],
            (1..=4)
                .map(|i| account_work(&format!("RJ{i:06}"), "2026-01-01"))
                .collect(),
        ))
        .await?;
    for search in [
        "萝",
        "$tag:萝$",
        "$tag:6 萝$",
        "$tag:6萝$",
        "$tag:つるぺた$",
    ] {
        let mut matches = ids(&storage, search).await?;
        matches.sort();
        assert_eq!(matches, vec!["RJ000001", "RJ000002"], "{search}");
    }
    for search in [
        "反向奸",
        "$tag:反向奸$",
        "$tag:7 反向奸$",
        "$tag:7反向奸$",
        "$tag:逆レ$",
    ] {
        let mut matches = ids(&storage, search).await?;
        matches.sort();
        assert_eq!(matches, vec!["RJ000003", "RJ000004"], "{search}");
    }
    assert!(ids(&storage, "$tag:萝$ $-tag:6 萝$").await?.is_empty());
    assert!(ids(&storage, "$tag:反向奸$ $-tag:7 反向奸$")
        .await?
        .is_empty());
    Ok(())
}

#[tokio::test]
async fn official_genre_ids_survive_homonyms_and_exact_ntr_searches() -> Result<()> {
    let storage = migrated_storage().await?;
    storage.save_account(&account("a", "A")).await?;
    let metadata = [
        r#"{"genre_ids":[48],"tags":[{"class":"genre","name":"NTR"}]}"#,
        r#"{"genre_ids":[302]}"#,
        r#"{"genre_ids":[529],"tags":[{"class":"genre","name":"被寝取/NTR"}]}"#,
        r#"{"genre_ids":[166,478],"tags":[{"class":"genre","name":"短髮"}]}"#,
        r#"{"tags":[{"class":"genre","name":"被寝取/NTR"}]}"#,
        r#"{"tags":[{"class":"genre","name":"NTR"}]}"#,
    ];
    let works = metadata
        .iter()
        .enumerate()
        .map(|(i, raw)| {
            work_with_raw_json(
                &format!("RJ{:06}", i + 1),
                "Work",
                "Circle",
                "2026-01-01",
                raw,
            )
        })
        .collect();
    storage
        .commit_account_sync(&sync_commit(
            "a",
            "official-genres",
            works,
            (1..=6)
                .map(|i| account_work(&format!("RJ{i:06}"), "2026-01-01"))
                .collect(),
        ))
        .await?;
    for name in [
        "NTR",
        "被NTR(苦主视角)",
        "被寝取/NTR",
        "寝取られ",
        "Cuckoldry (Netorare)",
    ] {
        let mut found = ids(&storage, &format!("$tag:{name}$")).await?;
        found.sort();
        assert_eq!(found, ["RJ000001", "RJ000005", "RJ000006"], "{name}");
    }
    assert_eq!(ids(&storage, "$tag:NTR(黄毛视角)$").await?, ["RJ000002"]);
    assert_eq!(ids(&storage, "$tag:故意被NTR/绿奴$").await?, ["RJ000003"]);
    let detail = storage
        .product_detail("RJ000003", &LanguagePriority::default())
        .await?;
    assert_eq!(detail.tags.len(), 1);
    assert_eq!(detail.tags[0].genre_id, Some(529));
    assert_eq!(
        genre_display_name_for_id(529, UiLocale::SimplifiedChinese),
        "故意被NTR/绿奴"
    );
    let detail = storage
        .product_detail("RJ000004", &LanguagePriority::default())
        .await?;
    assert_eq!(
        detail
            .tags
            .iter()
            .filter_map(|tag| tag.genre_id)
            .collect::<Vec<_>>(),
        [166, 478]
    );
    let facets = storage
        .product_filter_facets(&ProductListQuery::default())
        .await?;
    assert!(facets
        .genres
        .iter()
        .any(|facet| facet.genre_id == Some(166)));
    assert!(facets
        .genres
        .iter()
        .any(|facet| facet.genre_id == Some(478)));
    assert!(ids(&storage, "$tag:NTR$ $-tag:被NTR(苦主视角)$")
        .await?
        .is_empty());
    Ok(())
}
