use super::*;

#[tokio::test]
async fn circle_and_voice_names_prefer_japanese_then_chinese_without_changing_titles() -> Result<()>
{
    let storage = migrated_storage().await?;
    storage.save_account(&account("a", "A")).await?;
    let maps = [
        (
            r#"{"en_US":"English Circle","ja_JP":"日本語サークル","zh_TW":"繁中社團"}"#,
            "日本語サークル",
        ),
        (
            r#"{"en_US":"English Circle","ja_JP":" ","zh_TW":"繁中社團","zh_CN":"简中社团"}"#,
            "繁中社團",
        ),
        (
            r#"{"en_US":"English Circle","ja_JP":42,"zh_TW":"","zh_CN":"简中社团"}"#,
            "简中社团",
        ),
        (
            r#"{"en_US":"Only available circle"}"#,
            "Only available circle",
        ),
    ];
    let raw = serde_json::json!({
        "genre_ids": [503],
        "tags": [
            {"class":"voice_by", "name":{"en_US":"English Voice Alias", "ja_JP":"日文声優", "zh_TW":"繁中別名"}},
            {"class":"voice_by", "name":{"ja_JP":" ", "zh_TW":"繁中聲優", "zh_CN":"简中别名"}},
            {"class":"voice_by", "name":{"zh_TW":"", "zh_CN":"简中声优", "en_US":"Other Voice"}},
            {"class":"voice_by", "name":"Original Stage Name"},
            {"class":"voice_by", "name":"日文声優"},
            {"class":"voice_by", "name":null},
            {"class":"voice_by", "name":{"ja_JP":" "}}
        ]
    }).to_string();
    let works = maps
        .iter()
        .enumerate()
        .map(|(i, (map, _))| CachedWork {
            maker_json: Some((*map).into()),
            title_json: r#"{"en_US":"English work","ja_JP":"日本語の作品"}"#.into(),
            raw_json: raw.clone(),
            ..work(&format!("RJ{i:06}"), "unused", "unused", "2026-01-01")
        })
        .collect::<Vec<_>>();
    let owned = works
        .iter()
        .map(|w| account_work(&w.work_id, "2026-01-01"))
        .collect();
    storage
        .commit_account_sync(&sync_commit("a", "creator-names", works, owned))
        .await?;
    let page = storage
        .list_products(&ProductListQuery {
            sort: ProductSort::TitleAsc,
            language_priority: LanguagePriority::for_locale(UiLocale::English),
            ..Default::default()
        })
        .await?;
    for (i, (_, expected)) in maps.iter().enumerate() {
        let product = &page.products[i];
        assert_eq!(product.title, "English work");
        assert_eq!(product.maker_name.as_deref(), Some(*expected));
        let voice = &product.credit_groups[0];
        assert_eq!(voice.kind, "voice");
        assert_eq!(voice.names.len(), 4);
        for name in ["日文声優", "繁中聲優", "简中声优", "Original Stage Name"] {
            assert!(voice.names.iter().any(|value| value == name));
        }
        let detail = storage
            .product_detail(
                &product.work_id,
                &LanguagePriority::new(vec!["en_US".into()])?,
            )
            .await?;
        assert_eq!(detail.maker_name, product.maker_name);
        assert_eq!(detail.credit_groups, product.credit_groups);
    }
    for term in [
        "English Circle",
        "繁中社團",
        "日本語サークル",
        "English Voice Alias",
        "繁中別名",
    ] {
        let query = ProductListQuery {
            search: Some(term.into()),
            ..Default::default()
        };
        assert!(
            storage.list_products(&query).await?.total_count > 0,
            "alias not searchable: {term}"
        );
    }
    let facets = storage
        .product_filter_facets(&ProductListQuery::default())
        .await?;
    assert!(facets
        .makers
        .iter()
        .any(|maker| maker.name == "日本語サークル"));
    assert!(!facets
        .makers
        .iter()
        .any(|maker| maker.name == "English Circle"));
    Ok(())
}
