//! SQLite row conversions and checked primitive values.
use crate::models::*;
use crate::{Result, StorageError};
use sqlx::{sqlite::SqliteQueryResult, Row};

pub(crate) fn account_from_row(row: sqlx::sqlite::SqliteRow) -> Result<Account> {
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

pub(crate) fn sync_run_from_row(row: sqlx::sqlite::SqliteRow) -> Result<SyncRun> {
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

pub(crate) fn work_download_state_from_row(
    row: sqlx::sqlite::SqliteRow,
) -> Result<WorkDownloadState> {
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

pub(crate) fn work_download_state_from_product_row(
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

pub(crate) fn bool_to_i64(value: bool) -> i64 {
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

pub(crate) fn u64_to_i64(value: u64, field: &'static str) -> Result<i64> {
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

pub(crate) fn ensure_changed(
    result: SqliteQueryResult,
    entity: &'static str,
    id: &str,
) -> Result<()> {
    if result.rows_affected() == 0 {
        Err(StorageError::NotFound {
            entity,
            id: id.to_owned(),
        })
    } else {
        Ok(())
    }
}
