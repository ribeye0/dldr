use super::*;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub work_ids: Vec<String>,
}

pub(super) fn playlist_name(name: &str) -> Result<&str> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
        return Err(StorageError::InvalidPlaylist(
            "Use a playlist name of 1–80 characters without control characters.",
        ));
    }
    Ok(name)
}

impl Storage {
    pub async fn playlists(&self) -> Result<Vec<Playlist>> {
        let mut tx = self.pool.begin().await?;
        let rows = sqlx::query("SELECT id,name FROM playlists ORDER BY id='0' DESC,created_at,id")
            .fetch_all(&mut *tx)
            .await?;
        let entries =
            sqlx::query("SELECT playlist_id,work_id FROM playlist_works ORDER BY position,work_id")
                .fetch_all(&mut *tx)
                .await?;
        let mut works: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for row in entries {
            works
                .entry(row.try_get("playlist_id")?)
                .or_default()
                .push(row.try_get("work_id")?);
        }
        let lists = rows
            .into_iter()
            .map(|row| {
                let id: String = row.try_get("id")?;
                Ok(Playlist {
                    work_ids: works.remove(&id).unwrap_or_default(),
                    id,
                    name: row.try_get("name")?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        tx.commit().await?;
        Ok(lists)
    }

    pub async fn create_playlist(&self, name: &str) -> Result<String> {
        let name = playlist_name(name)?;
        Ok(sqlx::query_scalar(
            "INSERT INTO playlists(id,name) VALUES(lower(hex(randomblob(16))),?1) RETURNING id",
        )
        .bind(name)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn rename_playlist(&self, id: &str, name: &str) -> Result<()> {
        let name = playlist_name(name)?;
        let result = sqlx::query("UPDATE playlists SET name=?2 WHERE id=?1")
            .bind(id)
            .bind(name)
            .execute(&self.pool)
            .await?;
        if result.rows_affected() == 0 {
            return Err(StorageError::InvalidPlaylist("Playlist not found."));
        }
        Ok(())
    }

    pub async fn set_playlist_work(&self, id: &str, work_id: &str, present: bool) -> Result<()> {
        // Admission and insertion share the write lock; simultaneous adds cannot
        // reorder duplicates or race metadata deletion. Download state is unrelated.
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM playlists WHERE id=?1)")
            .bind(id)
            .fetch_one(&mut *tx)
            .await?;
        if !exists {
            return Err(StorageError::InvalidPlaylist("Playlist not found."));
        }
        if present {
            let known: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM works WHERE work_id=?1)")
                    .bind(work_id)
                    .fetch_one(&mut *tx)
                    .await?;
            if !known {
                return Err(StorageError::InvalidPlaylist("Work not found."));
            }
            sqlx::query("INSERT INTO playlist_works(playlist_id,work_id,position) VALUES(?1,?2,(SELECT coalesce(max(position),-1)+1 FROM playlist_works WHERE playlist_id=?1)) ON CONFLICT(playlist_id,work_id) DO NOTHING")
                .bind(id).bind(work_id).execute(&mut *tx).await?;
        } else {
            sqlx::query("DELETE FROM playlist_works WHERE playlist_id=?1 AND work_id=?2")
                .bind(id)
                .bind(work_id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }
}
