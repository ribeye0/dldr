use crate::models::{
    AccountSyncCommit, AccountUpsert, AccountWork, CachedWork, SyncCancellation, SyncFailure,
    SyncRunStatus, WorkDownloadUpdate,
};
use crate::rows::{bool_to_i64, ensure_changed, u64_to_i64};
use crate::{Result, StorageError};
use sqlx::{sqlite::SqliteQueryResult, Sqlite, Transaction};

pub struct WriteTransaction<'storage> {
    pub(super) transaction: Option<Transaction<'storage, Sqlite>>,
}

impl WriteTransaction<'_> {
    pub async fn execute(&mut self, sql: &'static str) -> Result<SqliteQueryResult> {
        let transaction = self
            .transaction
            .as_mut()
            .ok_or(StorageError::TransactionFinished)?;

        Ok(sqlx::query(sql).execute(&mut **transaction).await?)
    }

    pub async fn commit(mut self) -> Result<()> {
        let transaction = self
            .transaction
            .take()
            .ok_or(StorageError::TransactionFinished)?;
        transaction.commit().await?;
        Ok(())
    }

    pub async fn rollback(mut self) -> Result<()> {
        let transaction = self
            .transaction
            .take()
            .ok_or(StorageError::TransactionFinished)?;
        transaction.rollback().await?;
        Ok(())
    }

    pub(super) async fn set_setting(&mut self, key: &str, value: Option<&str>) -> Result<()> {
        let transaction = self
            .transaction
            .as_mut()
            .ok_or(StorageError::TransactionFinished)?;

        match value {
            Some(value) => {
                sqlx::query(
                    "INSERT INTO app_settings (key, value, updated_at)
                     VALUES (?1, ?2, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                     ON CONFLICT(key) DO UPDATE SET
                        value = excluded.value,
                        updated_at = excluded.updated_at",
                )
                .bind(key)
                .bind(value)
                .execute(&mut **transaction)
                .await?;
            }
            None => {
                sqlx::query("DELETE FROM app_settings WHERE key = ?1")
                    .bind(key)
                    .execute(&mut **transaction)
                    .await?;
            }
        }

        Ok(())
    }

    pub async fn upsert_account(&mut self, account: &AccountUpsert) -> Result<()> {
        let transaction = self
            .transaction
            .as_mut()
            .ok_or(StorageError::TransactionFinished)?;

        sqlx::query(
            "INSERT INTO accounts (
                id, label, login_name, credential_ref, enabled, created_at, updated_at
             )
             VALUES (
                ?1, ?2, ?3, ?4, ?5,
                strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             )
             ON CONFLICT(id) DO UPDATE SET
                label = excluded.label,
                login_name = excluded.login_name,
                credential_ref = excluded.credential_ref,
                enabled = excluded.enabled,
                updated_at = excluded.updated_at",
        )
        .bind(&account.id)
        .bind(&account.label)
        .bind(&account.login_name)
        .bind(&account.credential_ref)
        .bind(bool_to_i64(account.enabled))
        .execute(&mut **transaction)
        .await?;

        Ok(())
    }

    pub async fn set_account_enabled(&mut self, account_id: &str, enabled: bool) -> Result<()> {
        let transaction = self
            .transaction
            .as_mut()
            .ok_or(StorageError::TransactionFinished)?;

        let result = sqlx::query(
            "UPDATE accounts
             SET enabled = ?2,
                 updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?1",
        )
        .bind(account_id)
        .bind(bool_to_i64(enabled))
        .execute(&mut **transaction)
        .await?;

        ensure_changed(result, "account", account_id)
    }

    pub async fn delete_account(&mut self, account_id: &str) -> Result<()> {
        let transaction = self
            .transaction
            .as_mut()
            .ok_or(StorageError::TransactionFinished)?;

        let result = sqlx::query("DELETE FROM accounts WHERE id = ?1")
            .bind(account_id)
            .execute(&mut **transaction)
            .await?;

        ensure_changed(result, "account", account_id)
    }

    pub async fn record_account_login(
        &mut self,
        account_id: &str,
        logged_in_at: &str,
    ) -> Result<()> {
        let transaction = self
            .transaction
            .as_mut()
            .ok_or(StorageError::TransactionFinished)?;

        let result = sqlx::query(
            "UPDATE accounts
             SET last_login_at = ?2,
                 updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?1",
        )
        .bind(account_id)
        .bind(logged_in_at)
        .execute(&mut **transaction)
        .await?;

        ensure_changed(result, "account", account_id)
    }

    pub async fn commit_account_sync(&mut self, sync: &AccountSyncCommit) -> Result<()> {
        self.ensure_account_exists(&sync.account_id).await?;
        self.insert_sync_run(
            &sync.sync_run_id,
            &sync.account_id,
            SyncRunStatus::Completed,
            &sync.started_at,
            Some(&sync.completed_at),
            None,
            None,
        )
        .await?;

        for work in &sync.works {
            self.upsert_work(work).await?;
        }

        for account_work in &sync.account_works {
            self.upsert_account_work(
                &sync.account_id,
                account_work,
                &sync.sync_run_id,
                &sync.completed_at,
            )
            .await?;
        }

        self.mark_account_works_not_seen_in_sync(&sync.account_id, &sync.sync_run_id)
            .await?;
        self.record_account_sync_completed(&sync.account_id, &sync.completed_at)
            .await?;

        Ok(())
    }

    pub async fn record_sync_failure(&mut self, failure: &SyncFailure) -> Result<()> {
        self.ensure_account_exists(&failure.account_id).await?;
        self.insert_sync_run(
            &failure.sync_run_id,
            &failure.account_id,
            SyncRunStatus::Failed,
            &failure.started_at,
            Some(&failure.completed_at),
            failure.error_code.as_deref(),
            failure.error_message.as_deref(),
        )
        .await
    }

    pub async fn record_sync_cancellation(
        &mut self,
        cancellation: &SyncCancellation,
    ) -> Result<()> {
        self.ensure_account_exists(&cancellation.account_id).await?;
        self.insert_sync_run(
            &cancellation.sync_run_id,
            &cancellation.account_id,
            SyncRunStatus::Cancelled,
            &cancellation.started_at,
            Some(&cancellation.completed_at),
            Some("cancelled"),
            None,
        )
        .await
    }

    pub async fn save_work_download(&mut self, download: &WorkDownloadUpdate) -> Result<()> {
        self.ensure_work_exists(&download.work_id).await?;
        let transaction = self
            .transaction
            .as_mut()
            .ok_or(StorageError::TransactionFinished)?;

        sqlx::query(
            "INSERT INTO work_downloads (
                work_id, status, local_path, staging_path, unpack_policy,
                bytes_received, bytes_total, error_code, error_message,
                started_at, completed_at, updated_at
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(work_id) DO UPDATE SET
                status = excluded.status,
                local_path = excluded.local_path,
                staging_path = excluded.staging_path,
                unpack_policy = excluded.unpack_policy,
                bytes_received = excluded.bytes_received,
                bytes_total = excluded.bytes_total,
                error_code = excluded.error_code,
                error_message = excluded.error_message,
                started_at = excluded.started_at,
                completed_at = excluded.completed_at,
                updated_at = excluded.updated_at",
        )
        .bind(&download.work_id)
        .bind(download.status.as_str())
        .bind(&download.local_path)
        .bind(&download.staging_path)
        .bind(&download.unpack_policy)
        .bind(u64_to_i64(
            download.bytes_received,
            "work_downloads.bytes_received",
        )?)
        .bind(
            download
                .bytes_total
                .map(|value| u64_to_i64(value, "work_downloads.bytes_total"))
                .transpose()?,
        )
        .bind(&download.error_code)
        .bind(&download.error_message)
        .bind(&download.started_at)
        .bind(&download.completed_at)
        .bind(&download.updated_at)
        .execute(&mut **transaction)
        .await?;

        Ok(())
    }

    pub async fn delete_work_download(&mut self, work_id: &str) -> Result<()> {
        self.ensure_work_exists(work_id).await?;
        let transaction = self
            .transaction
            .as_mut()
            .ok_or(StorageError::TransactionFinished)?;

        sqlx::query("DELETE FROM work_downloads WHERE work_id = ?1")
            .bind(work_id)
            .execute(&mut **transaction)
            .await?;

        Ok(())
    }

    pub(super) async fn insert_work_download_if_missing(
        &mut self,
        download: &WorkDownloadUpdate,
    ) -> Result<()> {
        self.ensure_work_exists(&download.work_id).await?;
        let transaction = self
            .transaction
            .as_mut()
            .ok_or(StorageError::TransactionFinished)?;

        sqlx::query(
            "INSERT INTO work_downloads (
                work_id, status, local_path, staging_path, unpack_policy,
                bytes_received, bytes_total, error_code, error_message,
                started_at, completed_at, updated_at
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(work_id) DO NOTHING",
        )
        .bind(&download.work_id)
        .bind(download.status.as_str())
        .bind(&download.local_path)
        .bind(&download.staging_path)
        .bind(&download.unpack_policy)
        .bind(u64_to_i64(
            download.bytes_received,
            "work_downloads.bytes_received",
        )?)
        .bind(
            download
                .bytes_total
                .map(|value| u64_to_i64(value, "work_downloads.bytes_total"))
                .transpose()?,
        )
        .bind(&download.error_code)
        .bind(&download.error_message)
        .bind(&download.started_at)
        .bind(&download.completed_at)
        .bind(&download.updated_at)
        .execute(&mut **transaction)
        .await?;

        Ok(())
    }

    pub(super) async fn ensure_account_exists(&mut self, account_id: &str) -> Result<()> {
        let transaction = self
            .transaction
            .as_mut()
            .ok_or(StorageError::TransactionFinished)?;
        let row = sqlx::query("SELECT 1 FROM accounts WHERE id = ?1")
            .bind(account_id)
            .fetch_optional(&mut **transaction)
            .await?;

        if row.is_some() {
            Ok(())
        } else {
            Err(StorageError::NotFound {
                entity: "account",
                id: account_id.to_owned(),
            })
        }
    }

    pub(super) async fn ensure_work_exists(&mut self, work_id: &str) -> Result<()> {
        let transaction = self
            .transaction
            .as_mut()
            .ok_or(StorageError::TransactionFinished)?;
        let row = sqlx::query("SELECT 1 FROM works WHERE work_id = ?1")
            .bind(work_id)
            .fetch_optional(&mut **transaction)
            .await?;

        if row.is_some() {
            Ok(())
        } else {
            Err(StorageError::NotFound {
                entity: "work",
                id: work_id.to_owned(),
            })
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn insert_sync_run(
        &mut self,
        sync_run_id: &str,
        account_id: &str,
        status: SyncRunStatus,
        started_at: &str,
        completed_at: Option<&str>,
        error_code: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<()> {
        let transaction = self
            .transaction
            .as_mut()
            .ok_or(StorageError::TransactionFinished)?;

        sqlx::query(
            "INSERT INTO sync_runs (
                id, account_id, status, started_at, completed_at, error_code,
                error_message
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind(sync_run_id)
        .bind(account_id)
        .bind(status.as_str())
        .bind(started_at)
        .bind(completed_at)
        .bind(error_code)
        .bind(error_message)
        .execute(&mut **transaction)
        .await?;

        Ok(())
    }

    pub(super) async fn upsert_work(&mut self, work: &CachedWork) -> Result<()> {
        let transaction = self
            .transaction
            .as_mut()
            .ok_or(StorageError::TransactionFinished)?;

        sqlx::query(
            "INSERT INTO works (
                work_id, title_json, maker_id, maker_json,
                work_type, age_category, thumbnail_url, registered_at,
                published_at, updated_at, raw_json, last_detail_sync_at
             )
             VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12
             )
             ON CONFLICT(work_id) DO UPDATE SET
                title_json = excluded.title_json,
                maker_id = excluded.maker_id,
                maker_json = excluded.maker_json,
                work_type = excluded.work_type,
                age_category = excluded.age_category,
                thumbnail_url = excluded.thumbnail_url,
                registered_at = excluded.registered_at,
                published_at = excluded.published_at,
                updated_at = excluded.updated_at,
                raw_json = CASE WHEN json_valid(works.raw_json) AND json_valid(excluded.raw_json)
                    THEN CASE WHEN json_type(works.raw_json, '$.public_credit_tags') = 'array'
                        AND json_type(excluded.raw_json, '$.public_credit_tags') IS NULL
                        THEN json_set(excluded.raw_json,
                            '$.public_credit_tags', json_extract(works.raw_json, '$.public_credit_tags'),
                            '$.public_credits_checked_at', json_extract(works.raw_json, '$.public_credits_checked_at'),
                            '$.public_credits_version', json_extract(works.raw_json, '$.public_credits_version'))
                        ELSE excluded.raw_json END
                    ELSE excluded.raw_json END,
                last_detail_sync_at = excluded.last_detail_sync_at",
        )
        .bind(&work.work_id)
        .bind(&work.title_json)
        .bind(&work.maker_id)
        .bind(&work.maker_json)
        .bind(&work.work_type)
        .bind(&work.age_category)
        .bind(&work.thumbnail_url)
        .bind(&work.registered_at)
        .bind(&work.published_at)
        .bind(&work.updated_at)
        .bind(&work.raw_json)
        .bind(&work.last_detail_sync_at)
        .execute(&mut **transaction)
        .await?;

        Ok(())
    }

    pub(super) async fn insert_work_if_missing(&mut self, work: &CachedWork) -> Result<()> {
        let transaction = self
            .transaction
            .as_mut()
            .ok_or(StorageError::TransactionFinished)?;

        sqlx::query(
            "INSERT INTO works (
                work_id, title_json, maker_id, maker_json,
                work_type, age_category, thumbnail_url, registered_at,
                published_at, updated_at, raw_json, last_detail_sync_at
             )
             VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12
             )
             ON CONFLICT(work_id) DO NOTHING",
        )
        .bind(&work.work_id)
        .bind(&work.title_json)
        .bind(&work.maker_id)
        .bind(&work.maker_json)
        .bind(&work.work_type)
        .bind(&work.age_category)
        .bind(&work.thumbnail_url)
        .bind(&work.registered_at)
        .bind(&work.published_at)
        .bind(&work.updated_at)
        .bind(&work.raw_json)
        .bind(&work.last_detail_sync_at)
        .execute(&mut **transaction)
        .await?;

        Ok(())
    }

    pub(super) async fn upsert_account_work(
        &mut self,
        account_id: &str,
        account_work: &AccountWork,
        sync_run_id: &str,
        seen_at: &str,
    ) -> Result<()> {
        let transaction = self
            .transaction
            .as_mut()
            .ok_or(StorageError::TransactionFinished)?;

        sqlx::query(
            "INSERT INTO account_works (
                account_id, work_id, purchased_at, first_seen_at, last_seen_at,
                last_seen_sync_run_id, is_current
             )
             VALUES (?1, ?2, ?3, ?4, ?4, ?5, 1)
             ON CONFLICT(account_id, work_id) DO UPDATE SET
                purchased_at = excluded.purchased_at,
                last_seen_at = excluded.last_seen_at,
                last_seen_sync_run_id = excluded.last_seen_sync_run_id,
                is_current = 1",
        )
        .bind(account_id)
        .bind(&account_work.work_id)
        .bind(&account_work.purchased_at)
        .bind(seen_at)
        .bind(sync_run_id)
        .execute(&mut **transaction)
        .await?;

        Ok(())
    }

    pub(super) async fn mark_account_works_not_seen_in_sync(
        &mut self,
        account_id: &str,
        sync_run_id: &str,
    ) -> Result<()> {
        let transaction = self
            .transaction
            .as_mut()
            .ok_or(StorageError::TransactionFinished)?;

        sqlx::query(
            "UPDATE account_works
             SET is_current = 0
             WHERE account_id = ?1
               AND is_current = 1
               AND (
                    last_seen_sync_run_id IS NULL
                    OR last_seen_sync_run_id <> ?2
               )",
        )
        .bind(account_id)
        .bind(sync_run_id)
        .execute(&mut **transaction)
        .await?;

        Ok(())
    }

    pub(super) async fn record_account_sync_completed(
        &mut self,
        account_id: &str,
        completed_at: &str,
    ) -> Result<()> {
        let transaction = self
            .transaction
            .as_mut()
            .ok_or(StorageError::TransactionFinished)?;

        let result = sqlx::query(
            "UPDATE accounts
             SET last_sync_at = ?2,
                 updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?1",
        )
        .bind(account_id)
        .bind(completed_at)
        .execute(&mut **transaction)
        .await?;

        ensure_changed(result, "account", account_id)
    }
}
