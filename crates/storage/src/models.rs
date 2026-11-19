use crate::{LanguagePriority, Result, StorageError, UiLocale};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkDownloadStatus {
    NotDownloaded,
    Downloading,
    Downloaded,
    Failed,
    Cancelled,
}

impl WorkDownloadStatus {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::NotDownloaded => "not_downloaded",
            Self::Downloading => "downloading",
            Self::Downloaded => "downloaded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub(super) fn from_storage_value(value: &str) -> Result<Self> {
        match value {
            "downloading" => Ok(Self::Downloading),
            "downloaded" => Ok(Self::Downloaded),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(StorageError::InvalidStoredValue {
                field: "work_downloads.status",
                value: value.to_owned(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkDownloadState {
    pub status: WorkDownloadStatus,
    pub local_path: Option<String>,
    pub staging_path: Option<String>,
    pub unpack_policy: Option<String>,
    pub bytes_received: u64,
    pub bytes_total: Option<u64>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub updated_at: Option<String>,
}

impl Default for WorkDownloadState {
    fn default() -> Self {
        Self {
            status: WorkDownloadStatus::NotDownloaded,
            local_path: None,
            staging_path: None,
            unpack_policy: None,
            bytes_received: 0,
            bytes_total: None,
            error_code: None,
            error_message: None,
            started_at: None,
            completed_at: None,
            updated_at: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkDownloadUpdate {
    pub work_id: String,
    pub status: WorkDownloadStatus,
    pub local_path: Option<String>,
    pub staging_path: Option<String>,
    pub unpack_policy: String,
    pub bytes_received: u64,
    pub bytes_total: Option<u64>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub updated_at: String,
}

impl From<WorkDownloadUpdate> for WorkDownloadState {
    fn from(update: WorkDownloadUpdate) -> Self {
        Self {
            status: update.status,
            local_path: update.local_path,
            staging_path: update.staging_path,
            unpack_policy: Some(update.unpack_policy),
            bytes_received: update.bytes_received,
            bytes_total: update.bytes_total,
            error_code: update.error_code,
            error_message: update.error_message,
            started_at: update.started_at,
            completed_at: update.completed_at,
            updated_at: Some(update.updated_at),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalWorkDownloadImport {
    pub work: CachedWork,
    pub download: WorkDownloadUpdate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppSettings {
    pub auto_extract: bool,
    pub ui_locale: UiLocale,
    pub library_root: Option<String>,
    pub download_root: Option<String>,
    pub language_priority: LanguagePriority,
    pub title_language_follows_ui: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            auto_extract: true,
            ui_locale: UiLocale::default(),
            library_root: None,
            download_root: None,
            language_priority: LanguagePriority::default(),
            title_language_follows_ui: true,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppSettingsPatch {
    pub auto_extract: Option<bool>,
    pub ui_locale: Option<UiLocale>,
    pub library_root: Option<String>,
    pub download_root: Option<String>,
    pub language_priority: Option<LanguagePriority>,
    pub title_language_follows_ui: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub id: String,
    pub label: String,
    pub login_name: Option<String>,
    pub credential_ref: Option<String>,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
    pub last_login_at: Option<String>,
    pub last_sync_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountUpsert {
    pub id: String,
    pub label: String,
    pub login_name: Option<String>,
    pub credential_ref: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedWork {
    pub work_id: String,
    pub title_json: String,
    pub maker_id: Option<String>,
    pub maker_json: Option<String>,
    pub work_type: Option<String>,
    pub age_category: Option<String>,
    pub thumbnail_url: Option<String>,
    pub registered_at: Option<String>,
    pub published_at: Option<String>,
    pub updated_at: Option<String>,
    pub raw_json: String,
    pub last_detail_sync_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountWork {
    pub work_id: String,
    pub purchased_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountSyncCommit {
    pub sync_run_id: String,
    pub account_id: String,
    pub started_at: String,
    pub completed_at: String,
    pub works: Vec<CachedWork>,
    pub account_works: Vec<AccountWork>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncFailure {
    pub sync_run_id: String,
    pub account_id: String,
    pub started_at: String,
    pub completed_at: String,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncCancellation {
    pub sync_run_id: String,
    pub account_id: String,
    pub started_at: String,
    pub completed_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncRunStatus {
    Started,
    Completed,
    Failed,
    Cancelled,
}

impl SyncRunStatus {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub(super) fn from_str(value: &str) -> Result<Self> {
        match value {
            "started" => Ok(Self::Started),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(StorageError::InvalidStoredValue {
                field: "sync_runs.status",
                value: value.to_owned(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncRun {
    pub id: String,
    pub account_id: String,
    pub status: SyncRunStatus,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductSort {
    TitleAsc,
    LatestPurchaseDesc,
    PublishedAtDesc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductAgeCategory {
    All,
    R15,
    R18,
}

impl ProductAgeCategory {
    pub(super) fn as_storage_value(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::R15 => "r15",
            Self::R18 => "r18",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductTypeGroup {
    Audio,
    Video,
    Game,
    Image,
    Other,
}

impl ProductTypeGroup {
    pub(super) fn as_storage_value(self) -> &'static str {
        match self {
            Self::Audio => "audio",
            Self::Video => "video",
            Self::Game => "game",
            Self::Image => "image",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductSourceGroup {
    Owned,
    LocalOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductListQuery {
    pub playlist_id: Option<String>,
    pub genre_names: Vec<String>,
    pub excluded_genre_names: Vec<String>,
    pub voice_names: Vec<String>,
    pub excluded_voice_names: Vec<String>,
    pub downloaded_only: bool,
    pub search: Option<String>,
    pub account_id: Option<String>,
    pub account_ids: Vec<String>,
    pub type_group: Option<ProductTypeGroup>,
    pub type_groups: Vec<ProductTypeGroup>,
    pub age_category: Option<ProductAgeCategory>,
    pub age_categories: Vec<ProductAgeCategory>,
    pub source_groups: Vec<ProductSourceGroup>,
    pub maker_keys: Vec<String>,
    pub language_priority: LanguagePriority,
    pub sort: ProductSort,
    pub limit: u32,
    pub offset: u32,
}

impl Default for ProductListQuery {
    fn default() -> Self {
        Self {
            playlist_id: None,
            genre_names: Vec::new(),
            excluded_genre_names: Vec::new(),
            voice_names: Vec::new(),
            excluded_voice_names: Vec::new(),
            downloaded_only: false,
            search: None,
            account_id: None,
            account_ids: Vec::new(),
            type_group: None,
            type_groups: Vec::new(),
            age_category: None,
            age_categories: Vec::new(),
            source_groups: Vec::new(),
            maker_keys: Vec::new(),
            language_priority: LanguagePriority::default(),
            sort: ProductSort::LatestPurchaseDesc,
            limit: 100,
            offset: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductListPage {
    pub total_count: u64,
    pub products: Vec<ProductListItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductFilterFacets {
    pub genres: Vec<ProductNameFacet>,
    pub voices: Vec<ProductNameFacet>,
    pub makers: Vec<ProductMakerFacet>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductMakerFacet {
    pub key: String,
    pub name: String,
    pub count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductNameFacet {
    pub name: String,
    pub count: u64,
    pub genre_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductListItem {
    pub work_id: String,
    pub title: String,
    pub maker_name: Option<String>,
    pub work_type: Option<String>,
    pub age_category: Option<String>,
    pub thumbnail_url: Option<String>,
    pub content_size_bytes: Option<u64>,
    pub published_at: Option<String>,
    pub updated_at: Option<String>,
    pub earliest_purchased_at: Option<String>,
    pub latest_purchased_at: Option<String>,
    pub credit_groups: Vec<ProductCreditGroup>,
    pub tags: Vec<ProductTag>,
    pub download: WorkDownloadState,
    pub owners: Vec<ProductOwner>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductDetail {
    pub work_id: String,
    pub title: String,
    pub title_variants: Vec<ProductTextValue>,
    pub maker_id: Option<String>,
    pub maker_name: Option<String>,
    pub maker_names: Vec<ProductTextValue>,
    pub work_type: Option<String>,
    pub age_category: Option<String>,
    pub thumbnail_url: Option<String>,
    pub content_size_bytes: Option<u64>,
    pub registered_at: Option<String>,
    pub published_at: Option<String>,
    pub updated_at: Option<String>,
    pub last_detail_sync_at: String,
    pub earliest_purchased_at: Option<String>,
    pub latest_purchased_at: Option<String>,
    pub credit_groups: Vec<ProductCreditGroup>,
    pub tags: Vec<ProductTag>,
    pub download: WorkDownloadState,
    pub owners: Vec<ProductOwner>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductTextValue {
    pub language: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductCreditGroup {
    pub kind: String,
    pub label: String,
    pub names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductTag {
    pub class: String,
    pub name: String,
    pub genre_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductOwner {
    pub account_id: String,
    pub label: String,
    pub purchased_at: Option<String>,
}
