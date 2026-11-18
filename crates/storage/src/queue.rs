use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DownloadQueueEntry {
    pub work_id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub thumbnail_url: Option<String>,
    pub batch_id: String,
    pub position: i64,
    pub account_id: Option<String>,
    pub library_root: String,
    pub download_root: String,
    pub unpack_policy: String,
    pub replace_existing: bool,
    pub status: String,
    pub error_message: Option<String>,
}

fn queue_entry_from_row(
    r: sqlx::sqlite::SqliteRow,
    priority: &LanguagePriority,
) -> Result<DownloadQueueEntry> {
    Ok(DownloadQueueEntry {
        title: crate::names::preferred_name_with_priority(
            Some(&r.try_get::<String, _>("title_json")?),
            priority,
        )
        .unwrap_or_default(),
        thumbnail_url: r.try_get("thumbnail_url")?,
        work_id: r.try_get("work_id")?,
        batch_id: r.try_get("batch_id")?,
        position: r.try_get("position")?,
        account_id: r.try_get("account_id")?,
        library_root: r.try_get("library_root")?,
        download_root: r.try_get("download_root")?,
        unpack_policy: r.try_get("unpack_policy")?,
        replace_existing: r.try_get::<i64, _>("replace_existing")? != 0,
        status: r.try_get("status")?,
        error_message: r.try_get("error_message")?,
    })
}

impl Storage {
    pub async fn download_concurrency(&self) -> Result<usize> {
        let value: Option<String> =
            sqlx::query_scalar("SELECT value FROM app_settings WHERE key='download_concurrency'")
                .fetch_optional(&self.pool)
                .await?;
        match value {
            None => Ok(2),
            Some(value) => value
                .parse::<usize>()
                .ok()
                .filter(|v| (1..=4).contains(v))
                .ok_or(StorageError::InvalidStoredValue {
                    field: "download_concurrency",
                    value,
                }),
        }
    }

    pub async fn save_download_concurrency(&self, value: usize) -> Result<()> {
        if !(1..=4).contains(&value) {
            return Err(StorageError::InvalidStoredValue {
                field: "download_concurrency",
                value: value.to_string(),
            });
        }
        sqlx::query("INSERT INTO app_settings(key,value) VALUES('download_concurrency',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value")
            .bind(value.to_string()).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn download_queue(&self) -> Result<Vec<DownloadQueueEntry>> {
        let priority = self.app_settings().await?.language_priority;
        let rows = sqlx::query("SELECT q.*,w.title_json,w.thumbnail_url FROM download_queue q JOIN works w ON w.work_id=q.work_id ORDER BY q.position,q.work_id").fetch_all(&self.pool).await?;
        rows.into_iter()
            .map(|row| queue_entry_from_row(row, &priority))
            .collect()
    }

    pub async fn queued_download(&self, work_id: &str) -> Result<Option<DownloadQueueEntry>> {
        let priority = self.app_settings().await?.language_priority;
        sqlx::query("SELECT q.*,w.title_json,w.thumbnail_url FROM download_queue q JOIN works w ON w.work_id=q.work_id WHERE q.work_id=?1")
            .bind(work_id).fetch_optional(&self.pool).await?.map(|row| queue_entry_from_row(row, &priority)).transpose()
    }

    pub async fn enqueue_downloads(&self, entries: &[DownloadQueueEntry]) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let next: i64 =
            sqlx::query_scalar("SELECT coalesce(max(position),0)+1 FROM download_queue")
                .fetch_one(&mut *tx)
                .await?;
        for (index, e) in entries.iter().enumerate() {
            sqlx::query("INSERT INTO download_queue(work_id,batch_id,position,account_id,library_root,download_root,unpack_policy,replace_existing,status)
                VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'queued') ON CONFLICT(work_id) DO UPDATE SET
                batch_id=excluded.batch_id,position=excluded.position,account_id=excluded.account_id,library_root=excluded.library_root,
                download_root=excluded.download_root,unpack_policy=excluded.unpack_policy,replace_existing=excluded.replace_existing,
                status='queued',error_message=NULL,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')")
                .bind(&e.work_id).bind(&e.batch_id).bind(next.saturating_add(index as i64)).bind(&e.account_id)
                .bind(&e.library_root).bind(&e.download_root).bind(&e.unpack_policy).bind(e.replace_existing)
                .execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn set_queue_status(
        &self,
        work_id: &str,
        batch: &str,
        status: &str,
        error: Option<&str>,
    ) -> Result<()> {
        sqlx::query("UPDATE download_queue SET status=?3,error_message=?4,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE work_id=?1 AND batch_id=?2")
            .bind(work_id).bind(batch).bind(status).bind(error).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn request_queue_stop(&self, work: &str, batch: &str, status: &str) -> Result<()> {
        sqlx::query("UPDATE download_queue SET status=?3,error_message=NULL WHERE work_id=?1 AND batch_id=?2 AND status IN ('queued','running','pausing','cancelling','paused')")
            .bind(work).bind(batch).bind(status).execute(&self.pool).await?;
        Ok(())
    }
    pub async fn finish_queue_stop(&self, work: &str, batch: &str, status: &str) -> Result<()> {
        sqlx::query("UPDATE download_queue SET status=?3 WHERE work_id=?1 AND batch_id=?2 AND status IN ('pausing','cancelling')")
            .bind(work).bind(batch).bind(status).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn recover_download_queue(&self) -> Result<()> {
        sqlx::query("UPDATE download_queue SET status='paused',error_message='Application restarted; resume to continue',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE status IN ('queued','running','pausing','cancelling')")
            .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn pause_queued_batch(&self, batch: &str) -> Result<()> {
        sqlx::query(
            "UPDATE download_queue SET status='paused' WHERE batch_id=?1 AND status='queued'",
        )
        .bind(batch)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn clear_finished_queue(&self) -> Result<()> {
        sqlx::query("DELETE FROM download_queue WHERE status IN ('completed','cancelled')")
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
