use sqlx::Row;

use super::*;
mod creator_names;
mod downloaded;
mod language_priority;
mod localized_names;
mod playlists;
mod reliability;
mod search;

async fn migrated_storage() -> Result<Storage> {
    let storage = Storage::open_in_memory().await?;
    storage.run_migrations().await?;
    Ok(storage)
}

#[tokio::test]
async fn database_diagnostics_keep_codes_without_private_schema_or_values() -> Result<()> {
    let storage = migrated_storage().await?;
    let mut tx = storage.begin_write().await?;
    tx.execute("CREATE TABLE PRIVATE_TABLE(value TEXT CHECK(value='allowed'))")
        .await?;
    let error = tx
        .execute("INSERT INTO PRIVATE_TABLE VALUES('PRIVATE-VALUE')")
        .await
        .unwrap_err();
    let details = error.support_details();
    assert_eq!(details["failureKind"], "storage");
    assert_eq!(details["databaseCode"], "275");
    assert_eq!(details["databaseKind"], "CheckViolation");
    assert!(!details.to_string().contains("PRIVATE"));
    tx.rollback().await?;
    Ok(())
}

#[tokio::test]
async fn download_rate_limit_is_independent_of_other_settings_and_can_be_cleared() -> Result<()> {
    let storage = migrated_storage().await?;
    assert_eq!(storage.download_rate_limit().await?, None);
    storage.save_download_rate_limit(Some(524288)).await?;
    storage
        .save_app_settings_patch(&AppSettingsPatch {
            library_root: Some("/tmp/library".into()),
            ..Default::default()
        })
        .await?;
    assert_eq!(storage.download_rate_limit().await?, Some(524288));
    storage.save_download_rate_limit(None).await?;
    assert_eq!(storage.download_rate_limit().await?, None);
    assert_eq!(
        storage.app_settings().await?.library_root.as_deref(),
        Some("/tmp/library")
    );
    let mut transaction = storage.begin_write().await?;
    transaction
        .set_setting(DOWNLOAD_RATE_KEY, Some("invalid"))
        .await?;
    transaction.commit().await?;
    assert!(matches!(
        storage.download_rate_limit().await,
        Err(StorageError::InvalidStoredValue { .. })
    ));
    Ok(())
}

fn account(id: &str, label: &str) -> AccountUpsert {
    AccountUpsert {
        id: id.to_owned(),
        label: label.to_owned(),
        login_name: Some(format!("{id}@example.test")),
        credential_ref: Some(format!("account:{id}:password")),
        enabled: true,
    }
}

fn work(work_id: &str, title: &str, maker_name: &str, published_at: &str) -> CachedWork {
    work_with_age(work_id, title, maker_name, published_at, "all")
}

fn work_with_age(
    work_id: &str,
    title: &str,
    maker_name: &str,
    published_at: &str,
    age_category: &str,
) -> CachedWork {
    CachedWork {
        work_id: work_id.to_owned(),
        title_json: format!(r#"{{"ja_JP":"{title}"}}"#),
        maker_id: Some(format!("maker-{maker_name}")),
        maker_json: Some(format!(r#"{{"ja_JP":"{maker_name}"}}"#)),
        work_type: Some("SOU".to_owned()),
        age_category: Some(age_category.to_owned()),
        thumbnail_url: Some(format!("https://img.example.test/{work_id}.jpg")),
        registered_at: Some(published_at.to_owned()),
        published_at: Some(published_at.to_owned()),
        updated_at: Some(published_at.to_owned()),
        raw_json: format!(r#"{{"workno":"{work_id}"}}"#),
        last_detail_sync_at: "2026-05-09T00:00:00.000Z".to_owned(),
    }
}

fn work_with_type(
    work_id: &str,
    title: &str,
    maker_name: &str,
    published_at: &str,
    work_type: &str,
) -> CachedWork {
    CachedWork {
        work_type: Some(work_type.to_owned()),
        ..work(work_id, title, maker_name, published_at)
    }
}

fn work_with_raw_json(
    work_id: &str,
    title: &str,
    maker_name: &str,
    published_at: &str,
    raw_json: &str,
) -> CachedWork {
    CachedWork {
        raw_json: raw_json.to_owned(),
        ..work(work_id, title, maker_name, published_at)
    }
}

fn account_work(work_id: &str, purchased_at: &str) -> AccountWork {
    AccountWork {
        work_id: work_id.to_owned(),
        purchased_at: Some(purchased_at.to_owned()),
    }
}

fn sync_commit(
    account_id: &str,
    sync_run_id: &str,
    works: Vec<CachedWork>,
    account_works: Vec<AccountWork>,
) -> AccountSyncCommit {
    AccountSyncCommit {
        sync_run_id: sync_run_id.to_owned(),
        account_id: account_id.to_owned(),
        started_at: "2026-05-09T00:00:00.000Z".to_owned(),
        completed_at: "2026-05-09T00:01:00.000Z".to_owned(),
        works,
        account_works,
    }
}

#[tokio::test]
async fn runs_embedded_migrations_once() -> Result<()> {
    let storage = Storage::open_in_memory().await?;

    storage.run_migrations().await?;
    storage.run_migrations().await?;

    let migration_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(&storage.pool)
        .await?;

    assert_eq!(migration_count, 9);

    Ok(())
}

#[tokio::test]
async fn upgrades_v3_2_2_database_without_changing_existing_library() -> Result<()> {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let path = temp_dir.path().join("upgrade.sqlite");
    let storage = Storage::open(&path).await?;
    // These five embedded migrations are unchanged from the v3.2.2 release.
    // Run the actual SQLx migration runner to retain real checksums/history.
    let legacy = Migrator::with_migrations(
        MIGRATOR
            .iter()
            .filter(|migration| migration.version <= 20260512000000)
            .cloned()
            .collect(),
    );
    assert_eq!(legacy.iter().count(), 5);
    legacy.run(&storage.pool).await?;
    let legacy_history: Vec<(i64, Vec<u8>)> =
        sqlx::query_as("SELECT version, checksum FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&storage.pool)
            .await?;
    let recovery_table_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE name = 'download_finalizations'",
    )
    .fetch_one(&storage.pool)
    .await?;
    assert_eq!(recovery_table_count, 0);

    let settings = storage
        .save_app_settings_patch(&AppSettingsPatch {
            library_root: Some("/fixture/library".to_owned()),
            download_root: Some("/fixture/staging".to_owned()),
            ..Default::default()
        })
        .await?;
    storage.save_account(&account("upgrade", "Upgrade")).await?;
    // Seed the old schema directly: current writes intentionally no longer
    // populate the scalar name columns required by the legacy schema.
    sqlx::raw_sql(
        r#"INSERT INTO works (work_id, title, title_json, maker_name, maker_json, published_at, raw_json, last_detail_sync_at)
           VALUES ('RJ000001', '既存の作品', '{"ja_JP":"既存の作品"}', 'Maker', '{"ja_JP":"Maker"}', '2026-05-01', '{}', '2026-05-26');
           INSERT INTO sync_runs (id, account_id, status, started_at, completed_at)
           VALUES ('upgrade-sync', 'upgrade', 'completed', '2026-05-26', '2026-05-26');
           INSERT INTO account_works (account_id, work_id, purchased_at, first_seen_at, last_seen_at, last_seen_sync_run_id)
           VALUES ('upgrade', 'RJ000001', '2026-05-02', '2026-05-26', '2026-05-26', 'upgrade-sync');"#,
    ).execute(&storage.pool).await?;
    storage
        .save_work_download(&WorkDownloadUpdate {
            work_id: "RJ000001".to_owned(),
            status: WorkDownloadStatus::Downloaded,
            local_path: Some("/fixture/library/手動フォルダー".to_owned()),
            staging_path: None,
            unpack_policy: "manual".to_owned(),
            bytes_received: 2048,
            bytes_total: Some(2048),
            error_code: None,
            error_message: None,
            started_at: Some("2026-05-26T00:00:00Z".to_owned()),
            completed_at: Some("2026-05-26T00:01:00Z".to_owned()),
            updated_at: "2026-05-26T00:01:00Z".to_owned(),
        })
        .await?;
    sqlx::query("INSERT INTO work_custom_tags (work_id, name, normalized_name) VALUES ('RJ000001', '保存済み', '保存済み'), ('RJ000001', 'Favorite', 'favorite')")
        .execute(&storage.pool).await?;
    let accounts = storage.accounts().await?;
    let detail = storage
        .product_detail("RJ000001", &LanguagePriority::default())
        .await?;
    let sync_runs = storage.sync_runs_for_account("upgrade").await?;
    storage.pool.close().await;

    // Reopen as the new app would, migrate twice, and reopen again to check persistence.
    let storage = Storage::open(&path).await?;
    storage.run_migrations().await?;
    storage.run_migrations().await?;
    storage.pool.close().await;
    let storage = Storage::open(&path).await?;
    assert_eq!(storage.app_settings().await?, settings);
    assert_eq!(storage.accounts().await?, accounts);
    assert_eq!(
        storage
            .product_detail("RJ000001", &LanguagePriority::default())
            .await?,
        detail
    );
    assert_eq!(storage.sync_runs_for_account("upgrade").await?, sync_runs);
    let upgraded_history: Vec<(i64, Vec<u8>)> = sqlx::query_as(
        "SELECT version, checksum FROM _sqlx_migrations WHERE version <= 20260512000000 ORDER BY version",
    )
    .fetch_all(&storage.pool)
    .await?;
    assert_eq!(upgraded_history, legacy_history);
    assert_eq!(storage.download_finalization("RJ000001").await?, None);
    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&storage.pool)
        .await?;
    assert_eq!(integrity, "ok");
    let violations = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&storage.pool)
        .await?;
    assert!(violations.is_empty());

    let recovery = DownloadFinalization {
        work_id: "RJ000001".to_owned(),
        operation_id: "upgrade-recovery".to_owned(),
        staging_path: "/fixture/staging/RJ000001".to_owned(),
        final_path: "/fixture/library/RJ000001".to_owned(),
        old_path: detail.download.local_path,
        temporary_path: "/fixture/library/.dldr-finalize-upgrade".to_owned(),
        committed: false,
    };
    storage.begin_download_finalization(&recovery).await?;
    assert_eq!(
        storage.download_finalization("RJ000001").await?,
        Some(recovery.clone())
    );
    storage
        .clear_download_finalization(&recovery.work_id, &recovery.operation_id)
        .await?;
    assert_eq!(storage.download_finalization("RJ000001").await?, None);
    storage.pool.close().await;
    Ok(())
}

#[tokio::test]
async fn commits_write_transaction() -> Result<()> {
    let storage = migrated_storage().await?;

    let mut transaction = storage.begin_write().await?;
    transaction
        .execute("CREATE TABLE tx_test (id INTEGER PRIMARY KEY, value TEXT NOT NULL)")
        .await?;
    transaction
        .execute("INSERT INTO tx_test (value) VALUES ('committed')")
        .await?;
    transaction.commit().await?;

    let value: String = sqlx::query_scalar("SELECT value FROM tx_test WHERE id = 1")
        .fetch_one(&storage.pool)
        .await?;

    assert_eq!(value, "committed");

    Ok(())
}

#[tokio::test]
async fn rolls_back_write_transaction() -> Result<()> {
    let storage = migrated_storage().await?;

    let mut transaction = storage.begin_write().await?;
    transaction
        .execute("CREATE TABLE tx_test (id INTEGER PRIMARY KEY, value TEXT NOT NULL)")
        .await?;
    transaction.commit().await?;

    let mut transaction = storage.begin_write().await?;
    transaction
        .execute("INSERT INTO tx_test (value) VALUES ('rolled back')")
        .await?;
    transaction.rollback().await?;

    let row = sqlx::query("SELECT COUNT(*) AS count FROM tx_test")
        .fetch_one(&storage.pool)
        .await?;
    let count: i64 = row.try_get("count")?;

    assert_eq!(count, 0);

    Ok(())
}

#[tokio::test]
async fn opens_file_database_and_creates_it_if_missing() -> Result<()> {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let path = temp_dir.path().join("storage.sqlite");

    let storage = Storage::open(&path).await?;
    storage.run_migrations().await?;
    drop(storage);

    assert!(path.exists());

    Ok(())
}

#[tokio::test]
async fn reads_empty_app_settings_by_default() -> Result<()> {
    let storage = migrated_storage().await?;

    assert_eq!(storage.app_settings().await?, AppSettings::default());

    Ok(())
}

#[tokio::test]
async fn saves_app_settings_in_one_transaction() -> Result<()> {
    let storage = migrated_storage().await?;
    let settings = storage
        .save_app_settings_patch(&AppSettingsPatch {
            library_root: Some("/library".to_owned()),
            download_root: Some("/downloads".to_owned()),
            ..Default::default()
        })
        .await?;

    assert_eq!(storage.app_settings().await?, settings);

    Ok(())
}

#[tokio::test]
async fn clears_missing_app_settings() -> Result<()> {
    let storage = migrated_storage().await?;

    storage
        .save_app_settings_patch(&AppSettingsPatch {
            library_root: Some("/library".to_owned()),
            download_root: Some("/downloads".to_owned()),
            ..Default::default()
        })
        .await?;
    storage
        .save_app_settings_patch(&AppSettingsPatch {
            library_root: Some("/library".to_owned()),
            download_root: None,
            ..Default::default()
        })
        .await?;

    assert_eq!(
        storage.app_settings().await?,
        AppSettings {
            ui_locale: UiLocale::default(),
            library_root: Some("/library".to_owned()),
            download_root: None,
            ..Default::default()
        }
    );

    Ok(())
}

#[tokio::test]
async fn creates_updates_and_disables_accounts() -> Result<()> {
    let storage = migrated_storage().await?;

    storage
        .save_account(&account("account-a", "Account A"))
        .await?;
    storage
        .record_account_login("account-a", "2026-05-09T00:02:00.000Z")
        .await?;
    storage
        .save_account(&AccountUpsert {
            id: "account-a".to_owned(),
            label: "Renamed".to_owned(),
            login_name: Some("renamed@example.test".to_owned()),
            credential_ref: None,
            enabled: true,
        })
        .await?;
    storage.set_account_enabled("account-a", false).await?;

    let accounts = storage.accounts().await?;

    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].id, "account-a");
    assert_eq!(accounts[0].label, "Renamed");
    assert_eq!(
        accounts[0].login_name,
        Some("renamed@example.test".to_owned())
    );
    assert_eq!(accounts[0].credential_ref, None);
    assert!(!accounts[0].enabled);
    assert_eq!(
        accounts[0].last_login_at,
        Some("2026-05-09T00:02:00.000Z".to_owned())
    );

    Ok(())
}

#[tokio::test]
async fn deletes_account_source_without_deleting_global_work_cache() -> Result<()> {
    let storage = migrated_storage().await?;
    storage
        .save_account(&account("account-a", "Account A"))
        .await?;
    storage
        .save_account(&account("account-b", "Account B"))
        .await?;
    storage
        .commit_account_sync(&sync_commit(
            "account-a",
            "sync-a-1",
            vec![
                work(
                    "RJ000001",
                    "Shared Work",
                    "Maker One",
                    "2026-01-01T00:00:00Z",
                ),
                work(
                    "RJ000002",
                    "Downloaded Local Work",
                    "Maker Two",
                    "2026-01-02T00:00:00Z",
                ),
                work(
                    "RJ000003",
                    "Hidden After Removal",
                    "Maker Three",
                    "2026-01-03T00:00:00Z",
                ),
            ],
            vec![
                account_work("RJ000001", "2026-02-01T00:00:00Z"),
                account_work("RJ000002", "2026-02-02T00:00:00Z"),
                account_work("RJ000003", "2026-02-03T00:00:00Z"),
            ],
        ))
        .await?;
    storage
        .commit_account_sync(&sync_commit(
            "account-b",
            "sync-b-1",
            vec![work(
                "RJ000001",
                "Shared Work",
                "Maker One",
                "2026-01-01T00:00:00Z",
            )],
            vec![account_work("RJ000001", "2026-03-01T00:00:00Z")],
        ))
        .await?;
    storage
        .save_work_download(&WorkDownloadUpdate {
            work_id: "RJ000002".to_owned(),
            status: WorkDownloadStatus::Downloaded,
            local_path: Some("/library/RJ000002".to_owned()),
            staging_path: None,
            unpack_policy: "manual".to_owned(),
            bytes_received: 0,
            bytes_total: None,
            error_code: None,
            error_message: None,
            started_at: Some("2026-05-11T00:00:00.000Z".to_owned()),
            completed_at: Some("2026-05-11T00:00:00.000Z".to_owned()),
            updated_at: "2026-05-11T00:00:00.000Z".to_owned(),
        })
        .await?;

    storage.delete_account("account-a").await?;

    let accounts = storage.accounts().await?;
    let page = storage
        .list_products(&ProductListQuery {
            sort: ProductSort::TitleAsc,
            ..ProductListQuery::default()
        })
        .await?;
    let work_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM works")
        .fetch_one(&storage.pool)
        .await?;
    let removed_owner_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM account_works WHERE account_id = 'account-a'")
            .fetch_one(&storage.pool)
            .await?;
    let removed_sync_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sync_runs WHERE account_id = 'account-a'")
            .fetch_one(&storage.pool)
            .await?;

    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].id, "account-b");
    assert_eq!(page.total_count, 2);
    assert_eq!(page.products[0].work_id, "RJ000002");
    assert_eq!(page.products[0].owners[0].label, LOCAL_PRODUCT_OWNER_LABEL);
    assert_eq!(page.products[1].work_id, "RJ000001");
    assert_eq!(page.products[1].owners[0].account_id, "account-b");
    assert_eq!(work_count, 3);
    assert_eq!(removed_owner_count, 0);
    assert_eq!(removed_sync_count, 0);

    Ok(())
}

#[tokio::test]
async fn missing_account_updates_return_not_found() -> Result<()> {
    let storage = migrated_storage().await?;

    assert!(matches!(
        storage.set_account_enabled("missing", true).await,
        Err(StorageError::NotFound {
            entity: "account",
            id
        }) if id == "missing"
    ));
    assert!(matches!(
        storage.delete_account("missing").await,
        Err(StorageError::NotFound {
            entity: "account",
            id
        }) if id == "missing"
    ));

    Ok(())
}

#[tokio::test]
async fn unified_product_list_collapses_duplicate_ownership() -> Result<()> {
    let storage = migrated_storage().await?;
    storage
        .save_account(&account("account-a", "Account A"))
        .await?;
    storage
        .save_account(&account("account-b", "Account B"))
        .await?;

    storage
        .commit_account_sync(&sync_commit(
            "account-a",
            "sync-a-1",
            vec![
                work(
                    "RJ000001",
                    "Shared Work",
                    "Maker One",
                    "2026-01-01T00:00:00Z",
                ),
                work(
                    "RJ000002",
                    "Account A Work",
                    "Maker Two",
                    "2026-01-02T00:00:00Z",
                ),
            ],
            vec![
                account_work("RJ000001", "2026-02-01T00:00:00Z"),
                account_work("RJ000002", "2026-02-02T00:00:00Z"),
            ],
        ))
        .await?;
    storage
        .commit_account_sync(&sync_commit(
            "account-b",
            "sync-b-1",
            vec![work(
                "RJ000001",
                "Shared Work",
                "Maker One",
                "2026-01-01T00:00:00Z",
            )],
            vec![account_work("RJ000001", "2026-03-01T00:00:00Z")],
        ))
        .await?;

    let page = storage
        .list_products(&ProductListQuery {
            sort: ProductSort::TitleAsc,
            ..ProductListQuery::default()
        })
        .await?;

    assert_eq!(page.total_count, 2);
    assert_eq!(page.products.len(), 2);
    assert_eq!(page.products[1].work_id, "RJ000001");
    assert_eq!(page.products[1].owners.len(), 2);
    assert_eq!(page.products[1].owners[0].account_id, "account-a");
    assert_eq!(page.products[1].owners[1].account_id, "account-b");
    assert_eq!(
        page.products[1].earliest_purchased_at,
        Some("2026-02-01T00:00:00Z".to_owned())
    );
    assert_eq!(
        page.products[1].latest_purchased_at,
        Some("2026-03-01T00:00:00Z".to_owned())
    );

    let filtered_page = storage
        .list_products(&ProductListQuery {
            account_id: Some("account-b".to_owned()),
            ..ProductListQuery::default()
        })
        .await?;

    assert_eq!(filtered_page.total_count, 1);
    assert_eq!(filtered_page.products[0].work_id, "RJ000001");
    assert_eq!(filtered_page.products[0].owners.len(), 2);

    Ok(())
}

#[tokio::test]
async fn disabled_accounts_are_excluded_from_default_product_list() -> Result<()> {
    let storage = migrated_storage().await?;
    storage
        .save_account(&account("account-a", "Account A"))
        .await?;
    storage
        .save_account(&account("account-b", "Account B"))
        .await?;

    storage
        .commit_account_sync(&sync_commit(
            "account-a",
            "sync-a-1",
            vec![
                work(
                    "RJ000001",
                    "Shared Work",
                    "Maker One",
                    "2026-01-01T00:00:00Z",
                ),
                work("RJ000002", "Only A", "Maker Two", "2026-01-02T00:00:00Z"),
            ],
            vec![
                account_work("RJ000001", "2026-02-01T00:00:00Z"),
                account_work("RJ000002", "2026-02-02T00:00:00Z"),
            ],
        ))
        .await?;
    storage
        .commit_account_sync(&sync_commit(
            "account-b",
            "sync-b-1",
            vec![work(
                "RJ000001",
                "Shared Work",
                "Maker One",
                "2026-01-01T00:00:00Z",
            )],
            vec![account_work("RJ000001", "2026-03-01T00:00:00Z")],
        ))
        .await?;

    storage.set_account_enabled("account-a", false).await?;

    let page = storage.list_products(&ProductListQuery::default()).await?;

    assert_eq!(page.total_count, 1);
    assert_eq!(page.products[0].work_id, "RJ000001");
    assert_eq!(page.products[0].owners.len(), 1);
    assert_eq!(page.products[0].owners[0].account_id, "account-b");

    Ok(())
}

#[tokio::test]
async fn product_list_hides_missing_detail_placeholders_without_deleting_cache() -> Result<()> {
    let storage = migrated_storage().await?;
    storage
        .save_account(&account("account-a", "Account A"))
        .await?;

    storage
        .commit_account_sync(&sync_commit(
            "account-a",
            "sync-a-1",
            vec![
                work(
                    "RJ000001",
                    "Visible Work",
                    "Maker One",
                    "2026-01-01T00:00:00Z",
                ),
                work_with_raw_json(
                    "RJ000002",
                    "RJ000002",
                    "",
                    "2026-01-02T00:00:00Z",
                    r#"{
                        "workno": "RJ000002",
                        "source": "content/sales",
                        "detail_status": "missing_from_content_works",
                        "sales_date": "2026-01-02T00:00:00Z"
                    }"#,
                ),
            ],
            vec![
                account_work("RJ000001", "2026-02-01T00:00:00Z"),
                account_work("RJ000002", "2026-02-02T00:00:00Z"),
            ],
        ))
        .await?;

    let page = storage.list_products(&ProductListQuery::default()).await?;
    let search_page = storage
        .list_products(&ProductListQuery {
            search: Some("RJ000002".to_owned()),
            ..ProductListQuery::default()
        })
        .await?;
    let cached_placeholder_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM works WHERE work_id = 'RJ000002'")
            .fetch_one(&storage.pool)
            .await?;
    let current_ownership_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM account_works WHERE work_id = 'RJ000002' AND is_current = 1",
    )
    .fetch_one(&storage.pool)
    .await?;

    assert_eq!(page.total_count, 1);
    assert_eq!(page.products.len(), 1);
    assert_eq!(page.products[0].work_id, "RJ000001");
    assert_eq!(search_page.total_count, 0);
    assert_eq!(search_page.products.len(), 0);
    assert_eq!(cached_placeholder_count, 1);
    assert_eq!(current_ownership_count, 1);

    Ok(())
}

#[tokio::test]
async fn product_list_groups_source_credits_from_raw_json() -> Result<()> {
    let storage = migrated_storage().await?;
    storage
        .save_account(&account("account-a", "Account A"))
        .await?;
    storage
        .commit_account_sync(&sync_commit(
            "account-a",
            "sync-a-1",
            vec![work_with_raw_json(
                "RJ000001",
                "Credit Work",
                "Maker One",
                "2026-01-01T00:00:00Z",
                r#"{
                    "workno": "RJ000001",
                    "tags": [
                        { "class": "genre", "name": "ASMR" },
                        { "class": "voice_by", "name": "Voice One" },
                        { "class": "voice_by", "name": "Voice One" },
                        { "class": "voice_by", "name": "Voice Two" },
                        { "class": "illust_by", "name": "Illust One" },
                        { "class": "scenario_by", "name": "Scenario One" },
                        { "class": "created_by", "name": "Creator One" },
                        { "class": "music_by", "name": "Music One" },
                        { "class": "other_by", "name": "Other One" },
                        { "class": "unknown_by", "name": "Unknown Credit" }
                    ]
                }"#,
            )],
            vec![account_work("RJ000001", "2026-02-01T00:00:00Z")],
        ))
        .await?;

    let page = storage.list_products(&ProductListQuery::default()).await?;

    assert_eq!(page.products[0].credit_groups.len(), 6);
    assert_eq!(page.products[0].credit_groups[0].kind, "voice");
    assert_eq!(page.products[0].credit_groups[0].label, "CV");
    assert_eq!(
        page.products[0].credit_groups[0].names,
        vec!["Voice One".to_owned(), "Voice Two".to_owned()]
    );
    assert_eq!(page.products[0].credit_groups[1].kind, "illust");
    assert_eq!(page.products[0].credit_groups[2].kind, "scenario");
    assert_eq!(page.products[0].credit_groups[3].kind, "creator");
    assert_eq!(page.products[0].credit_groups[4].kind, "music");
    assert_eq!(page.products[0].credit_groups[5].kind, "other");
    assert_eq!(
        page.products[0].credit_groups[5].names,
        vec!["Other One".to_owned(), "Unknown Credit".to_owned()]
    );

    Ok(())
}

#[tokio::test]
async fn product_list_includes_content_size_from_raw_json() -> Result<()> {
    let storage = migrated_storage().await?;
    storage
        .save_account(&account("account-a", "Account A"))
        .await?;
    storage
        .commit_account_sync(&sync_commit(
            "account-a",
            "sync-a-1",
            vec![work_with_raw_json(
                "RJ000001",
                "Sized Work",
                "Maker One",
                "2026-01-01T00:00:00Z",
                r#"{"workno":"RJ000001","content_size":123456}"#,
            )],
            vec![account_work("RJ000001", "2026-02-01T00:00:00Z")],
        ))
        .await?;

    let page = storage.list_products(&ProductListQuery::default()).await?;

    assert_eq!(page.products[0].content_size_bytes, Some(123456));

    Ok(())
}

#[tokio::test]
async fn product_detail_assembles_cached_metadata_without_new_storage() -> Result<()> {
    let storage = migrated_storage().await?;
    storage
        .save_account(&account("account-a", "Account A"))
        .await?;
    storage
        .commit_account_sync(&sync_commit(
            "account-a",
            "sync-a-1",
            vec![CachedWork {
                title_json: r#"{"en_US":"Detail Work","ja_JP":"詳細作品"}"#.to_owned(),
                maker_json: Some(r#"{"ja_JP":"Maker JP","en_US":"Maker EN"}"#.to_owned()),
                raw_json: r#"{
                    "workno": "RJ000001",
                    "content_size": "2048",
                    "tags": [
                        { "class": "genre", "name": "ASMR" },
                        { "class": "voice_by", "name": "Voice One" },
                        { "class": "genre", "name": "ASMR" }
                    ],
                    "viewer_type": "download"
                }"#
                .to_owned(),
                ..work(
                    "RJ000001",
                    "Detail Work",
                    "Maker JP",
                    "2026-01-01T00:00:00Z",
                )
            }],
            vec![account_work("RJ000001", "2026-02-01T00:00:00Z")],
        ))
        .await?;
    storage
        .save_work_download(&WorkDownloadUpdate {
            work_id: "RJ000001".to_owned(),
            status: WorkDownloadStatus::Downloaded,
            local_path: Some("/library/RJ000001".to_owned()),
            staging_path: None,
            unpack_policy: "unpack_when_recognized".to_owned(),
            bytes_received: 2048,
            bytes_total: Some(2048),
            error_code: None,
            error_message: None,
            started_at: Some("2026-05-11T00:00:00.000Z".to_owned()),
            completed_at: Some("2026-05-11T00:01:00.000Z".to_owned()),
            updated_at: "2026-05-11T00:01:00.000Z".to_owned(),
        })
        .await?;

    let detail = storage
        .product_detail("RJ000001", &LanguagePriority::default())
        .await?;

    assert_eq!(detail.work_id, "RJ000001");
    assert_eq!(detail.content_size_bytes, Some(2048));
    assert_eq!(detail.title_variants.len(), 2);
    assert_eq!(detail.maker_names.len(), 2);
    assert_eq!(detail.credit_groups[0].names, vec!["Voice One".to_owned()]);
    assert_eq!(detail.tags.len(), 2);
    assert_eq!(detail.tags[0].class, "genre");
    assert_eq!(detail.download.status, WorkDownloadStatus::Downloaded);
    assert_eq!(detail.owners[0].label, "Account A");

    Ok(())
}

#[tokio::test]
async fn complete_sync_marks_missing_ownership_stale_without_deleting_history() -> Result<()> {
    let storage = migrated_storage().await?;
    storage
        .save_account(&account("account-a", "Account A"))
        .await?;

    storage
        .commit_account_sync(&sync_commit(
            "account-a",
            "sync-a-1",
            vec![
                work("RJ000001", "Old Work", "Maker One", "2026-01-01T00:00:00Z"),
                work("RJ000002", "Kept Work", "Maker Two", "2026-01-02T00:00:00Z"),
            ],
            vec![
                account_work("RJ000001", "2026-02-01T00:00:00Z"),
                account_work("RJ000002", "2026-02-02T00:00:00Z"),
            ],
        ))
        .await?;
    storage
        .commit_account_sync(&sync_commit(
            "account-a",
            "sync-a-2",
            vec![work(
                "RJ000002",
                "Kept Work",
                "Maker Two",
                "2026-01-02T00:00:00Z",
            )],
            vec![account_work("RJ000002", "2026-02-02T00:00:00Z")],
        ))
        .await?;

    let page = storage.list_products(&ProductListQuery::default()).await?;
    let stale_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM account_works WHERE is_current = 0")
            .fetch_one(&storage.pool)
            .await?;
    let sync_runs = storage.sync_runs_for_account("account-a").await?;
    let accounts = storage.accounts().await?;

    assert_eq!(page.total_count, 1);
    assert_eq!(page.products[0].work_id, "RJ000002");
    assert_eq!(stale_count, 1);
    assert_eq!(sync_runs.len(), 2);
    assert_eq!(sync_runs[0].status, SyncRunStatus::Completed);
    assert_eq!(
        accounts[0].last_sync_at,
        Some("2026-05-09T00:01:00.000Z".to_owned())
    );

    Ok(())
}

#[tokio::test]
async fn sync_failure_is_recorded_without_marking_existing_cache_stale() -> Result<()> {
    let storage = migrated_storage().await?;
    storage
        .save_account(&account("account-a", "Account A"))
        .await?;

    storage
        .commit_account_sync(&sync_commit(
            "account-a",
            "sync-a-1",
            vec![work(
                "RJ000001",
                "Existing Work",
                "Maker One",
                "2026-01-01T00:00:00Z",
            )],
            vec![account_work("RJ000001", "2026-02-01T00:00:00Z")],
        ))
        .await?;
    storage
        .record_sync_failure(&SyncFailure {
            sync_run_id: "sync-a-failed".to_owned(),
            account_id: "account-a".to_owned(),
            started_at: "2026-05-09T00:02:00.000Z".to_owned(),
            completed_at: "2026-05-09T00:03:00.000Z".to_owned(),
            error_code: Some("network".to_owned()),
            error_message: Some("network unavailable".to_owned()),
        })
        .await?;

    let page = storage.list_products(&ProductListQuery::default()).await?;
    let sync_runs = storage.sync_runs_for_account("account-a").await?;

    assert_eq!(page.total_count, 1);
    assert_eq!(sync_runs.len(), 2);
    assert_eq!(sync_runs[0].status, SyncRunStatus::Failed);
    assert_eq!(sync_runs[0].error_code, Some("network".to_owned()));

    Ok(())
}

#[tokio::test]
async fn sync_cancellation_is_recorded_with_cancelled_status() -> Result<()> {
    let storage = migrated_storage().await?;
    storage
        .save_account(&account("account-a", "Account A"))
        .await?;

    storage
        .record_sync_cancellation(&SyncCancellation {
            sync_run_id: "sync-a-cancelled".to_owned(),
            account_id: "account-a".to_owned(),
            started_at: "2026-05-09T00:02:00.000Z".to_owned(),
            completed_at: "2026-05-09T00:03:00.000Z".to_owned(),
        })
        .await?;

    let sync_runs = storage.sync_runs_for_account("account-a").await?;

    assert_eq!(sync_runs.len(), 1);
    assert_eq!(sync_runs[0].status, SyncRunStatus::Cancelled);
    assert_eq!(sync_runs[0].error_code, Some("cancelled".to_owned()));

    Ok(())
}

#[tokio::test]
async fn failed_sync_commit_rolls_back_partial_writes() -> Result<()> {
    let storage = migrated_storage().await?;
    storage
        .save_account(&account("account-a", "Account A"))
        .await?;

    let result = storage
        .commit_account_sync(&sync_commit(
            "account-a",
            "sync-a-1",
            Vec::new(),
            vec![account_work("RJ000001", "2026-02-01T00:00:00Z")],
        ))
        .await;

    assert!(result.is_err());
    assert_eq!(
        storage.sync_runs_for_account("account-a").await?,
        Vec::new()
    );
    assert_eq!(
        storage
            .list_products(&ProductListQuery::default())
            .await?
            .total_count,
        0
    );

    Ok(())
}

#[tokio::test]
async fn explicit_write_transaction_can_roll_back_library_changes() -> Result<()> {
    let storage = migrated_storage().await?;
    let mut transaction = storage.begin_write().await?;

    transaction
        .upsert_account(&account("account-a", "Account A"))
        .await?;
    transaction
        .commit_account_sync(&sync_commit(
            "account-a",
            "sync-a-1",
            vec![work(
                "RJ000001",
                "Rolled Back Work",
                "Maker One",
                "2026-01-01T00:00:00Z",
            )],
            vec![account_work("RJ000001", "2026-02-01T00:00:00Z")],
        ))
        .await?;
    transaction.rollback().await?;

    assert_eq!(storage.accounts().await?, Vec::new());
    assert_eq!(
        storage
            .list_products(&ProductListQuery::default())
            .await?
            .total_count,
        0
    );

    Ok(())
}

#[tokio::test]
async fn product_list_search_and_sort_use_localized_names() -> Result<()> {
    let storage = migrated_storage().await?;
    storage
        .save_account(&account("account-a", "Account A"))
        .await?;
    storage
        .commit_account_sync(&sync_commit(
            "account-a",
            "sync-a-1",
            vec![
                work("RJ000001", "Alpha", "Circle One", "2026-01-01T00:00:00Z"),
                work("RJ000002", "Beta", "Special Maker", "2026-03-01T00:00:00Z"),
            ],
            vec![
                account_work("RJ000001", "2026-02-01T00:00:00Z"),
                account_work("RJ000002", "2026-02-02T00:00:00Z"),
            ],
        ))
        .await?;

    let search_page = storage
        .list_products(&ProductListQuery {
            search: Some("Special".to_owned()),
            ..ProductListQuery::default()
        })
        .await?;
    let sorted_page = storage
        .list_products(&ProductListQuery {
            sort: ProductSort::PublishedAtDesc,
            ..ProductListQuery::default()
        })
        .await?;

    assert_eq!(search_page.total_count, 1);
    assert_eq!(search_page.products[0].work_id, "RJ000002");
    assert_eq!(sorted_page.products[0].work_id, "RJ000002");

    Ok(())
}

#[tokio::test]
async fn product_list_searches_cached_tag_names() -> Result<()> {
    let storage = migrated_storage().await?;
    storage
        .save_account(&account("account-a", "Account A"))
        .await?;
    storage
        .commit_account_sync(&sync_commit(
            "account-a",
            "sync-a-1",
            vec![work_with_raw_json(
                "RJ000001",
                "Tagged Work",
                "Circle One",
                "2026-01-01T00:00:00Z",
                r#"{
                    "workno": "RJ000001",
                    "tags": [
                        { "class": "genre", "name": "Deep Sleep" },
                        { "class": "voice_by", "name": "Voice Person" }
                    ]
                }"#,
            )],
            vec![account_work("RJ000001", "2026-02-01T00:00:00Z")],
        ))
        .await?;

    let genre_page = storage
        .list_products(&ProductListQuery {
            search: Some("Deep Sleep".to_owned()),
            ..ProductListQuery::default()
        })
        .await?;
    let credit_page = storage
        .list_products(&ProductListQuery {
            search: Some("Voice Person".to_owned()),
            ..ProductListQuery::default()
        })
        .await?;

    assert_eq!(genre_page.total_count, 1);
    assert_eq!(genre_page.products[0].work_id, "RJ000001");
    assert_eq!(credit_page.total_count, 1);
    assert_eq!(credit_page.products[0].work_id, "RJ000001");

    Ok(())
}

#[tokio::test]
async fn product_filter_facets_return_all_matching_makers() -> Result<()> {
    let storage = migrated_storage().await?;
    storage
        .save_account(&account("account-a", "Account A"))
        .await?;

    let works = (0..30)
        .map(|index| {
            work(
                &format!("RJ{index:06}"),
                &format!("Work {index:02}"),
                &format!("Maker {index:02}"),
                "2026-01-01T00:00:00Z",
            )
        })
        .collect::<Vec<_>>();
    let account_works = works
        .iter()
        .map(|work| account_work(&work.work_id, "2026-02-01T00:00:00Z"))
        .collect::<Vec<_>>();

    storage
        .commit_account_sync(&sync_commit("account-a", "sync-a-1", works, account_works))
        .await?;

    let facets = storage
        .product_filter_facets(&ProductListQuery {
            maker_keys: vec!["id:maker-Maker 00".to_owned()],
            ..ProductListQuery::default()
        })
        .await?;

    assert_eq!(facets.makers.len(), 30);
    assert_eq!(facets.makers[0].name, "Maker 00");
    assert_eq!(facets.makers[29].name, "Maker 29");
    assert!(facets.makers.iter().all(|maker| maker.count == 1));

    Ok(())
}

#[tokio::test]
async fn product_list_can_filter_by_age_category() -> Result<()> {
    let storage = migrated_storage().await?;
    storage
        .save_account(&account("account-a", "Account A"))
        .await?;
    storage
        .commit_account_sync(&sync_commit(
            "account-a",
            "sync-a-1",
            vec![
                work_with_age(
                    "RJ000001",
                    "All Ages",
                    "Circle One",
                    "2026-01-01T00:00:00Z",
                    "all",
                ),
                work_with_age(
                    "RJ000002",
                    "Fifteen",
                    "Circle Two",
                    "2026-01-02T00:00:00Z",
                    "r15",
                ),
                work_with_age(
                    "RJ000003",
                    "Eighteen",
                    "Circle Three",
                    "2026-01-03T00:00:00Z",
                    "r18",
                ),
            ],
            vec![
                account_work("RJ000001", "2026-02-01T00:00:00Z"),
                account_work("RJ000002", "2026-02-02T00:00:00Z"),
                account_work("RJ000003", "2026-02-03T00:00:00Z"),
            ],
        ))
        .await?;

    let all_ages_page = storage
        .list_products(&ProductListQuery {
            age_category: Some(ProductAgeCategory::All),
            ..ProductListQuery::default()
        })
        .await?;
    let r15_page = storage
        .list_products(&ProductListQuery {
            age_category: Some(ProductAgeCategory::R15),
            ..ProductListQuery::default()
        })
        .await?;
    let r18_page = storage
        .list_products(&ProductListQuery {
            age_category: Some(ProductAgeCategory::R18),
            ..ProductListQuery::default()
        })
        .await?;

    assert_eq!(all_ages_page.total_count, 1);
    assert_eq!(all_ages_page.products[0].work_id, "RJ000001");
    assert_eq!(r15_page.total_count, 1);
    assert_eq!(r15_page.products[0].work_id, "RJ000002");
    assert_eq!(r18_page.total_count, 1);
    assert_eq!(r18_page.products[0].work_id, "RJ000003");

    Ok(())
}

#[tokio::test]
async fn product_list_can_filter_by_type_group() -> Result<()> {
    let storage = migrated_storage().await?;
    storage
        .save_account(&account("account-a", "Account A"))
        .await?;
    storage
        .commit_account_sync(&sync_commit(
            "account-a",
            "sync-a-1",
            vec![
                work_with_type(
                    "RJ000001",
                    "Voice Work",
                    "Circle One",
                    "2026-01-01T00:00:00Z",
                    "SOU",
                ),
                work_with_type(
                    "RJ000002",
                    "Video Work",
                    "Circle Two",
                    "2026-01-02T00:00:00Z",
                    "MOV",
                ),
                work_with_type(
                    "RJ000003",
                    "Game Work",
                    "Circle Three",
                    "2026-01-03T00:00:00Z",
                    "SLN",
                ),
                work_with_type(
                    "RJ000004",
                    "Comic Work",
                    "Circle Four",
                    "2026-01-04T00:00:00Z",
                    "COM",
                ),
                work_with_type(
                    "RJ000005",
                    "Other Work",
                    "Circle Five",
                    "2026-01-05T00:00:00Z",
                    "SOF",
                ),
                work_with_type(
                    "RJ000006",
                    "Voice Comic Work",
                    "Circle Six",
                    "2026-01-06T00:00:00Z",
                    "VCM",
                ),
            ],
            vec![
                account_work("RJ000001", "2026-02-01T00:00:00Z"),
                account_work("RJ000002", "2026-02-02T00:00:00Z"),
                account_work("RJ000003", "2026-02-03T00:00:00Z"),
                account_work("RJ000004", "2026-02-04T00:00:00Z"),
                account_work("RJ000005", "2026-02-05T00:00:00Z"),
                account_work("RJ000006", "2026-02-06T00:00:00Z"),
            ],
        ))
        .await?;

    let audio_page = storage
        .list_products(&ProductListQuery {
            type_group: Some(ProductTypeGroup::Audio),
            ..ProductListQuery::default()
        })
        .await?;
    let video_page = storage
        .list_products(&ProductListQuery {
            type_group: Some(ProductTypeGroup::Video),
            ..ProductListQuery::default()
        })
        .await?;
    let game_page = storage
        .list_products(&ProductListQuery {
            type_group: Some(ProductTypeGroup::Game),
            ..ProductListQuery::default()
        })
        .await?;
    let image_page = storage
        .list_products(&ProductListQuery {
            type_group: Some(ProductTypeGroup::Image),
            ..ProductListQuery::default()
        })
        .await?;
    let other_page = storage
        .list_products(&ProductListQuery {
            type_group: Some(ProductTypeGroup::Other),
            ..ProductListQuery::default()
        })
        .await?;

    assert_eq!(audio_page.products[0].work_id, "RJ000001");
    assert_eq!(video_page.products[0].work_id, "RJ000002");
    assert_eq!(game_page.products[0].work_id, "RJ000003");
    assert_eq!(
        image_page
            .products
            .iter()
            .map(|product| product.work_id.as_str())
            .collect::<Vec<_>>(),
        ["RJ000006", "RJ000004"]
    );
    assert_eq!(other_page.products[0].work_id, "RJ000005");

    Ok(())
}

#[tokio::test]
async fn product_list_includes_download_state() -> Result<()> {
    let storage = migrated_storage().await?;
    storage
        .save_account(&account("account-a", "Account A"))
        .await?;
    storage
        .commit_account_sync(&sync_commit(
            "account-a",
            "sync-a-1",
            vec![work(
                "RJ000001",
                "Downloaded Work",
                "Circle One",
                "2026-01-01T00:00:00Z",
            )],
            vec![account_work("RJ000001", "2026-02-01T00:00:00Z")],
        ))
        .await?;

    let initial_page = storage.list_products(&ProductListQuery::default()).await?;

    assert_eq!(
        initial_page.products[0].download.status,
        WorkDownloadStatus::NotDownloaded
    );

    storage
        .save_work_download(&WorkDownloadUpdate {
            work_id: "RJ000001".to_owned(),
            status: WorkDownloadStatus::Downloaded,
            local_path: Some("/library/RJ000001".to_owned()),
            staging_path: Some("/downloads/RJ000001".to_owned()),
            unpack_policy: "unpack_when_recognized".to_owned(),
            bytes_received: 42,
            bytes_total: Some(42),
            error_code: None,
            error_message: None,
            started_at: Some("2026-05-11T00:00:00.000Z".to_owned()),
            completed_at: Some("2026-05-11T00:01:00.000Z".to_owned()),
            updated_at: "2026-05-11T00:01:00.000Z".to_owned(),
        })
        .await?;

    let page = storage.list_products(&ProductListQuery::default()).await?;
    let state = &page.products[0].download;

    assert_eq!(state.status, WorkDownloadStatus::Downloaded);
    assert_eq!(state.local_path, Some("/library/RJ000001".to_owned()));
    assert_eq!(state.bytes_received, 42);
    assert_eq!(state.bytes_total, Some(42));

    storage.delete_work_download("RJ000001").await?;
    let removed_page = storage.list_products(&ProductListQuery::default()).await?;

    assert_eq!(
        removed_page.products[0].download.status,
        WorkDownloadStatus::NotDownloaded
    );
    assert_eq!(removed_page.products[0].download.local_path, None);

    Ok(())
}

#[tokio::test]
async fn product_list_includes_local_only_download_imports() -> Result<()> {
    let storage = migrated_storage().await?;
    storage
        .import_local_work_downloads(&[LocalWorkDownloadImport {
            work: CachedWork {
                work_id: "RJ123456".to_owned(),
                title_json: r#"{"en_US":"[RJ123456] Local Folder"}"#.to_owned(),
                maker_id: None,
                maker_json: None,
                work_type: None,
                age_category: None,
                thumbnail_url: None,
                registered_at: None,
                published_at: None,
                updated_at: None,
                raw_json:
                    r#"{"workno":"RJ123456","source":"local_scan","detail_status":"local_only"}"#
                        .to_owned(),
                last_detail_sync_at: "2026-05-11T00:00:00.000Z".to_owned(),
            },
            download: WorkDownloadUpdate {
                work_id: "RJ123456".to_owned(),
                status: WorkDownloadStatus::Downloaded,
                local_path: Some("/library/[RJ123456] Local Folder".to_owned()),
                staging_path: None,
                unpack_policy: "manual".to_owned(),
                bytes_received: 0,
                bytes_total: None,
                error_code: None,
                error_message: None,
                started_at: Some("2026-05-11T00:00:00.000Z".to_owned()),
                completed_at: Some("2026-05-11T00:00:00.000Z".to_owned()),
                updated_at: "2026-05-11T00:00:00.000Z".to_owned(),
            },
        }])
        .await?;

    let page = storage.list_products(&ProductListQuery::default()).await?;
    let account_page = storage
        .list_products(&ProductListQuery {
            account_id: Some("account-a".to_owned()),
            ..ProductListQuery::default()
        })
        .await?;

    assert_eq!(page.total_count, 1);
    assert_eq!(page.products[0].work_id, "RJ123456");
    assert_eq!(page.products[0].title, "[RJ123456] Local Folder");
    assert_eq!(page.products[0].owners.len(), 1);
    assert_eq!(
        page.products[0].owners[0].account_id,
        LOCAL_PRODUCT_OWNER_ID
    );
    assert_eq!(page.products[0].owners[0].label, LOCAL_PRODUCT_OWNER_LABEL);
    assert_eq!(
        page.products[0].download.status,
        WorkDownloadStatus::Downloaded
    );
    assert_eq!(account_page.total_count, 0);
    assert_eq!(account_page.products.len(), 0);

    Ok(())
}

#[tokio::test]
async fn local_download_import_can_hydrate_placeholder_metadata() -> Result<()> {
    let storage = migrated_storage().await?;
    storage
        .import_local_work_downloads(&[LocalWorkDownloadImport {
            work: CachedWork {
                work_id: "RJ123456".to_owned(),
                title_json: r#"{"en_US":"RJ123456"}"#.to_owned(),
                maker_id: None,
                maker_json: None,
                work_type: None,
                age_category: None,
                thumbnail_url: None,
                registered_at: None,
                published_at: None,
                updated_at: None,
                raw_json:
                    r#"{"workno":"RJ123456","source":"local_scan","detail_status":"local_only"}"#
                        .to_owned(),
                last_detail_sync_at: "2026-05-11T00:00:00.000Z".to_owned(),
            },
            download: WorkDownloadUpdate {
                work_id: "RJ123456".to_owned(),
                status: WorkDownloadStatus::Downloaded,
                local_path: Some("/library/RJ123456".to_owned()),
                staging_path: None,
                unpack_policy: "manual".to_owned(),
                bytes_received: 0,
                bytes_total: None,
                error_code: None,
                error_message: None,
                started_at: Some("2026-05-11T00:00:00.000Z".to_owned()),
                completed_at: Some("2026-05-11T00:00:00.000Z".to_owned()),
                updated_at: "2026-05-11T00:00:00.000Z".to_owned(),
            },
        }])
        .await?;

    assert!(
        storage
            .work_needs_local_metadata_hydration("RJ123456")
            .await?
    );

    storage
        .import_local_work_downloads_with_metadata(
            &[],
            &[CachedWork {
                raw_json:
                    r#"{"workno":"RJ123456","source":"public_product","contents_file_size":2048}"#
                        .to_owned(),
                ..work(
                    "RJ123456",
                    "Hydrated Work",
                    "Public Maker",
                    "2026-01-01T00:00:00Z",
                )
            }],
        )
        .await?;

    let page = storage.list_products(&ProductListQuery::default()).await?;

    assert_eq!(page.products[0].title, "Hydrated Work");
    assert_eq!(page.products[0].maker_name.as_deref(), Some("Public Maker"));
    assert_eq!(page.products[0].content_size_bytes, Some(2048));
    assert!(
        !storage
            .work_needs_local_metadata_hydration("RJ123456")
            .await?
    );

    Ok(())
}

#[tokio::test]
async fn product_list_filters_and_searches_local_only_source() -> Result<()> {
    let storage = migrated_storage().await?;
    storage
        .save_account(&account("account-a", "Account A"))
        .await?;
    storage
        .commit_account_sync(&sync_commit(
            "account-a",
            "sync-a-1",
            vec![work(
                "RJ000001",
                "Owned Work",
                "Circle One",
                "2026-01-01T00:00:00Z",
            )],
            vec![account_work("RJ000001", "2026-02-01T00:00:00Z")],
        ))
        .await?;
    storage
        .import_local_work_downloads(&[LocalWorkDownloadImport {
            work: CachedWork {
                work_id: "RJ123456".to_owned(),
                title_json: r#"{"en_US":"[RJ123456] Local Folder"}"#.to_owned(),
                maker_id: None,
                maker_json: None,
                work_type: None,
                age_category: None,
                thumbnail_url: None,
                registered_at: None,
                published_at: None,
                updated_at: None,
                raw_json:
                    r#"{"workno":"RJ123456","source":"local_scan","detail_status":"local_only"}"#
                        .to_owned(),
                last_detail_sync_at: "2026-05-11T00:00:00.000Z".to_owned(),
            },
            download: WorkDownloadUpdate {
                work_id: "RJ123456".to_owned(),
                status: WorkDownloadStatus::Downloaded,
                local_path: Some("/library/[RJ123456] Local Folder".to_owned()),
                staging_path: None,
                unpack_policy: "manual".to_owned(),
                bytes_received: 0,
                bytes_total: None,
                error_code: None,
                error_message: None,
                started_at: Some("2026-05-11T00:00:00.000Z".to_owned()),
                completed_at: Some("2026-05-11T00:00:00.000Z".to_owned()),
                updated_at: "2026-05-11T00:00:00.000Z".to_owned(),
            },
        }])
        .await?;

    let local_page = storage
        .list_products(&ProductListQuery {
            source_groups: vec![ProductSourceGroup::LocalOnly],
            ..ProductListQuery::default()
        })
        .await?;
    let owned_page = storage
        .list_products(&ProductListQuery {
            source_groups: vec![ProductSourceGroup::Owned],
            ..ProductListQuery::default()
        })
        .await?;
    let searched_page = storage
        .list_products(&ProductListQuery {
            search: Some("not owned".to_owned()),
            ..ProductListQuery::default()
        })
        .await?;
    let account_local_page = storage
        .list_products(&ProductListQuery {
            account_id: Some("account-a".to_owned()),
            source_groups: vec![ProductSourceGroup::LocalOnly],
            ..ProductListQuery::default()
        })
        .await?;

    assert_eq!(local_page.total_count, 1);
    assert_eq!(local_page.products[0].work_id, "RJ123456");
    assert_eq!(
        local_page.products[0].owners[0].account_id,
        LOCAL_PRODUCT_OWNER_ID
    );
    assert_eq!(owned_page.total_count, 1);
    assert_eq!(owned_page.products[0].work_id, "RJ000001");
    assert_eq!(searched_page.total_count, 1);
    assert_eq!(searched_page.products[0].work_id, "RJ123456");
    assert_eq!(account_local_page.total_count, 0);

    Ok(())
}

#[tokio::test]
async fn local_download_import_preserves_existing_work_and_download_state() -> Result<()> {
    let storage = migrated_storage().await?;
    storage
        .save_account(&account("account-a", "Account A"))
        .await?;
    storage
        .commit_account_sync(&sync_commit(
            "account-a",
            "sync-a-1",
            vec![work(
                "RJ000001",
                "Synced Work",
                "Circle One",
                "2026-01-01T00:00:00Z",
            )],
            vec![account_work("RJ000001", "2026-02-01T00:00:00Z")],
        ))
        .await?;
    storage
        .save_work_download(&WorkDownloadUpdate {
            work_id: "RJ000001".to_owned(),
            status: WorkDownloadStatus::Downloaded,
            local_path: Some("/library/RJ000001".to_owned()),
            staging_path: None,
            unpack_policy: "unpack_when_recognized".to_owned(),
            bytes_received: 42,
            bytes_total: Some(42),
            error_code: None,
            error_message: None,
            started_at: Some("2026-05-11T00:00:00.000Z".to_owned()),
            completed_at: Some("2026-05-11T00:01:00.000Z".to_owned()),
            updated_at: "2026-05-11T00:01:00.000Z".to_owned(),
        })
        .await?;

    storage
        .import_local_work_downloads(&[LocalWorkDownloadImport {
            work: CachedWork {
                work_id: "RJ000001".to_owned(),
                title_json: r#"{"en_US":"[RJ000001] Local Folder"}"#.to_owned(),
                maker_id: None,
                maker_json: None,
                work_type: None,
                age_category: None,
                thumbnail_url: None,
                registered_at: None,
                published_at: None,
                updated_at: None,
                raw_json:
                    r#"{"workno":"RJ000001","source":"local_scan","detail_status":"local_only"}"#
                        .to_owned(),
                last_detail_sync_at: "2026-05-11T00:02:00.000Z".to_owned(),
            },
            download: WorkDownloadUpdate {
                work_id: "RJ000001".to_owned(),
                status: WorkDownloadStatus::Downloaded,
                local_path: Some("/library/[RJ000001] Local Folder".to_owned()),
                staging_path: None,
                unpack_policy: "manual".to_owned(),
                bytes_received: 0,
                bytes_total: None,
                error_code: None,
                error_message: None,
                started_at: Some("2026-05-11T00:02:00.000Z".to_owned()),
                completed_at: Some("2026-05-11T00:02:00.000Z".to_owned()),
                updated_at: "2026-05-11T00:02:00.000Z".to_owned(),
            },
        }])
        .await?;

    let page = storage.list_products(&ProductListQuery::default()).await?;

    assert_eq!(page.total_count, 1);
    assert_eq!(page.products[0].title, "Synced Work");
    assert_eq!(page.products[0].owners[0].label, "Account A");
    assert_eq!(
        page.products[0].download.local_path,
        Some("/library/RJ000001".to_owned())
    );
    assert_eq!(page.products[0].download.bytes_received, 42);
    assert_eq!(
        page.products[0].download.unpack_policy,
        Some("unpack_when_recognized".to_owned())
    );

    Ok(())
}

#[tokio::test]
async fn download_account_for_work_uses_enabled_current_owner() -> Result<()> {
    let storage = migrated_storage().await?;
    storage
        .save_account(&account("account-a", "Account A"))
        .await?;
    storage
        .save_account(&account("account-b", "Account B"))
        .await?;
    storage.set_account_enabled("account-a", false).await?;
    storage
        .commit_account_sync(&sync_commit(
            "account-a",
            "sync-a-1",
            vec![work(
                "RJ000001",
                "Owned Work",
                "Circle One",
                "2026-01-01T00:00:00Z",
            )],
            vec![account_work("RJ000001", "2026-02-01T00:00:00Z")],
        ))
        .await?;
    storage
        .commit_account_sync(&sync_commit(
            "account-b",
            "sync-b-1",
            vec![work(
                "RJ000001",
                "Owned Work",
                "Circle One",
                "2026-01-01T00:00:00Z",
            )],
            vec![account_work("RJ000001", "2026-02-02T00:00:00Z")],
        ))
        .await?;

    let account = storage.download_account_for_work("RJ000001", None).await?;

    assert_eq!(account.id, "account-b");
    assert!(matches!(
        storage
            .download_account_for_work("RJ000001", Some("account-a"))
            .await,
        Err(StorageError::NotFound { .. })
    ));

    Ok(())
}
