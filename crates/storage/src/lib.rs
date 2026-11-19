mod accounts;
mod downloads;
mod product_metadata;
mod product_query;
mod products;
mod rows;
mod settings;
mod transaction;
pub use transaction::WriteTransaction;
mod genres;
mod locale;
pub use genres::display_name as genre_display_name;
pub use genres::display_name_for_id as genre_display_name_for_id;
pub use locale::UiLocale;
mod names;
mod search;
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
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    SqlitePool,
};
use std::path::Path;

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");
const LIBRARY_ROOT_KEY: &str = "library_root";
const DOWNLOAD_ROOT_KEY: &str = "download_root";
const LANGUAGE_PRIORITY_KEY: &str = "language_priority";
const TITLE_LANGUAGE_FOLLOWS_UI_KEY: &str = "title_language_follows_ui";
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
}

#[cfg(test)]
mod tests;
