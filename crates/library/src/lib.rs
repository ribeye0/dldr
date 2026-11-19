//! Shared application services used by the desktop and CLI adapters.
mod accounts;
mod auth;
mod bulk;
mod contracts;
pub mod cover;
mod credits;
pub mod download_progress;
mod downloads;
mod error;
mod export;
mod finalization;
mod instance;
mod local;
mod metadata;
mod paths;
pub mod product_page;
mod products;
mod serial;
mod sources;
mod transfers;
mod verification;
mod work_ids;

pub use auth::{
    DlsiteAuthSource, TwoFactorPrompt, TwoFactorPromptRequest, TwoFactorPromptResponse,
};
pub use contracts::*;
pub use dldr_jobs::CancellationToken;
pub use error::LibraryError;
pub use instance::{profile_dir, LibraryInstanceGuard};
pub use sources::{
    AccountSyncSource, DlsitePublicMetadataSource, DlsiteSource, LocalWorkMetadataSource,
    WorkDownloadSource,
};
pub use transfers::{DiskBudget, TransferCoordinator};
pub use verification::{VerificationItem, VerificationReport};
pub use work_ids::{
    detect_unique_work_id_in_path, detect_work_ids_in_path, detect_work_ids_in_text,
    DL_SITE_WORK_ID_PREFIXES,
};

use chrono::{DateTime, SecondsFormat, Utc};
use dldr_credentials::CredentialStore;
use dldr_download::UnpackPolicy;
use dldr_storage::Storage;
use std::{collections::HashMap, sync::Arc};

pub type Result<T> = std::result::Result<T, LibraryError>;
pub const MAX_PARALLEL_WORK_DOWNLOADS: usize = 4;
const DOWNLOAD_CANCELLATION_POLL_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(50);

#[derive(Clone)]
pub struct Library {
    transfers: TransferCoordinator,
    storage: Storage,
    credentials: Arc<dyn CredentialStore>,
    /// One lock per account, held across authentication only.
    ///
    /// Without it, jobs started together would each begin their own login and each raise its
    /// own two-factor prompt. Serializing lets the first login store a session the others can
    /// then reuse.
    login_locks: Arc<std::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>,
    work_locks: Arc<std::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>,
    credit_locks: Arc<std::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>>,
}

impl Library {
    pub fn new(storage: Storage, credentials: Arc<dyn CredentialStore>) -> Self {
        Self {
            storage,
            credentials,
            transfers: TransferCoordinator::default(),
            login_locks: Arc::new(std::sync::Mutex::new(HashMap::new())),
            work_locks: Arc::new(std::sync::Mutex::new(HashMap::new())),
            credit_locks: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }

    pub fn storage(&self) -> &Storage {
        &self.storage
    }
}

fn unpack_policy_storage_value(policy: UnpackPolicy) -> &'static str {
    match policy {
        UnpackPolicy::KeepArchives => "keep_archives",
        UnpackPolicy::UnpackWhenRecognized => "unpack_when_recognized",
    }
}

fn datetime_to_string(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn now_string() -> String {
    datetime_to_string(Utc::now())
}

#[cfg(test)]
mod tests;
