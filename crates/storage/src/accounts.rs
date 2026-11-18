use super::*;

impl Storage {
    pub async fn accounts(&self) -> Result<Vec<Account>> {
        let rows = sqlx::query(
            "SELECT id, label, login_name, credential_ref, enabled, created_at,
                    updated_at, last_login_at, last_sync_at
             FROM accounts
             ORDER BY label COLLATE NOCASE ASC, id ASC",
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(account_from_row).collect()
    }

    pub async fn save_account(&self, account: &AccountUpsert) -> Result<()> {
        let mut transaction = self.begin_write().await?;
        transaction.upsert_account(account).await?;
        transaction.commit().await?;

        Ok(())
    }

    pub async fn set_account_enabled(&self, account_id: &str, enabled: bool) -> Result<()> {
        let mut transaction = self.begin_write().await?;
        transaction.set_account_enabled(account_id, enabled).await?;
        transaction.commit().await?;

        Ok(())
    }

    pub async fn delete_account(&self, account_id: &str) -> Result<()> {
        let mut transaction = self.begin_write().await?;
        transaction.delete_account(account_id).await?;
        transaction.commit().await?;

        Ok(())
    }

    pub async fn record_account_login(&self, account_id: &str, logged_in_at: &str) -> Result<()> {
        let mut transaction = self.begin_write().await?;
        transaction
            .record_account_login(account_id, logged_in_at)
            .await?;
        transaction.commit().await?;

        Ok(())
    }

    pub async fn commit_account_sync(&self, sync: &AccountSyncCommit) -> Result<()> {
        let mut transaction = self.begin_write().await?;
        transaction.commit_account_sync(sync).await?;
        transaction.commit().await?;

        Ok(())
    }

    pub async fn record_sync_failure(&self, failure: &SyncFailure) -> Result<()> {
        let mut transaction = self.begin_write().await?;
        transaction.record_sync_failure(failure).await?;
        transaction.commit().await?;

        Ok(())
    }

    pub async fn record_sync_cancellation(&self, cancellation: &SyncCancellation) -> Result<()> {
        let mut transaction = self.begin_write().await?;
        transaction.record_sync_cancellation(cancellation).await?;
        transaction.commit().await?;

        Ok(())
    }

    pub async fn sync_runs_for_account(&self, account_id: &str) -> Result<Vec<SyncRun>> {
        let rows = sqlx::query(
            "SELECT id, account_id, status, started_at, completed_at, error_code,
                    error_message
             FROM sync_runs
             WHERE account_id = ?1
             ORDER BY started_at DESC, id DESC",
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(sync_run_from_row).collect()
    }
}
