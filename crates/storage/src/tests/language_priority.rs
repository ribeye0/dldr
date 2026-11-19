use super::*;

#[tokio::test]
async fn patch_preserves_omitted_priority_and_repairs_invalid_stored_value_atomically() -> Result<()>
{
    let dir = tempfile::tempdir().expect("temporary settings database");
    let database = dir.path().join("settings.sqlite");
    let storage = Storage::open(&database).await?;
    storage.run_migrations().await?;
    assert_eq!(
        storage.app_settings().await?.language_priority.codes(),
        &DEFAULT_LANGUAGE_PRIORITY
    );
    let priority = LanguagePriority::new(vec![
        "ja_JP".into(),
        "zh_TW".into(),
        "zh_CN".into(),
        "en_US".into(),
    ])?;
    storage
        .save_app_settings_patch(&AppSettingsPatch {
            title_language_follows_ui: None,
            auto_extract: None,
            ui_locale: None,
            library_root: Some("/library".into()),
            download_root: Some("/downloads".into()),
            language_priority: Some(priority.clone()),
        })
        .await?;
    let saved = storage
        .save_app_settings_patch(&AppSettingsPatch {
            title_language_follows_ui: None,
            auto_extract: None,
            ui_locale: None,
            library_root: Some("/new-library".into()),
            download_root: None,
            language_priority: None,
        })
        .await?;
    assert_eq!(saved.language_priority, priority);
    assert_eq!(storage.app_settings().await?, saved);
    assert!(LanguagePriority::new(vec!["ja_JP".into(), "ja_JP".into()]).is_err());
    assert!(LanguagePriority::new(vec![" ".into()]).is_err());
    assert_eq!(storage.app_settings().await?, saved);

    sqlx::query("UPDATE app_settings SET value = 'not JSON' WHERE key = ?")
        .bind(LANGUAGE_PRIORITY_KEY)
        .execute(&storage.pool)
        .await?;
    assert!(matches!(
        storage.app_settings().await,
        Err(StorageError::InvalidStoredValue { .. })
    ));
    let result = storage
        .save_app_settings_patch(&AppSettingsPatch {
            library_root: Some("/must-not-be-written".into()),
            ..Default::default()
        })
        .await;
    assert!(matches!(
        result,
        Err(StorageError::InvalidStoredValue { .. })
    ));
    let root: String = sqlx::query_scalar("SELECT value FROM app_settings WHERE key = ?")
        .bind(LIBRARY_ROOT_KEY)
        .fetch_one(&storage.pool)
        .await?;
    assert_eq!(root, "/new-library");
    let repaired = storage
        .save_app_settings_patch(&AppSettingsPatch {
            library_root: Some(root),
            language_priority: Some(priority),
            ..Default::default()
        })
        .await?;
    assert_eq!(storage.app_settings().await?, repaired);
    storage.pool.close().await;
    let reopened = Storage::open(&database).await?;
    reopened.run_migrations().await?;
    assert_eq!(reopened.app_settings().await?, repaired);
    Ok(())
}

#[tokio::test]
async fn priority_changes_display_sort_and_facets_without_changing_search_or_maker_identity(
) -> Result<()> {
    let storage = migrated_storage().await?;
    storage
        .save_account(&account("account-a", "Account A"))
        .await?;
    let works = vec![
        CachedWork {
            title_json:
                r#"{"en_US":"Zulu","ja_JP":"Alpha","pt_BR":"Primeiro","ko_KR":"검색 이름"}"#.into(),
            maker_id: Some(" RG1 ".into()),
            maker_json: Some(
                r#"{"en_US":"Circle One","ja_JP":"制作元一","pt_BR":"Círculo"}"#.into(),
            ),
            ..work("RJ000001", "unused", "unused", "2026-01-01")
        },
        CachedWork {
            title_json: r#"{"en_US":"Alpha","ja_JP":"Zulu"}"#.into(),
            maker_id: Some("RG1".into()),
            maker_json: Some(r#"{"en_US":"Circle One","ja_JP":"制作元一"}"#.into()),
            ..work("RJ000002", "unused", "unused", "2026-01-01")
        },
        CachedWork {
            title_json: r#"{"en_US":"Middle","aa_AA":"Unlisted title"}"#.into(),
            maker_id: Some("  ".into()),
            maker_json: Some(r#"{"en_US":"Fallback Circle","ja_JP":"別の制作元"}"#.into()),
            ..work("RJ000003", "unused", "unused", "2026-01-01")
        },
        CachedWork {
            title_json: "broken".into(),
            maker_id: None,
            maker_json: Some("[]".into()),
            ..work("RJ000004", "unused", "unused", "2026-01-01")
        },
    ];
    let owned = works
        .iter()
        .map(|w| account_work(&w.work_id, "2026-01-01"))
        .collect();
    storage
        .commit_account_sync(&sync_commit("account-a", "sync-a", works, owned))
        .await?;
    let mut query = ProductListQuery {
        language_priority: LanguagePriority::for_locale(UiLocale::English),
        sort: ProductSort::TitleAsc,
        limit: 1,
        ..Default::default()
    };
    assert_eq!(
        storage.list_products(&query).await?.products[0].work_id,
        "RJ000002"
    );
    let english_facets = storage.product_filter_facets(&query).await?;
    assert_eq!(english_facets.makers[0].key, "id:RG1");
    assert_eq!(english_facets.makers[0].count, 2);
    assert_eq!(english_facets.makers[1].key, "name:Fallback Circle");
    query.language_priority = LanguagePriority::new(vec!["ja_JP".into()])?;
    assert_eq!(
        storage.list_products(&query).await?.products[0].work_id,
        "RJ000001"
    );
    query.offset = 1;
    assert_eq!(
        storage.list_products(&query).await?.products[0].work_id,
        "RJ000004"
    );
    let japanese_facets = storage.product_filter_facets(&query).await?;
    assert_eq!(japanese_facets.makers[0].key, "id:RG1");
    assert_eq!(japanese_facets.makers[0].name, "制作元一");
    assert_eq!(japanese_facets.makers[1].key, "name:Fallback Circle");
    assert_eq!(japanese_facets.makers[1].name, "別の制作元");
    query.maker_keys = vec!["id:RG1".into()];
    query.offset = 0;
    query.limit = 100;
    assert_eq!(storage.list_products(&query).await?.total_count, 2);
    query.language_priority = LanguagePriority::new(vec!["pt_BR".into()])?;
    let detail = storage
        .product_detail("RJ000001", &query.language_priority)
        .await?;
    assert_eq!(detail.title, "Primeiro");
    assert_eq!(detail.maker_name.as_deref(), Some("制作元一"));
    assert_eq!(
        storage.product_filter_facets(&query).await?.makers[0].name,
        "制作元一"
    );
    assert_eq!(storage.list_products(&query).await?.total_count, 2);
    for search in ["검색 이름", "制作元一", "Zulu"] {
        query.search = Some(search.into());
        assert!(storage.list_products(&query).await?.total_count > 0);
    }
    assert_eq!(
        storage.available_name_languages().await?,
        vec!["aa_AA", "en_US", "ja_JP", "ko_KR", "pt_BR"]
    );
    Ok(())
}

#[tokio::test]
async fn large_unknown_priority_uses_one_json_parameter_and_unlisted_code_fallback() -> Result<()> {
    let storage = migrated_storage().await?;
    storage
        .save_account(&account("account-a", "Account A"))
        .await?;
    storage
        .commit_account_sync(&sync_commit(
            "account-a",
            "sync-a",
            vec![CachedWork {
                title_json:
                    r#"{"zz_ZZ":"Last fallback","aa_AA":"First fallback","pt_BR":"Prioritized"}"#
                        .into(),
                maker_json: None,
                ..work("RJ000001", "unused", "unused", "2026-01-01")
            }],
            vec![account_work("RJ000001", "2026-01-01")],
        ))
        .await?;
    let mut codes: Vec<_> = (0..1500).map(|i| format!("unknown_{i:04}")).collect();
    codes.push("pt_BR".into());
    codes.push("long".repeat(300));
    let priority = LanguagePriority::new(codes)?;
    let saved = storage
        .save_app_settings_patch(&AppSettingsPatch {
            language_priority: Some(priority.clone()),
            ..Default::default()
        })
        .await?;
    assert_eq!(saved, storage.app_settings().await?);
    let query = ProductListQuery {
        language_priority: priority,
        ..Default::default()
    };
    assert_eq!(
        storage.list_products(&query).await?.products[0].title,
        "Prioritized"
    );
    assert_eq!(
        storage
            .product_detail("RJ000001", &LanguagePriority::new(vec![])?)
            .await?
            .title,
        "First fallback"
    );
    assert_eq!(
        storage.available_name_languages().await?,
        vec!["aa_AA", "pt_BR", "zz_ZZ"]
    );
    Ok(())
}

#[tokio::test]
async fn interface_locale_persists_independently_from_names_and_survives_backup() -> Result<()> {
    let storage = migrated_storage().await?;
    assert_eq!(
        storage.app_settings().await?.ui_locale,
        UiLocale::TraditionalChinese
    );
    let priority = LanguagePriority::new(vec!["ja_JP".into(), "en_US".into()])?;
    for locale in [
        UiLocale::SimplifiedChinese,
        UiLocale::Japanese,
        UiLocale::English,
        UiLocale::TraditionalChinese,
    ] {
        let settings = storage
            .save_app_settings_patch(&AppSettingsPatch {
                ui_locale: Some(locale),
                language_priority: Some(priority.clone()),
                ..Default::default()
            })
            .await?;
        assert_eq!(settings.ui_locale, locale);
        assert_eq!(
            settings.language_priority,
            priority.clone().for_settings(locale)
        );
        let settings = storage
            .save_app_settings_patch(&AppSettingsPatch::default())
            .await?;
        assert_eq!(settings.ui_locale, locale);
        assert_eq!(
            settings.language_priority,
            priority.clone().for_settings(locale)
        );
    }
    let backup = storage.export_library_backup().await?;
    storage
        .save_app_settings_patch(&AppSettingsPatch {
            ui_locale: Some(UiLocale::English),
            ..Default::default()
        })
        .await?;
    storage.restore_library_backup(&backup).await?;
    assert_eq!(
        storage.app_settings().await?.ui_locale,
        UiLocale::TraditionalChinese
    );
    assert!(serde_json::from_str::<UiLocale>("\"fr\"").is_err());
    Ok(())
}

#[tokio::test]
async fn locale_defaults_prioritize_chinese_and_preserve_custom_orders_and_maker_keys() -> Result<()>
{
    let storage = migrated_storage().await?;
    storage.save_account(&account("account-a", "A")).await?;
    let works = vec![CachedWork {
        title_json: r#"{"zh_TW":"繁體作品","zh_CN":"简体作品","ja_JP":"日本語作品","en_US":"English title","ko_KR":"한국어"}"#.into(),
        maker_id: None,
        maker_json: Some(r#"{"zh_TW":"繁體社團","ja_JP":"日本語サークル","en_US":"Stable maker identity"}"#.into()),
        ..work("RJ000001", "unused", "unused", "2026-01-01")
    }];
    storage
        .commit_account_sync(&sync_commit(
            "account-a",
            "locale-names",
            works,
            vec![account_work("RJ000001", "2026-01-01")],
        ))
        .await?;
    storage
        .enqueue_downloads(&[DownloadQueueEntry {
            work_id: "RJ000001".into(),
            title: String::new(),
            thumbnail_url: None,
            batch_id: "locale-queue".into(),
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
    for (locale, codes, title) in [
        (
            UiLocale::TraditionalChinese,
            vec!["zh_TW", "zh_CN", "ja_JP", "en_US"],
            "繁體作品",
        ),
        (
            UiLocale::SimplifiedChinese,
            vec!["zh_CN", "zh_TW", "ja_JP", "en_US"],
            "简体作品",
        ),
    ] {
        let settings = storage
            .save_app_settings_patch(&AppSettingsPatch {
                ui_locale: Some(locale),
                ..Default::default()
            })
            .await?;
        assert_eq!(settings.language_priority.codes(), &codes);
        let query = ProductListQuery {
            language_priority: settings.language_priority.clone(),
            ..Default::default()
        };
        let products = storage.list_products(&query).await?;
        assert_eq!(products.products[0].title, title);
        assert_eq!(storage.download_queue().await?[0].title, title);
        assert_eq!(
            storage.queued_download("RJ000001").await?.unwrap().title,
            title
        );
        assert_eq!(
            products.products[0].maker_name.as_deref(),
            Some("日本語サークル")
        );
        assert_eq!(
            storage.product_filter_facets(&query).await?.makers[0].key,
            "name:Stable maker identity"
        );
        assert_eq!(
            storage
                .product_detail("RJ000001", &settings.language_priority)
                .await?
                .title,
            title
        );
    }
    // An exact obsolete built-in preset follows locale defaults without rewriting stored data.
    sqlx::query("INSERT INTO app_settings(key,value) VALUES ('language_priority', ?)")
        .bind(r#"["en_US","ja_JP","ko_KR","zh_TW","zh_CN"]"#)
        .execute(&storage.pool)
        .await?;
    assert_eq!(
        storage.app_settings().await?.language_priority,
        LanguagePriority::for_locale(UiLocale::SimplifiedChinese)
    );
    let custom = LanguagePriority::new(vec![
        "ja_JP".into(),
        "en_US".into(),
        "zh_TW".into(),
        "zh_CN".into(),
    ])?;
    storage
        .save_app_settings_patch(&AppSettingsPatch {
            language_priority: Some(custom.clone()),
            ..Default::default()
        })
        .await?;
    assert_eq!(
        storage
            .save_app_settings_patch(&AppSettingsPatch {
                ui_locale: Some(UiLocale::TraditionalChinese),
                ..Default::default()
            })
            .await?
            .language_priority,
        custom
    );
    Ok(())
}

#[test]
fn chinese_title_fallbacks_skip_blanks_and_do_not_give_korean_a_special_rank() {
    let choose = |json, locale| {
        crate::names::preferred_name_with_priority(
            Some(json),
            &LanguagePriority::for_locale(locale),
        )
    };
    assert_eq!(
        choose(
            r#"{"zh_TW":" ","zh_CN":"简体","ja_JP":"日本語"}"#,
            UiLocale::TraditionalChinese
        )
        .as_deref(),
        Some("简体")
    );
    assert_eq!(
        choose(
            r#"{"zh_TW":"繁體","zh_CN":" ","ja_JP":"日本語"}"#,
            UiLocale::SimplifiedChinese
        )
        .as_deref(),
        Some("繁體")
    );
    let japanese = r#"{"zh_TW":" ","zh_CN":" ","ja_JP":"日本語","en_US":"English"}"#;
    assert_eq!(
        choose(japanese, UiLocale::TraditionalChinese).as_deref(),
        Some("日本語")
    );
    assert_eq!(
        choose(japanese, UiLocale::SimplifiedChinese).as_deref(),
        Some("日本語")
    );
    let others = r#"{"aa_AA":"Other","en_US":"English","ko_KR":"Korean"}"#;
    assert_eq!(
        choose(others, UiLocale::TraditionalChinese).as_deref(),
        Some("English")
    );
    assert_eq!(
        choose(others, UiLocale::SimplifiedChinese).as_deref(),
        Some("English")
    );
    assert_eq!(
        choose(r#"{"ko_KR":"Only name"}"#, UiLocale::TraditionalChinese).as_deref(),
        Some("Only name")
    );
}

#[tokio::test]
async fn legacy_extra_languages_become_fallback_without_rewriting_stored_preferences() -> Result<()>
{
    let storage = migrated_storage().await?;
    let raw = r#"["ko_KR","ja_JP","pt_BR","en_US"]"#;
    sqlx::query("INSERT INTO app_settings(key,value) VALUES ('language_priority', ?)")
        .bind(raw)
        .execute(&storage.pool)
        .await?;
    let priority = storage.app_settings().await?.language_priority;
    assert_eq!(priority.codes(), &["ja_JP", "en_US", "zh_TW", "zh_CN"]);
    let stored: String =
        sqlx::query_scalar("SELECT value FROM app_settings WHERE key='language_priority'")
            .fetch_one(&storage.pool)
            .await?;
    assert_eq!(stored, raw);
    // A locale-only save must not shuffle the completed custom order.
    let changed = storage
        .save_app_settings_patch(&AppSettingsPatch {
            ui_locale: Some(UiLocale::SimplifiedChinese),
            ..Default::default()
        })
        .await?;
    assert_eq!(changed.language_priority, priority);
    let title = crate::names::preferred_name_with_priority(
        Some(r#"{"ko_KR":"Legacy first","ja_JP":"Japanese","en_US":"English"}"#),
        &priority,
    );
    assert_eq!(title.as_deref(), Some("Japanese"));
    let fallback = crate::names::preferred_name_with_priority(
        Some(r#"{"pt_BR":"Second","aa_AA":"First","ja_JP":" "}"#),
        &priority,
    );
    assert_eq!(fallback.as_deref(), Some("First"));
    Ok(())
}

#[tokio::test]
async fn explicit_custom_default_order_survives_locale_changes_reopen_and_backup() -> Result<()> {
    let dir = tempfile::tempdir().expect("temporary settings database");
    let database = dir.path().join("settings.sqlite");
    let storage = Storage::open(&database).await?;
    storage.run_migrations().await?;
    assert!(storage.app_settings().await?.title_language_follows_ui);
    let custom = LanguagePriority::for_locale(UiLocale::TraditionalChinese);
    let settings = storage
        .save_app_settings_patch(&AppSettingsPatch {
            title_language_follows_ui: Some(false),
            language_priority: Some(custom.clone()),
            ..Default::default()
        })
        .await?;
    assert!(!settings.title_language_follows_ui);
    let settings = storage
        .save_app_settings_patch(&AppSettingsPatch {
            ui_locale: Some(UiLocale::English),
            ..Default::default()
        })
        .await?;
    assert_eq!(settings.language_priority, custom);
    assert!(!settings.title_language_follows_ui);
    let backup = storage.export_library_backup().await?;
    storage
        .save_app_settings_patch(&AppSettingsPatch {
            title_language_follows_ui: Some(true),
            ..Default::default()
        })
        .await?;
    storage.restore_library_backup(&backup).await?;
    storage.pool.close().await;
    let reopened = Storage::open(&database).await?;
    assert_eq!(reopened.app_settings().await?, settings);
    Ok(())
}

#[tokio::test]
async fn following_interface_language_clears_custom_order_and_survives_backup() -> Result<()> {
    let storage = migrated_storage().await?;
    storage
        .save_app_settings_patch(&AppSettingsPatch {
            language_priority: Some(LanguagePriority::for_locale(UiLocale::Japanese)),
            ..Default::default()
        })
        .await?;
    assert!(!storage.app_settings().await?.title_language_follows_ui);
    let settings = storage
        .save_app_settings_patch(&AppSettingsPatch {
            ui_locale: Some(UiLocale::English),
            title_language_follows_ui: Some(true),
            ..Default::default()
        })
        .await?;
    assert!(settings.title_language_follows_ui);
    assert_eq!(
        settings.language_priority,
        LanguagePriority::for_locale(UiLocale::English)
    );
    let stored: Option<String> = sqlx::query_scalar("SELECT value FROM app_settings WHERE key=?")
        .bind(LANGUAGE_PRIORITY_KEY)
        .fetch_optional(&storage.pool)
        .await?;
    assert_eq!(stored, None);
    let backup = storage.export_library_backup().await?;
    storage
        .save_app_settings_patch(&AppSettingsPatch {
            language_priority: Some(LanguagePriority::for_locale(UiLocale::TraditionalChinese)),
            ..Default::default()
        })
        .await?;
    storage.restore_library_backup(&backup).await?;
    let settings = storage
        .save_app_settings_patch(&AppSettingsPatch {
            ui_locale: Some(UiLocale::SimplifiedChinese),
            ..Default::default()
        })
        .await?;
    assert!(settings.title_language_follows_ui);
    assert_eq!(
        settings.language_priority,
        LanguagePriority::for_locale(UiLocale::SimplifiedChinese)
    );
    Ok(())
}

#[tokio::test]
async fn old_profiles_infer_follow_mode_before_changing_locale_without_losing_custom_order(
) -> Result<()> {
    for (raw, follows_ui) in [
        (r#"["zh_TW","zh_CN","ja_JP","en_US"]"#, true),
        (r#"["ja_JP","en_US","zh_TW","zh_CN"]"#, false),
    ] {
        let storage = migrated_storage().await?;
        sqlx::query("INSERT INTO app_settings(key,value) VALUES ('language_priority', ?)")
            .bind(raw)
            .execute(&storage.pool)
            .await?;
        assert_eq!(
            storage.app_settings().await?.title_language_follows_ui,
            follows_ui
        );
        let settings = storage
            .save_app_settings_patch(&AppSettingsPatch {
                ui_locale: Some(UiLocale::English),
                ..Default::default()
            })
            .await?;
        assert_eq!(settings.title_language_follows_ui, follows_ui);
        let expected = if follows_ui {
            LanguagePriority::for_locale(UiLocale::English)
        } else {
            LanguagePriority::from_stored(raw)?
        };
        assert_eq!(settings.language_priority, expected);
    }
    Ok(())
}

#[tokio::test]
async fn choosing_custom_without_an_order_freezes_the_current_interface_preset() -> Result<()> {
    let storage = migrated_storage().await?;
    storage
        .save_app_settings_patch(&AppSettingsPatch {
            ui_locale: Some(UiLocale::English),
            ..Default::default()
        })
        .await?;
    storage
        .save_app_settings_patch(&AppSettingsPatch {
            title_language_follows_ui: Some(false),
            ..Default::default()
        })
        .await?;
    let settings = storage
        .save_app_settings_patch(&AppSettingsPatch {
            ui_locale: Some(UiLocale::Japanese),
            ..Default::default()
        })
        .await?;
    assert!(!settings.title_language_follows_ui);
    assert_eq!(
        settings.language_priority,
        LanguagePriority::for_locale(UiLocale::English)
    );
    Ok(())
}
