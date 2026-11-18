mod accounts;
mod products;
mod settings;
mod transaction;
use serde::Deserialize;
pub use transaction::WriteTransaction;
mod genres;
mod locale;
pub use genres::display_name as genre_display_name;
pub use genres::display_name_for_id as genre_display_name_for_id;
pub use locale::UiLocale;
mod names;
mod search;
use names::{preferred_creator_name, preferred_name, product_query, push_valid_name_map};
pub use names::{LanguagePriority, DEFAULT_LANGUAGE_PRIORITY};
mod finalization;
mod playlists;
mod queue;
pub use playlists::Playlist;
pub use queue::DownloadQueueEntry;
mod backup;
pub use backup::LibraryBackup;
pub use finalization::DownloadFinalization;
use sqlx::{
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteQueryResult},
    QueryBuilder, Row, Sqlite, SqlitePool, Transaction,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");
const LIBRARY_ROOT_KEY: &str = "library_root";
const DOWNLOAD_ROOT_KEY: &str = "download_root";
const LANGUAGE_PRIORITY_KEY: &str = "language_priority";
const DOWNLOAD_RATE_KEY: &str = "download_rate_limit_bytes_per_second";
const MISSING_WORK_DETAIL_STATUS: &str = "missing_from_content_works";
pub const LOCAL_PRODUCT_OWNER_ID: &str = "__local__";
pub const LOCAL_PRODUCT_OWNER_LABEL: &str = "Local";

pub type Result<T> = std::result::Result<T, StorageError>;

mod error;
pub use error::StorageError;
mod models;
pub use models::*;

#[derive(Clone)]
pub struct Storage {
    pool: SqlitePool,
}

impl Storage {
    /// Open an existing library without migrations or permission to change its data.
    pub async fn open_read_only(path: impl AsRef<Path>) -> Result<Self> {
        Self::connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .read_only(true)
                .foreign_keys(true),
        )
        .await
    }

    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(true);

        Self::connect_with(options).await
    }

    pub async fn open_in_memory() -> Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(":memory:")
            .foreign_keys(true);

        Self::connect_with(options).await
    }

    pub async fn connect_with(options: SqliteConnectOptions) -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;

        Ok(Self { pool })
    }

    pub async fn run_migrations(&self) -> Result<()> {
        MIGRATOR.run(&self.pool).await?;
        Ok(())
    }

    pub async fn begin_write(&self) -> Result<WriteTransaction<'_>> {
        let transaction = self.pool.begin().await?;
        Ok(WriteTransaction {
            transaction: Some(transaction),
        })
    }

    pub async fn download_account_for_work(
        &self,
        work_id: &str,
        account_id: Option<&str>,
    ) -> Result<Account> {
        let mut builder = QueryBuilder::<Sqlite>::new(
            "SELECT a.id, a.label, a.login_name, a.credential_ref, a.enabled,
                    a.created_at, a.updated_at, a.last_login_at, a.last_sync_at
             FROM account_works aw
             JOIN accounts a ON a.id = aw.account_id
             WHERE aw.work_id = ",
        );

        builder.push_bind(work_id.to_owned());
        builder.push(" AND aw.is_current = 1 AND a.enabled = 1");

        if let Some(account_id) = account_id {
            builder.push(" AND aw.account_id = ");
            builder.push_bind(account_id.to_owned());
        }

        builder.push(" ORDER BY lower(a.label) ASC, a.id ASC LIMIT 1");

        builder
            .build()
            .fetch_optional(&self.pool)
            .await?
            .map(account_from_row)
            .transpose()?
            .ok_or_else(|| StorageError::NotFound {
                entity: "download account for work",
                id: match account_id {
                    Some(account_id) => format!("{work_id}/{account_id}"),
                    None => work_id.to_owned(),
                },
            })
    }

    pub async fn work_download_state(&self, work_id: &str) -> Result<WorkDownloadState> {
        let row = sqlx::query(
            "SELECT status, local_path, staging_path, unpack_policy, bytes_received,
                    bytes_total, error_code, error_message, started_at, completed_at,
                    updated_at
             FROM work_downloads
             WHERE work_id = ?1",
        )
        .bind(work_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(work_download_state_from_row)
            .transpose()
            .map(|state| state.unwrap_or_default())
    }

    pub async fn save_work_download(&self, download: &WorkDownloadUpdate) -> Result<()> {
        let mut transaction = self.begin_write().await?;

        transaction.save_work_download(download).await?;
        transaction.commit().await
    }

    pub async fn delete_work_download(&self, work_id: &str) -> Result<()> {
        let mut transaction = self.begin_write().await?;

        transaction.delete_work_download(work_id).await?;
        transaction.commit().await
    }

    pub async fn import_local_work_downloads(
        &self,
        imports: &[LocalWorkDownloadImport],
    ) -> Result<()> {
        self.import_local_work_downloads_with_metadata(imports, &[])
            .await
    }

    pub async fn import_local_work_downloads_with_metadata(
        &self,
        imports: &[LocalWorkDownloadImport],
        metadata_works: &[CachedWork],
    ) -> Result<()> {
        let mut transaction = self.begin_write().await?;

        for import in imports {
            transaction.insert_work_if_missing(&import.work).await?;
            transaction
                .insert_work_download_if_missing(&import.download)
                .await?;
        }

        for work in metadata_works {
            transaction.upsert_work(work).await?;
        }

        transaction.commit().await
    }

    pub async fn work_needs_local_metadata_hydration(&self, work_id: &str) -> Result<bool> {
        let row = sqlx::query(
            "SELECT title_json, maker_json, raw_json
             FROM works
             WHERE work_id = ?1",
        )
        .bind(work_id)
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            return Ok(true);
        };

        let title_json: String = row.try_get("title_json")?;
        let maker_json: Option<String> = row.try_get("maker_json")?;
        let title = preferred_name(Some(&title_json)).unwrap_or_else(|| work_id.to_owned());
        let maker_name = preferred_name(maker_json.as_deref());
        let raw_json: String = row.try_get("raw_json")?;

        Ok(cached_work_needs_local_metadata_hydration(
            work_id,
            &title,
            maker_name.as_deref(),
            &raw_json,
        ))
    }

    pub async fn product_credits_checked_at(&self, work_id: &str) -> Result<Option<String>> {
        Ok(sqlx::query_scalar::<_, Option<String>>(
            "SELECT CASE WHEN json_valid(raw_json)
                THEN CASE WHEN json_extract(raw_json, '$.public_credits_version') = 2
                    THEN json_extract(raw_json, '$.public_credits_checked_at') END
                END FROM works WHERE work_id = ?1",
        )
        .bind(work_id)
        .fetch_optional(&self.pool)
        .await?
        .flatten())
    }

    /// Supplement credits only; never replace purchase metadata or download state.
    pub async fn cache_public_product_credits(
        &self,
        work_id: &str,
        tags: &[serde_json::Value],
        checked_at: &str,
    ) -> Result<()> {
        let result = sqlx::query(
            "UPDATE works SET raw_json = json_set(raw_json,
                '$.public_credit_tags', json(?2), '$.public_credits_checked_at', ?3, '$.public_credits_version', 2)
             WHERE work_id = ?1 AND json_valid(raw_json)",
        )
        .bind(work_id)
        .bind(serde_json::to_string(tags).expect("JSON tags serialize"))
        .bind(checked_at)
        .execute(&self.pool)
        .await?;
        ensure_changed(result, "product metadata", work_id)
    }
}

fn account_from_row(row: sqlx::sqlite::SqliteRow) -> Result<Account> {
    Ok(Account {
        id: row.try_get("id")?,
        label: row.try_get("label")?,
        login_name: row.try_get("login_name")?,
        credential_ref: row.try_get("credential_ref")?,
        enabled: i64_to_bool(row.try_get("enabled")?, "accounts.enabled")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        last_login_at: row.try_get("last_login_at")?,
        last_sync_at: row.try_get("last_sync_at")?,
    })
}

fn sync_run_from_row(row: sqlx::sqlite::SqliteRow) -> Result<SyncRun> {
    let status: String = row.try_get("status")?;

    Ok(SyncRun {
        id: row.try_get("id")?,
        account_id: row.try_get("account_id")?,
        status: SyncRunStatus::from_str(&status)?,
        started_at: row.try_get("started_at")?,
        completed_at: row.try_get("completed_at")?,
        error_code: row.try_get("error_code")?,
        error_message: row.try_get("error_message")?,
    })
}

fn work_download_state_from_row(row: sqlx::sqlite::SqliteRow) -> Result<WorkDownloadState> {
    let status: String = row.try_get("status")?;

    Ok(WorkDownloadState {
        status: WorkDownloadStatus::from_storage_value(&status)?,
        local_path: row.try_get("local_path")?,
        staging_path: row.try_get("staging_path")?,
        unpack_policy: row.try_get("unpack_policy")?,
        bytes_received: i64_to_u64(
            row.try_get("bytes_received")?,
            "work_downloads.bytes_received",
        )?,
        bytes_total: row
            .try_get::<Option<i64>, _>("bytes_total")?
            .map(|value| i64_to_u64(value, "work_downloads.bytes_total"))
            .transpose()?,
        error_code: row.try_get("error_code")?,
        error_message: row.try_get("error_message")?,
        started_at: row.try_get("started_at")?,
        completed_at: row.try_get("completed_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn work_download_state_from_product_row(
    row: &sqlx::sqlite::SqliteRow,
) -> Result<WorkDownloadState> {
    let Some(status) = row.try_get::<Option<String>, _>("download_status")? else {
        return Ok(WorkDownloadState::default());
    };

    Ok(WorkDownloadState {
        status: WorkDownloadStatus::from_storage_value(&status)?,
        local_path: row.try_get("download_local_path")?,
        staging_path: row.try_get("download_staging_path")?,
        unpack_policy: row.try_get("download_unpack_policy")?,
        bytes_received: i64_to_u64(
            row.try_get("download_bytes_received")?,
            "work_downloads.bytes_received",
        )?,
        bytes_total: row
            .try_get::<Option<i64>, _>("download_bytes_total")?
            .map(|value| i64_to_u64(value, "work_downloads.bytes_total"))
            .transpose()?,
        error_code: row.try_get("download_error_code")?,
        error_message: row.try_get("download_error_message")?,
        started_at: row.try_get("download_started_at")?,
        completed_at: row.try_get("download_completed_at")?,
        updated_at: row.try_get("download_updated_at")?,
    })
}

fn push_product_visibility_filter(builder: &mut QueryBuilder<Sqlite>, query: &ProductListQuery) {
    let account_ids = product_account_ids(query);

    builder.push(" AND (");
    push_product_owned_condition(builder, &account_ids);

    if account_ids.is_empty() {
        builder.push(" OR ");
        push_product_has_download_condition(builder);
    }

    builder.push(")");
}

fn push_product_filters(
    builder: &mut QueryBuilder<Sqlite>,
    query: &ProductListQuery,
) -> Result<()> {
    push_product_filters_internal(builder, query, true)
}

fn push_product_filters_internal(
    builder: &mut QueryBuilder<Sqlite>,
    query: &ProductListQuery,
    include_maker_filter: bool,
) -> Result<()> {
    if let Some(id) = &query.playlist_id {
        builder.push(" AND EXISTS(SELECT 1 FROM playlist_works pw WHERE pw.work_id=w.work_id AND pw.playlist_id=");
        builder.push_bind(id);
        builder.push(")");
    }
    if query.downloaded_only {
        builder.push(
            " AND EXISTS (
                SELECT 1 FROM work_downloads downloaded_wd
                WHERE downloaded_wd.work_id = w.work_id
                    AND downloaded_wd.status = 'downloaded'
                    AND NULLIF(trim(downloaded_wd.local_path), '') IS NOT NULL
            )",
        );
    }

    builder.push(
        " AND COALESCE(
            CASE
                WHEN json_valid(w.raw_json) THEN json_extract(w.raw_json, '$.detail_status')
            END,
            ''
        ) <> ",
    );
    builder.push_bind(MISSING_WORK_DETAIL_STATUS);

    let age_categories = product_age_categories(query);
    if !age_categories.is_empty() {
        builder.push(" AND w.age_category IN (");
        for (index, age_category) in age_categories.iter().enumerate() {
            if index > 0 {
                builder.push(", ");
            }
            builder.push_bind(age_category.as_storage_value());
        }
        builder.push(")");
    }

    let type_groups = product_type_groups(query);
    if !type_groups.is_empty() {
        builder.push(" AND ");
        builder.push(product_type_group_case_sql());
        builder.push(" IN (");
        for (index, type_group) in type_groups.iter().enumerate() {
            if index > 0 {
                builder.push(", ");
            }
            builder.push_bind(type_group.as_storage_value());
        }
        builder.push(")");
    }

    let source_groups = product_source_groups(query);
    if !source_groups.is_empty() && source_groups.len() < 2 {
        builder.push(" AND (");
        for (index, source_group) in source_groups.iter().enumerate() {
            if index > 0 {
                builder.push(" OR ");
            }

            match source_group {
                ProductSourceGroup::Owned => push_product_owned_condition(builder, &[]),
                ProductSourceGroup::LocalOnly => push_product_local_only_condition(builder),
            }
        }
        builder.push(")");
    }

    let maker_keys = product_maker_keys(query);
    if include_maker_filter && !maker_keys.is_empty() {
        builder.push(" AND w.maker_key IN (");
        for (index, maker_key) in maker_keys.iter().enumerate() {
            if index > 0 {
                builder.push(", ");
            }
            builder.push_bind((*maker_key).to_owned());
        }
        builder.push(")");
    }

    for name in &query.genre_names {
        builder.push(" AND (");
        genres::push_exact_filter(builder, name);
        builder.push(")");
    }
    for name in &query.excluded_genre_names {
        builder.push(" AND NOT (");
        genres::push_exact_filter(builder, name);
        builder.push(")");
    }
    if !query.voice_names.is_empty() {
        builder.push(" AND (");
        for (i, name) in query.voice_names.iter().enumerate() {
            if i > 0 {
                builder.push(" OR ");
            }
            push_voice_filter(builder, name);
        }
        builder.push(")");
    }
    for name in &query.excluded_voice_names {
        builder.push(" AND NOT (");
        push_voice_filter(builder, name);
        builder.push(")");
    }

    search::push_filters(builder, query.search.as_deref().unwrap_or_default())
}

fn push_text_filter(builder: &mut QueryBuilder<Sqlite>, search: &str) {
    let pattern = format!("%{}%", escape_like(search));
    builder.push(
        " AND (
                w.work_id LIKE ",
    );
    builder.push_bind(pattern.clone());
    builder.push(" ESCAPE '\\'");
    for column in ["w.title_json", "w.maker_json"] {
        builder.push(" OR EXISTS (SELECT 1 FROM json_each(");
        push_valid_name_map(builder, column);
        builder.push(
            ") AS name WHERE name.type = 'text' AND trim(name.value) <> '' AND name.value LIKE ",
        );
        builder.push_bind(pattern.clone());
        builder.push(" ESCAPE '\\')");
    }
    builder.push(
        " OR EXISTS (
                    SELECT 1
                    FROM (SELECT value FROM json_each(
                        CASE
                            WHEN json_valid(w.raw_json) THEN w.raw_json
                            ELSE '{\"tags\":[]}'
                        END,
                        '$.tags'
                    ) UNION ALL SELECT value FROM json_each(
                        CASE WHEN json_valid(w.raw_json) THEN w.raw_json ELSE '{}' END,
                        '$.public_credit_tags'
                    )) AS tag
                    WHERE (json_type(tag.value, '$.name') = 'text' AND json_extract(tag.value, '$.name') LIKE ",
    );
    builder.push_bind(pattern.clone());
    builder.push(" ESCAPE '\\') OR EXISTS (SELECT 1 FROM json_each(CASE WHEN json_type(tag.value, '$.name') = 'object' THEN json_extract(tag.value, '$.name') ELSE '{}' END) AS credit_name WHERE credit_name.type = 'text' AND credit_name.value LIKE ");
    builder.push_bind(pattern.clone());
    builder.push(" ESCAPE '\\'))");

    genres::push_search(builder, search);

    if search_matches_local_only_source(search) {
        builder.push(" OR ");
        push_product_local_only_condition(builder);
    }

    builder.push(")");
}

fn push_product_owned_condition(builder: &mut QueryBuilder<Sqlite>, account_ids: &[&str]) {
    builder.push(
        "EXISTS (
            SELECT 1
            FROM account_works source_aw
            JOIN accounts source_a ON source_a.id = source_aw.account_id
            WHERE source_aw.work_id = w.work_id
                AND source_aw.is_current = 1
                AND source_a.enabled = 1",
    );

    if !account_ids.is_empty() {
        builder.push(" AND source_aw.account_id IN (");
        for (index, account_id) in account_ids.iter().enumerate() {
            if index > 0 {
                builder.push(", ");
            }
            builder.push_bind((*account_id).to_owned());
        }
        builder.push(")");
    }

    builder.push(")");
}

fn push_voice_filter(builder: &mut QueryBuilder<Sqlite>, name: &str) {
    builder.push("EXISTS (SELECT 1 FROM (SELECT value FROM json_each(CASE WHEN json_valid(w.raw_json) THEN w.raw_json ELSE '{}' END,'$.tags') UNION ALL SELECT value FROM json_each(CASE WHEN json_valid(w.raw_json) THEN w.raw_json ELSE '{}' END,'$.public_credit_tags')) AS credit WHERE json_extract(credit.value,'$.class')='voice_by' AND ((json_type(credit.value,'$.name')='text' AND trim(json_extract(credit.value,'$.name')) = ");
    builder.push_bind(name.to_owned());
    builder.push(" COLLATE NOCASE) OR EXISTS (SELECT 1 FROM json_each(CASE WHEN json_type(credit.value,'$.name')='object' THEN json_extract(credit.value,'$.name') ELSE '{}' END) AS alias WHERE alias.type='text' AND trim(alias.value) = ");
    builder.push_bind(name.to_owned());
    builder.push(" COLLATE NOCASE)))");
}

fn push_product_has_download_condition(builder: &mut QueryBuilder<Sqlite>) {
    builder.push(
        "EXISTS (
            SELECT 1
            FROM work_downloads source_wd
            WHERE source_wd.work_id = w.work_id
                AND (source_wd.status = 'downloaded' OR (source_wd.status = 'failed' AND source_wd.error_code = 'local_missing'))
        )",
    );
}

fn push_product_local_only_condition(builder: &mut QueryBuilder<Sqlite>) {
    builder.push("(NOT ");
    push_product_owned_condition(builder, &[]);
    builder.push(" AND ");
    push_product_has_download_condition(builder);
    builder.push(")");
}

fn product_account_ids(query: &ProductListQuery) -> Vec<&str> {
    let mut ids = Vec::new();

    if let Some(account_id) = query.account_id.as_deref() {
        push_unique_str(&mut ids, account_id);
    }

    for account_id in &query.account_ids {
        push_unique_str(&mut ids, account_id);
    }

    ids
}

fn product_age_categories(query: &ProductListQuery) -> Vec<ProductAgeCategory> {
    let mut values = Vec::new();

    if let Some(age_category) = query.age_category {
        push_unique_copy(&mut values, age_category);
    }

    for age_category in &query.age_categories {
        push_unique_copy(&mut values, *age_category);
    }

    values
}

fn product_type_groups(query: &ProductListQuery) -> Vec<ProductTypeGroup> {
    let mut values = Vec::new();

    if let Some(type_group) = query.type_group {
        push_unique_copy(&mut values, type_group);
    }

    for type_group in &query.type_groups {
        push_unique_copy(&mut values, *type_group);
    }

    values
}

fn product_source_groups(query: &ProductListQuery) -> Vec<ProductSourceGroup> {
    let mut values = Vec::new();

    for source_group in &query.source_groups {
        push_unique_copy(&mut values, *source_group);
    }

    values
}

fn product_maker_keys(query: &ProductListQuery) -> Vec<&str> {
    let mut names = Vec::new();

    for maker_name in &query.maker_keys {
        let maker_name = maker_name.trim();
        if !maker_name.is_empty() {
            push_unique_str(&mut names, maker_name);
        }
    }

    names
}

fn search_matches_local_only_source(search: &str) -> bool {
    let normalized = search.trim().to_lowercase().replace(['_', '-'], " ");

    normalized.contains("local")
        || normalized.contains("not owned")
        || normalized.contains("notowned")
        || normalized.contains("unowned")
}

fn push_unique_str<'a>(values: &mut Vec<&'a str>, value: &'a str) {
    if !values.contains(&value) {
        values.push(value);
    }
}

fn push_unique_copy<T>(values: &mut Vec<T>, value: T)
where
    T: Copy + PartialEq,
{
    if !values.contains(&value) {
        values.push(value);
    }
}

fn local_product_owner() -> ProductOwner {
    ProductOwner {
        account_id: LOCAL_PRODUCT_OWNER_ID.to_owned(),
        label: LOCAL_PRODUCT_OWNER_LABEL.to_owned(),
        purchased_at: None,
    }
}

fn cached_work_needs_local_metadata_hydration(
    work_id: &str,
    title: &str,
    maker_name: Option<&str>,
    raw_json: &str,
) -> bool {
    let parsed = serde_json::from_str::<serde_json::Value>(raw_json).ok();
    let detail_status = parsed
        .as_ref()
        .and_then(|value| value.get("detail_status"))
        .and_then(serde_json::Value::as_str);
    let source = parsed
        .as_ref()
        .and_then(|value| value.get("source"))
        .and_then(serde_json::Value::as_str);

    matches!(
        detail_status,
        Some("local_only") | Some(MISSING_WORK_DETAIL_STATUS)
    ) || source == Some("local_scan")
        || (maker_name.is_none() && title.trim().eq_ignore_ascii_case(work_id))
}

fn product_type_group_case_sql() -> &'static str {
    "CASE
        WHEN lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%vcm%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%voicecomic%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%vcomic%'
            THEN 'image'
        WHEN lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%sou%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%amt%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%mus%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%audio%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%voice%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%asmr%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%music%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%sound%'
            THEN 'audio'
        WHEN lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%mov%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%movie%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%video%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%anime%'
            THEN 'video'
        WHEN lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%gam%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%acn%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%adv%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%etc%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%pzl%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%qiz%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%sln%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%tbl%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%typ%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%game%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%rpg%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%adv%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%action%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%acn%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%puzzle%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%puz%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%quiz%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%simulation%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%slg%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%shooter%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%stg%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%tabletop%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%typing%'
            THEN 'game'
        WHEN lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%cg%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%adl%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%doh%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%dnv%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%icg%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%imt%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%mng%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%scm%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%nre%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%ksv%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%icg%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%image%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%illust%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%comic%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%com%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%manga%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%mng%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%gekiga%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%pdf%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%novel%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%digitalnovel%'
            OR lower(replace(replace(replace(coalesce(w.work_type, ''), '_', ''), '-', ''), ' ', '')) LIKE '%book%'
            THEN 'image'
        ELSE 'other'
    END"
}

fn push_product_sort(builder: &mut QueryBuilder<Sqlite>, sort: ProductSort) {
    match sort {
        ProductSort::TitleAsc => {
            builder.push("sort_title ASC, w.work_id ASC");
        }
        ProductSort::LatestPurchaseDesc => {
            builder.push("latest_purchased_at DESC, sort_title ASC, w.work_id ASC");
        }
        ProductSort::PublishedAtDesc => {
            builder.push("sort_published_at DESC, sort_title ASC, w.work_id ASC");
        }
    }
}

fn push_outer_product_sort(builder: &mut QueryBuilder<Sqlite>, sort: ProductSort) {
    match sort {
        ProductSort::TitleAsc => {
            builder.push("vw.sort_title ASC, w.work_id ASC");
        }
        ProductSort::LatestPurchaseDesc => {
            builder.push("vw.latest_purchased_at DESC, vw.sort_title ASC, w.work_id ASC");
        }
        ProductSort::PublishedAtDesc => {
            builder.push("vw.sort_published_at DESC, vw.sort_title ASC, w.work_id ASC");
        }
    }
}

fn escape_like(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());

    for character in value.chars() {
        match character {
            '\\' | '%' | '_' => {
                escaped.push('\\');
                escaped.push(character);
            }
            _ => escaped.push(character),
        }
    }

    escaped
}

fn product_credit_groups_from_raw_json(raw_json: &str) -> Vec<ProductCreditGroup> {
    let Ok(work) = serde_json::from_str::<RawWorkCredits>(raw_json) else {
        return Vec::new();
    };

    let mut groups = credit_group_templates();

    let mut tags = work
        .tags
        .into_iter()
        .chain(work.public_credit_tags)
        .filter_map(|tag| Some((tag.class, preferred_creator_name(&tag.name)?)))
        .collect::<Vec<_>>();
    tags.sort_by(|(left_class, left_name), (right_class, right_name)| {
        credit_sort_key(left_class, left_name).cmp(&credit_sort_key(right_class, right_name))
    });

    for (class, name) in tags {
        let Some((kind, _label, _rank)) = credit_kind_label_and_rank(&class) else {
            continue;
        };
        let name = name.trim();

        if name.is_empty() {
            continue;
        }

        if let Some((_kind, _label, names)) = groups
            .iter_mut()
            .find(|(group_kind, _, _)| *group_kind == kind)
        {
            if !names.iter().any(|existing| existing.as_str() == name) {
                names.push(name.to_owned());
            }
        }
    }

    groups
        .into_iter()
        .filter_map(|(kind, label, names)| {
            if names.is_empty() {
                None
            } else {
                Some(ProductCreditGroup {
                    kind: kind.to_owned(),
                    label: label.to_owned(),
                    names,
                })
            }
        })
        .collect()
}

fn product_content_size_from_raw_json(raw_json: &str) -> Option<u64> {
    let value = serde_json::from_str::<serde_json::Value>(raw_json).ok()?;
    value
        .get("content_size")
        .or_else(|| value.get("contents_file_size"))
        .and_then(json_value_as_u64)
}

fn product_text_values_from_json(raw_json: &str) -> Vec<ProductTextValue> {
    let Ok(values) = serde_json::from_str::<BTreeMap<String, serde_json::Value>>(raw_json) else {
        return Vec::new();
    };

    values
        .into_iter()
        .filter_map(|(language, value)| {
            let value = value.as_str()?.trim();

            if value.is_empty() {
                None
            } else {
                Some(ProductTextValue {
                    language,
                    value: value.to_owned(),
                })
            }
        })
        .collect()
}

fn product_tags_from_raw_json(raw_json: &str) -> Vec<ProductTag> {
    let Ok(work) = serde_json::from_str::<RawWorkTags>(raw_json) else {
        return Vec::new();
    };

    let genre_ids = serde_json::from_str(raw_json)
        .map(|value| genres::ids_from_json(&value))
        .unwrap_or_default();
    let mut tags = work
        .tags
        .into_iter()
        .chain(work.public_credit_tags)
        .filter_map(|tag| {
            let class = tag.class.trim();
            let name = preferred_creator_name(&tag.name)?;
            let name = name.as_str();

            if class.is_empty() || name.is_empty() || (class == "genre" && !genre_ids.is_empty()) {
                None
            } else {
                Some(ProductTag {
                    class: class.to_owned(),
                    name: if class == "genre" {
                        genres::localized_name(name)?
                    } else {
                        name.to_owned()
                    },
                    genre_id: (class == "genre")
                        .then(|| genres::id_for_name(name))
                        .flatten(),
                })
            }
        })
        .collect::<Vec<_>>();

    tags.extend(genre_ids.into_iter().map(|id| ProductTag {
        class: "genre".into(),
        name: genres::name_for_id(id),
        genre_id: Some(id),
    }));

    tags.sort_by(|left, right| {
        left.class
            .cmp(&right.class)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then(left.genre_id.cmp(&right.genre_id))
    });
    tags.dedup();
    tags
}

fn json_value_as_u64(value: &serde_json::Value) -> Option<u64> {
    match value {
        serde_json::Value::Number(number) => number.as_u64(),
        serde_json::Value::String(value) => value.parse::<u64>().ok(),
        _ => None,
    }
}

fn credit_group_templates() -> Vec<(&'static str, &'static str, Vec<String>)> {
    vec![
        ("voice", "CV", Vec::new()),
        ("illust", "Illust", Vec::new()),
        ("scenario", "Scenario", Vec::new()),
        ("creator", "Creator", Vec::new()),
        ("series", "Series", Vec::new()),
        ("music", "Music", Vec::new()),
        ("other", "Other", Vec::new()),
    ]
}

fn credit_sort_key(class: &str, name: &str) -> (u8, String) {
    let rank = credit_kind_label_and_rank(class)
        .map(|(_kind, _label, rank)| rank)
        .unwrap_or(u8::MAX);

    (rank, name.trim().to_lowercase())
}

fn credit_kind_label_and_rank(class: &str) -> Option<(&'static str, &'static str, u8)> {
    match class {
        "voice_by" => Some(("voice", "CV", 0)),
        "illust_by" => Some(("illust", "Illust", 1)),
        "scenario_by" => Some(("scenario", "Scenario", 2)),
        "created_by" => Some(("creator", "Creator", 3)),
        "music_by" => Some(("music", "Music", 4)),
        "other_by" => Some(("other", "Other", 5)),
        "series" => Some(("series", "Series", 7)),
        _ if class.ends_with("_by") => Some(("other", "Other", 6)),
        _ => None,
    }
}

#[derive(Debug, Deserialize)]
struct RawWorkCredits {
    #[serde(default)]
    tags: Vec<RawWorkCreditTag>,
    #[serde(default)]
    public_credit_tags: Vec<RawWorkCreditTag>,
}

#[derive(Debug, Deserialize)]
struct RawWorkTags {
    #[serde(default)]
    tags: Vec<RawWorkCreditTag>,
    #[serde(default)]
    public_credit_tags: Vec<RawWorkCreditTag>,
}

#[derive(Debug, Deserialize)]
struct RawWorkCreditTag {
    #[serde(rename = "class")]
    class: String,
    #[serde(rename = "name")]
    name: serde_json::Value,
}

fn bool_to_i64(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}

fn i64_to_bool(value: i64, field: &'static str) -> Result<bool> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(StorageError::InvalidStoredValue {
            field,
            value: value.to_string(),
        }),
    }
}

fn u64_to_i64(value: u64, field: &'static str) -> Result<i64> {
    i64::try_from(value).map_err(|_| StorageError::InvalidStoredValue {
        field,
        value: value.to_string(),
    })
}

fn i64_to_u64(value: i64, field: &'static str) -> Result<u64> {
    u64::try_from(value).map_err(|_| StorageError::InvalidStoredValue {
        field,
        value: value.to_string(),
    })
}

fn ensure_changed(result: SqliteQueryResult, entity: &'static str, id: &str) -> Result<()> {
    if result.rows_affected() == 0 {
        Err(StorageError::NotFound {
            entity,
            id: id.to_owned(),
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
