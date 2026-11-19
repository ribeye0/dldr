//! Download records and atomic local imports.
use crate::models::{
    Account, CachedWork, LocalWorkDownloadImport, WorkDownloadState, WorkDownloadUpdate,
};
use crate::rows::{account_from_row, work_download_state_from_row};
use crate::{Result, Storage, StorageError};
use sqlx::{QueryBuilder, Sqlite};

impl Storage {
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
}
