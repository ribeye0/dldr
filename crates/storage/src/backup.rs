use super::*;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

const TABLES: &[&str] = &[
    "app_settings",
    "accounts",
    "works",
    "account_works",
    "work_custom_tags",
    "work_downloads",
    "download_queue",
    "playlists",
    "playlist_works",
];
const SETTINGS: &[&str] = &[
    "auto_extract",
    "library_root",
    "download_root",
    "language_priority",
    TITLE_LANGUAGE_FOLLOWS_UI_KEY,
    "ui_locale",
    "download_rate_limit_bytes_per_second",
    "download_concurrency",
];

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LibraryBackup {
    pub format: String,
    pub version: u32,
    pub tables: BTreeMap<String, Vec<Map<String, Value>>>,
}

fn invalid(message: impl Into<String>) -> StorageError {
    StorageError::InvalidStoredValue {
        field: "library_backup",
        value: message.into(),
    }
}

impl Storage {
    pub async fn export_library_backup(&self) -> Result<LibraryBackup> {
        let mut tx = self.pool.begin().await?;
        let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM download_finalizations")
            .fetch_one(&mut *tx)
            .await?;
        if pending != 0 {
            return Err(invalid(
                "Resolve incomplete download installations before backup",
            ));
        }
        let mut tables = BTreeMap::new();
        for table in TABLES {
            let columns = QueryBuilder::<Sqlite>::new(format!("PRAGMA table_info({table})"))
                .build()
                .fetch_all(&mut *tx)
                .await?
                .into_iter()
                .map(|r| r.get::<String, _>("name"))
                .collect::<Vec<_>>();
            let pairs = columns
                .iter()
                .map(|c| format!("'{c}',\"{c}\""))
                .collect::<Vec<_>>()
                .join(",");
            let values: Vec<String> =
                QueryBuilder::<Sqlite>::new(format!("SELECT json_object({pairs}) FROM {table}"))
                    .build_query_scalar()
                    .fetch_all(&mut *tx)
                    .await?;
            let mut rows = Vec::new();
            for value in values {
                let mut row: Map<String, Value> =
                    serde_json::from_str(&value).map_err(|_| invalid("Invalid row JSON"))?;
                if *table == "accounts" {
                    row.insert("credential_ref".into(), Value::Null);
                }
                if *table == "account_works" {
                    row.insert("last_seen_sync_run_id".into(), Value::Null);
                }
                if *table == "download_queue"
                    && row.get("status").and_then(Value::as_str) != Some("completed")
                {
                    row.insert("status".into(), Value::String("paused".into()));
                    row.insert("error_message".into(), Value::Null);
                }
                if *table == "app_settings"
                    && !row
                        .get("key")
                        .and_then(Value::as_str)
                        .is_some_and(|k| SETTINGS.contains(&k))
                {
                    continue;
                }
                rows.push(row);
            }
            tables.insert(table.to_string(), rows);
        }
        tx.commit().await?;
        Ok(LibraryBackup {
            format: "dldr-library".into(),
            version: 2,
            tables,
        })
    }

    /// Transactional merge. Credentials are neither exported nor replaced.
    pub async fn restore_library_backup(&self, backup: &LibraryBackup) -> Result<usize> {
        if backup.format != "dldr-library"
            || !matches!(backup.version, 1 | 2)
            || backup.tables.len()
                != if backup.version == 1 {
                    TABLES.len() - 2
                } else {
                    TABLES.len()
                }
            || backup.tables.keys().any(|t| !TABLES.contains(&t.as_str()))
        {
            return Err(invalid("Unsupported backup format/version/tables. Export a new DLDR backup from the current library."));
        }
        let mut tx = self.pool.begin().await?;
        let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM download_finalizations")
            .fetch_one(&mut *tx)
            .await?;
        if pending != 0 {
            return Err(invalid(
                "Resolve incomplete download installations before restore",
            ));
        }
        // Missing optional settings in the snapshot mean defaults/unlimited.
        // Clear only our allowlisted keys; the whole operation remains transactional.
        for key in SETTINGS {
            sqlx::query("DELETE FROM app_settings WHERE key=?1")
                .bind(key)
                .execute(&mut *tx)
                .await?;
        }
        let mut count = 0;
        for table in TABLES {
            let Some(inputs) = backup.tables.get(*table) else {
                if backup.version == 1 && matches!(*table, "playlists" | "playlist_works") {
                    continue;
                }
                return Err(invalid("Missing backup table"));
            };
            let schema = QueryBuilder::<Sqlite>::new(format!("PRAGMA table_info({table})"))
                .build()
                .fetch_all(&mut *tx)
                .await?;
            let columns = schema
                .iter()
                .map(|r| r.get::<String, _>("name"))
                .collect::<Vec<_>>();
            let keys = schema
                .iter()
                .filter(|r| r.get::<i64, _>("pk") > 0)
                .map(|r| r.get::<String, _>("name"))
                .collect::<Vec<_>>();
            for input in inputs {
                if input.len() != columns.len() || input.keys().any(|key| !columns.contains(key)) {
                    return Err(invalid("Backup columns do not match this version"));
                }
                let mut row = input.clone();
                if *table == "playlists" {
                    let name = row
                        .get("name")
                        .and_then(Value::as_str)
                        .ok_or_else(|| invalid("Invalid playlist name"))?;
                    if !(row.get("id").and_then(Value::as_str) == Some("0") && name.is_empty()) {
                        super::playlists::playlist_name(name)?;
                    }
                }
                if *table == "accounts" {
                    if let Some(id) = row.get("id").and_then(Value::as_str) {
                        let existing = sqlx::query(
                            "SELECT login_name,credential_ref FROM accounts WHERE id=?1",
                        )
                        .bind(id)
                        .fetch_optional(&mut *tx)
                        .await?;
                        if let Some(existing) = existing {
                            let login: Option<String> = existing.try_get("login_name")?;
                            let credential: Option<String> = existing.try_get("credential_ref")?;
                            if credential.is_some()
                                && login.as_deref() != row.get("login_name").and_then(Value::as_str)
                            {
                                return Err(invalid("Backup account sign-in identity differs from the local saved account; resolve that account first"));
                            }
                        }
                    }
                    row.insert("credential_ref".into(), Value::Null);
                }
                if *table == "account_works" {
                    row.insert("last_seen_sync_run_id".into(), Value::Null);
                }
                if *table == "download_queue" {
                    row.insert("status".into(), Value::String("paused".into()));
                    row.insert("error_message".into(), Value::Null);
                }
                if *table == "app_settings" {
                    let key = row
                        .get("key")
                        .and_then(Value::as_str)
                        .ok_or_else(|| invalid("Invalid setting key"))?;
                    let value = row
                        .get("value")
                        .and_then(Value::as_str)
                        .ok_or_else(|| invalid("Invalid setting value"))?;
                    if !SETTINGS.contains(&key) {
                        return Err(invalid("Unknown backup setting"));
                    }
                    match key {
                        "auto_extract" | TITLE_LANGUAGE_FOLLOWS_UI_KEY => {
                            value
                                .parse::<bool>()
                                .map_err(|_| invalid("Invalid archive preference"))?;
                        }
                        "ui_locale" => {
                            serde_json::from_str::<UiLocale>(value)
                                .map_err(|_| invalid("Invalid interface language"))?;
                        }
                        "language_priority" => {
                            LanguagePriority::from_stored(value)?;
                        }
                        "download_concurrency" => {
                            if !value
                                .parse::<usize>()
                                .ok()
                                .is_some_and(|v| (1..=4).contains(&v))
                            {
                                return Err(invalid("Invalid concurrency"));
                            }
                        }
                        "download_rate_limit_bytes_per_second"
                            if !value
                                .parse::<u64>()
                                .ok()
                                .is_some_and(|v| (1024..=1024 * 1024 * 1024).contains(&v)) =>
                        {
                            return Err(invalid("Invalid download rate"));
                        }
                        _ => {}
                    }
                }
                let updates = columns
                    .iter()
                    .filter(|c| {
                        !keys.contains(c)
                            && !(*table == "accounts" && c.as_str() == "credential_ref")
                    })
                    .map(|c| format!("\"{c}\"=excluded.\"{c}\""))
                    .collect::<Vec<_>>()
                    .join(",");
                let mut query = QueryBuilder::<Sqlite>::new(format!(
                    "INSERT INTO {table} ({}) VALUES (",
                    columns.join(",")
                ));
                for (index, column) in columns.iter().enumerate() {
                    if index != 0 {
                        query.push(",");
                    }
                    let value = &row[column];
                    let scalar = match value {
                        Value::Null => None,
                        Value::String(v) => Some(v.clone()),
                        Value::Number(v) => Some(v.to_string()),
                        _ => return Err(invalid("Backup row must contain scalar values")),
                    };
                    query.push_bind(scalar);
                }
                query.push(format!(
                    ") ON CONFLICT({}) DO UPDATE SET {updates}",
                    keys.join(",")
                ));
                query.build().execute(&mut *tx).await?;
                count += 1;
            }
        }
        tx.commit().await?;
        Ok(count)
    }
}
