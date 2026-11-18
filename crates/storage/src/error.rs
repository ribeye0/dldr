#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("{0}")]
    InvalidPlaylist(&'static str),
    #[error("{0}")]
    InvalidSearch(&'static str),
    #[error("database error")]
    Database(#[from] sqlx::Error),
    #[error("database migration error")]
    Migration(#[from] sqlx::migrate::MigrateError),
    #[error("write transaction is already finished")]
    TransactionFinished,
    #[error("{entity} not found: {id}")]
    NotFound { entity: &'static str, id: String },
    #[error("invalid stored value for {field}: {value}")]
    InvalidStoredValue { field: &'static str, value: String },
    #[error("invalid language priority: {0}")]
    InvalidLanguagePriority(&'static str),
}

impl StorageError {
    /// Stable database diagnostics without SQL text, bound values or schema messages.
    pub fn support_details(&self) -> serde_json::Value {
        let mut details = serde_json::json!({"failureKind": "storage"});
        if let Self::Database(error) = self {
            if let Some(database) = error.as_database_error() {
                details["databaseCode"] = serde_json::json!(database.code());
                details["databaseKind"] = serde_json::json!(format!("{:?}", database.kind()));
            }
        }
        if let Self::Database(error) = self {
            if error.as_database_error().is_none() {
                let kind = match error {
                    sqlx::Error::PoolTimedOut => "PoolTimedOut",
                    sqlx::Error::PoolClosed => "PoolClosed",
                    sqlx::Error::RowNotFound => "RowNotFound",
                    sqlx::Error::ColumnDecode { .. } | sqlx::Error::Decode(_) => "ColumnDecode",
                    _ => "Other",
                };
                details["databaseKind"] = serde_json::json!(kind);
                if let sqlx::Error::Io(io) = error {
                    details["osCode"] = serde_json::json!(io.raw_os_error());
                }
            }
        } else if matches!(self, Self::Migration(_)) {
            details["databaseKind"] = serde_json::json!("Migration");
        }
        details
    }
}
