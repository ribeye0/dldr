use crate::state::BULK_DOWNLOAD_PAGE_LIMIT;
use crate::{errors::command_error, validation::*};
use dldr_library::{
    AccountRemovalReport, BulkWorkDownloadPreview, LocalWorkImportReport, SaveAccountRequest,
};
use dldr_storage::{
    Account, AppSettings, AppSettingsPatch, LanguagePriority, ProductAgeCategory,
    ProductCreditGroup, ProductDetail, ProductFilterFacets, ProductListItem, ProductListPage,
    ProductListQuery, ProductMakerFacet, ProductNameFacet, ProductOwner, ProductSort,
    ProductSourceGroup, ProductTag, ProductTextValue, ProductTypeGroup, UiLocale,
    WorkDownloadState, WorkDownloadStatus,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppSettingsDto {
    pub(crate) auto_extract: bool,
    pub(crate) ui_locale: UiLocale,
    pub(crate) library_root: Option<String>,
    pub(crate) download_root: Option<String>,
    pub(crate) language_priority: Vec<String>,
    pub(crate) title_language_follows_ui: bool,
}

impl From<AppSettings> for AppSettingsDto {
    fn from(settings: AppSettings) -> Self {
        Self {
            auto_extract: settings.auto_extract,
            ui_locale: settings.ui_locale,
            library_root: settings.library_root,
            download_root: settings.download_root,
            language_priority: settings.language_priority.into_codes(),
            title_language_follows_ui: settings.title_language_follows_ui,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SaveSettingsRequest {
    pub(crate) auto_extract: Option<bool>,
    pub(crate) ui_locale: Option<UiLocale>,
    pub(crate) library_root: Option<String>,
    pub(crate) download_root: Option<String>,
    pub(crate) language_priority: Option<Vec<String>>,
    pub(crate) title_language_follows_ui: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AccountDto {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) login_name: Option<String>,
    pub(crate) has_credential: bool,
    pub(crate) enabled: bool,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) last_login_at: Option<String>,
    pub(crate) last_sync_at: Option<String>,
}

impl AccountDto {
    pub(crate) fn from_account(account: Account, has_credential: bool) -> Self {
        Self {
            id: account.id,
            label: account.label,
            login_name: account.login_name,
            has_credential,
            enabled: account.enabled,
            created_at: account.created_at,
            updated_at: account.updated_at,
            last_login_at: account.last_login_at,
            last_sync_at: account.last_sync_at,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SaveAccountCommandRequest {
    pub(crate) id: Option<String>,
    pub(crate) label: String,
    pub(crate) login_name: Option<String>,
    pub(crate) password: Option<String>,
}

impl SaveAccountCommandRequest {
    pub(crate) fn into_library_request(self) -> Result<SaveAccountRequest, String> {
        Ok(SaveAccountRequest {
            id: normalize_optional_id(self.id)?,
            label: normalize_label(self.label)?,
            login_name: normalize_optional_string(self.login_name)?,
            password: normalize_secret(self.password)?,
            remember_password: true,
            enabled: true,
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SetAccountEnabledRequest {
    pub(crate) account_id: String,
    pub(crate) enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RemoveAccountRequest {
    pub(crate) account_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AccountRemovalReportDto {
    pub(crate) account_id: String,
    pub(crate) label: String,
    pub(crate) credential_deleted: bool,
}

impl From<AccountRemovalReport> for AccountRemovalReportDto {
    fn from(report: AccountRemovalReport) -> Self {
        Self {
            account_id: report.account_id,
            label: report.label,
            credential_deleted: report.credential_deleted,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ListProductsRequest {
    pub(crate) playlist_id: Option<String>,
    pub(crate) genre_names: Option<Vec<String>>,
    pub(crate) excluded_genre_names: Option<Vec<String>>,
    pub(crate) voice_names: Option<Vec<String>>,
    pub(crate) excluded_voice_names: Option<Vec<String>>,
    pub(crate) downloaded_only: Option<bool>,
    pub(crate) search: Option<String>,
    pub(crate) account_id: Option<String>,
    pub(crate) account_ids: Option<Vec<String>>,
    pub(crate) type_group: Option<ProductTypeGroupDto>,
    pub(crate) type_groups: Option<Vec<ProductTypeGroupDto>>,
    pub(crate) age_category: Option<ProductAgeCategoryDto>,
    pub(crate) age_categories: Option<Vec<ProductAgeCategoryDto>>,
    pub(crate) source_groups: Option<Vec<ProductSourceGroupDto>>,
    pub(crate) maker_keys: Option<Vec<String>>,
    pub(crate) sort: Option<ProductSortDto>,
    pub(crate) limit: Option<u32>,
    pub(crate) offset: Option<u32>,
}

impl ListProductsRequest {
    pub(crate) fn into_query(self) -> Result<ProductListQuery, String> {
        Ok(ProductListQuery {
            playlist_id: self.playlist_id,
            genre_names: normalize_optional_strings(self.genre_names.clone())?,
            excluded_genre_names: normalize_optional_strings(self.excluded_genre_names.clone())?,
            voice_names: normalize_optional_strings(self.voice_names.clone())?,
            excluded_voice_names: normalize_optional_strings(self.excluded_voice_names.clone())?,
            downloaded_only: self.downloaded_only.unwrap_or(false),
            search: normalize_optional_string(self.search)?,
            account_id: normalize_optional_id(self.account_id)?,
            account_ids: normalize_optional_ids(self.account_ids)?,
            type_group: self.type_group.map(Into::into),
            type_groups: self
                .type_groups
                .unwrap_or_default()
                .into_iter()
                .map(Into::into)
                .collect(),
            age_category: self.age_category.map(Into::into),
            age_categories: self
                .age_categories
                .unwrap_or_default()
                .into_iter()
                .map(Into::into)
                .collect(),
            source_groups: self
                .source_groups
                .unwrap_or_default()
                .into_iter()
                .map(Into::into)
                .collect(),
            maker_keys: normalize_optional_strings(self.maker_keys)?,
            sort: self.sort.unwrap_or_default().into(),
            language_priority: LanguagePriority::default(),
            limit: self.limit.unwrap_or(100).clamp(1, 500),
            offset: self.offset.unwrap_or(0),
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GetProductDetailRequest {
    pub(crate) work_id: String,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ProductSortDto {
    TitleAsc,
    #[default]
    LatestPurchaseDesc,
    PublishedAtDesc,
}

impl From<ProductSortDto> for ProductSort {
    fn from(sort: ProductSortDto) -> Self {
        match sort {
            ProductSortDto::TitleAsc => Self::TitleAsc,
            ProductSortDto::LatestPurchaseDesc => Self::LatestPurchaseDesc,
            ProductSortDto::PublishedAtDesc => Self::PublishedAtDesc,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ProductAgeCategoryDto {
    All,
    R15,
    R18,
}

impl From<ProductAgeCategoryDto> for ProductAgeCategory {
    fn from(age_category: ProductAgeCategoryDto) -> Self {
        match age_category {
            ProductAgeCategoryDto::All => Self::All,
            ProductAgeCategoryDto::R15 => Self::R15,
            ProductAgeCategoryDto::R18 => Self::R18,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ProductTypeGroupDto {
    Audio,
    Video,
    Game,
    Image,
    Other,
}

impl From<ProductTypeGroupDto> for ProductTypeGroup {
    fn from(type_group: ProductTypeGroupDto) -> Self {
        match type_group {
            ProductTypeGroupDto::Audio => Self::Audio,
            ProductTypeGroupDto::Video => Self::Video,
            ProductTypeGroupDto::Game => Self::Game,
            ProductTypeGroupDto::Image => Self::Image,
            ProductTypeGroupDto::Other => Self::Other,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ProductSourceGroupDto {
    Owned,
    LocalOnly,
}

impl From<ProductSourceGroupDto> for ProductSourceGroup {
    fn from(source_group: ProductSourceGroupDto) -> Self {
        match source_group {
            ProductSourceGroupDto::Owned => Self::Owned,
            ProductSourceGroupDto::LocalOnly => Self::LocalOnly,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProductListPageDto {
    pub(crate) total_count: u64,
    pub(crate) products: Vec<ProductListItemDto>,
}

impl From<ProductListPage> for ProductListPageDto {
    fn from(page: ProductListPage) -> Self {
        Self {
            total_count: page.total_count,
            products: page
                .products
                .into_iter()
                .map(ProductListItemDto::from)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BulkWorkDownloadPreviewDto {
    pub(crate) total_count: u64,
    pub(crate) requested_count: usize,
    pub(crate) skipped_downloaded_count: usize,
    pub(crate) skipped_queued_count: usize,
    pub(crate) planned_count: usize,
    pub(crate) failed_count: usize,
    pub(crate) known_expected_bytes: u64,
    pub(crate) total_expected_bytes: Option<u64>,
    pub(crate) unknown_size_count: usize,
}

impl BulkWorkDownloadPreviewDto {
    pub(crate) fn from_preview(
        preview: BulkWorkDownloadPreview,
        skipped_queued_count: usize,
    ) -> Self {
        Self {
            total_count: preview.total_count,
            requested_count: preview.requested_count,
            skipped_downloaded_count: preview.skipped_downloaded_count,
            skipped_queued_count,
            planned_count: preview.planned_count,
            failed_count: preview.failed_count,
            known_expected_bytes: preview.known_expected_bytes,
            total_expected_bytes: preview.total_expected_bytes,
            unknown_size_count: preview.unknown_size_count,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProductFilterFacetsDto {
    pub(crate) genres: Vec<ProductNameFacetDto>,
    pub(crate) voices: Vec<ProductNameFacetDto>,
    pub(crate) makers: Vec<ProductMakerFacetDto>,
}

impl From<ProductFilterFacets> for ProductFilterFacetsDto {
    fn from(facets: ProductFilterFacets) -> Self {
        Self {
            genres: facets
                .genres
                .into_iter()
                .map(ProductNameFacetDto::from)
                .collect(),
            voices: facets
                .voices
                .into_iter()
                .map(ProductNameFacetDto::from)
                .collect(),
            makers: facets
                .makers
                .into_iter()
                .map(ProductMakerFacetDto::from)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProductMakerFacetDto {
    pub(crate) key: String,
    pub(crate) name: String,
    pub(crate) count: u64,
}

impl From<ProductMakerFacet> for ProductMakerFacetDto {
    fn from(facet: ProductMakerFacet) -> Self {
        Self {
            key: facet.key,
            name: facet.name,
            count: facet.count,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProductNameFacetDto {
    pub(crate) name: String,
    pub(crate) count: u64,
    pub(crate) genre_id: Option<i64>,
}

impl From<ProductNameFacet> for ProductNameFacetDto {
    fn from(facet: ProductNameFacet) -> Self {
        Self {
            name: facet.name,
            count: facet.count,
            genre_id: facet.genre_id,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProductListItemDto {
    pub(crate) work_id: String,
    pub(crate) title: String,
    pub(crate) maker_name: Option<String>,
    pub(crate) work_type: Option<String>,
    pub(crate) age_category: Option<String>,
    pub(crate) thumbnail_url: Option<String>,
    pub(crate) content_size_bytes: Option<u64>,
    pub(crate) published_at: Option<String>,
    pub(crate) updated_at: Option<String>,
    pub(crate) earliest_purchased_at: Option<String>,
    pub(crate) latest_purchased_at: Option<String>,
    pub(crate) credit_groups: Vec<ProductCreditGroupDto>,
    pub(crate) tags: Vec<ProductTagDto>,
    pub(crate) download: WorkDownloadStateDto,
    pub(crate) owners: Vec<ProductOwnerDto>,
}

impl From<ProductListItem> for ProductListItemDto {
    fn from(product: ProductListItem) -> Self {
        Self {
            work_id: product.work_id,
            title: product.title,
            maker_name: product.maker_name,
            work_type: product.work_type,
            age_category: product.age_category,
            thumbnail_url: product.thumbnail_url,
            content_size_bytes: product.content_size_bytes,
            published_at: product.published_at,
            updated_at: product.updated_at,
            earliest_purchased_at: product.earliest_purchased_at,
            latest_purchased_at: product.latest_purchased_at,
            tags: product.tags.into_iter().map(ProductTagDto::from).collect(),
            credit_groups: product
                .credit_groups
                .into_iter()
                .map(ProductCreditGroupDto::from)
                .collect(),
            download: WorkDownloadStateDto::from(product.download),
            owners: product
                .owners
                .into_iter()
                .map(ProductOwnerDto::from)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProductDetailDto {
    pub(crate) work_id: String,
    pub(crate) title: String,
    pub(crate) title_variants: Vec<ProductTextValueDto>,
    pub(crate) maker_id: Option<String>,
    pub(crate) maker_name: Option<String>,
    pub(crate) maker_names: Vec<ProductTextValueDto>,
    pub(crate) work_type: Option<String>,
    pub(crate) age_category: Option<String>,
    pub(crate) thumbnail_url: Option<String>,
    pub(crate) content_size_bytes: Option<u64>,
    pub(crate) registered_at: Option<String>,
    pub(crate) published_at: Option<String>,
    pub(crate) updated_at: Option<String>,
    pub(crate) last_detail_sync_at: String,
    pub(crate) earliest_purchased_at: Option<String>,
    pub(crate) latest_purchased_at: Option<String>,
    pub(crate) credit_groups: Vec<ProductCreditGroupDto>,
    pub(crate) tags: Vec<ProductTagDto>,
    pub(crate) download: WorkDownloadStateDto,
    pub(crate) owners: Vec<ProductOwnerDto>,
}

impl From<ProductDetail> for ProductDetailDto {
    fn from(detail: ProductDetail) -> Self {
        Self {
            work_id: detail.work_id,
            title: detail.title,
            title_variants: detail
                .title_variants
                .into_iter()
                .map(ProductTextValueDto::from)
                .collect(),
            maker_id: detail.maker_id,
            maker_name: detail.maker_name,
            maker_names: detail
                .maker_names
                .into_iter()
                .map(ProductTextValueDto::from)
                .collect(),
            work_type: detail.work_type,
            age_category: detail.age_category,
            thumbnail_url: detail.thumbnail_url,
            content_size_bytes: detail.content_size_bytes,
            registered_at: detail.registered_at,
            published_at: detail.published_at,
            updated_at: detail.updated_at,
            last_detail_sync_at: detail.last_detail_sync_at,
            earliest_purchased_at: detail.earliest_purchased_at,
            latest_purchased_at: detail.latest_purchased_at,
            credit_groups: detail
                .credit_groups
                .into_iter()
                .map(ProductCreditGroupDto::from)
                .collect(),
            tags: detail.tags.into_iter().map(ProductTagDto::from).collect(),
            download: WorkDownloadStateDto::from(detail.download),
            owners: detail
                .owners
                .into_iter()
                .map(ProductOwnerDto::from)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProductTextValueDto {
    pub(crate) language: String,
    pub(crate) value: String,
}

impl From<ProductTextValue> for ProductTextValueDto {
    fn from(value: ProductTextValue) -> Self {
        Self {
            language: value.language,
            value: value.value,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkDownloadStateDto {
    pub(crate) status: WorkDownloadStatusDto,
    pub(crate) local_path: Option<String>,
    pub(crate) staging_path: Option<String>,
    pub(crate) unpack_policy: Option<String>,
    pub(crate) bytes_received: u64,
    pub(crate) bytes_total: Option<u64>,
    pub(crate) error_code: Option<String>,
    pub(crate) error_message: Option<String>,
    pub(crate) started_at: Option<String>,
    pub(crate) completed_at: Option<String>,
    pub(crate) updated_at: Option<String>,
}

impl From<WorkDownloadState> for WorkDownloadStateDto {
    fn from(state: WorkDownloadState) -> Self {
        Self {
            status: WorkDownloadStatusDto::from(state.status),
            local_path: state.local_path,
            staging_path: state.staging_path,
            unpack_policy: state.unpack_policy,
            bytes_received: state.bytes_received,
            bytes_total: state.bytes_total,
            error_code: state.error_code,
            error_message: state.error_message,
            started_at: state.started_at,
            completed_at: state.completed_at,
            updated_at: state.updated_at,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum WorkDownloadStatusDto {
    NotDownloaded,
    Downloading,
    Downloaded,
    Failed,
    Cancelled,
}

impl From<WorkDownloadStatus> for WorkDownloadStatusDto {
    fn from(status: WorkDownloadStatus) -> Self {
        match status {
            WorkDownloadStatus::NotDownloaded => Self::NotDownloaded,
            WorkDownloadStatus::Downloading => Self::Downloading,
            WorkDownloadStatus::Downloaded => Self::Downloaded,
            WorkDownloadStatus::Failed => Self::Failed,
            WorkDownloadStatus::Cancelled => Self::Cancelled,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalWorkImportReportDto {
    pub(crate) recovery_errors: Vec<String>,
    pub(crate) scanned_directories: usize,
    pub(crate) imported_count: usize,
    pub(crate) skipped_no_id: usize,
    pub(crate) skipped_ambiguous: usize,
    pub(crate) skipped_non_utf8: usize,
    pub(crate) skipped_existing: usize,
    pub(crate) metadata_candidate_count: usize,
    pub(crate) metadata_updated_count: usize,
    pub(crate) metadata_missing_count: usize,
    pub(crate) metadata_error: Option<String>,
    pub(crate) imported_works: Vec<LocalWorkImportItemDto>,
}

impl From<LocalWorkImportReport> for LocalWorkImportReportDto {
    fn from(report: LocalWorkImportReport) -> Self {
        Self {
            recovery_errors: report.recovery_errors,
            scanned_directories: report.scanned_directories,
            imported_count: report.imported_count,
            skipped_no_id: report.skipped_no_id,
            skipped_ambiguous: report.skipped_ambiguous,
            skipped_non_utf8: report.skipped_non_utf8,
            skipped_existing: report.skipped_existing,
            metadata_candidate_count: report.metadata_candidate_count,
            metadata_updated_count: report.metadata_updated_count,
            metadata_missing_count: report.metadata_missing_count,
            metadata_error: report.metadata_error,
            imported_works: report
                .imported_works
                .into_iter()
                .map(LocalWorkImportItemDto::from)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalWorkImportItemDto {
    pub(crate) work_id: String,
    pub(crate) local_path: String,
}

impl From<dldr_library::LocalWorkImportItem> for LocalWorkImportItemDto {
    fn from(item: dldr_library::LocalWorkImportItem) -> Self {
        Self {
            work_id: item.work_id,
            local_path: item.local_path.to_string_lossy().into_owned(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProductCreditGroupDto {
    pub(crate) kind: String,
    pub(crate) label: String,
    pub(crate) names: Vec<String>,
}

impl From<ProductCreditGroup> for ProductCreditGroupDto {
    fn from(group: ProductCreditGroup) -> Self {
        Self {
            kind: group.kind,
            label: group.label,
            names: group.names,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProductTagDto {
    pub(crate) class: String,
    pub(crate) name: String,
    pub(crate) genre_id: Option<i64>,
}

impl From<ProductTag> for ProductTagDto {
    fn from(tag: ProductTag) -> Self {
        Self {
            class: tag.class,
            name: tag.name,
            genre_id: tag.genre_id,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProductOwnerDto {
    pub(crate) account_id: String,
    pub(crate) label: String,
    pub(crate) purchased_at: Option<String>,
}

impl From<ProductOwner> for ProductOwnerDto {
    fn from(owner: ProductOwner) -> Self {
        Self {
            account_id: owner.account_id,
            label: owner.label,
            purchased_at: owner.purchased_at,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StartAccountSyncRequest {
    pub(crate) account_id: String,
    pub(crate) password: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StartWorkDownloadRequest {
    pub(crate) work_id: String,
    pub(crate) account_id: Option<String>,
    pub(crate) password: Option<String>,
    pub(crate) unpack_policy: Option<UnpackPolicyDto>,
    pub(crate) replace_existing: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BulkWorkDownloadCommandRequest {
    pub(crate) genre_names: Option<Vec<String>>,
    pub(crate) excluded_genre_names: Option<Vec<String>>,
    pub(crate) voice_names: Option<Vec<String>>,
    pub(crate) excluded_voice_names: Option<Vec<String>>,
    pub(crate) max_concurrent_works: Option<usize>,
    pub(crate) work_ids: Option<Vec<String>>,
    pub(crate) search: Option<String>,
    pub(crate) account_id: Option<String>,
    pub(crate) account_ids: Option<Vec<String>>,
    pub(crate) type_group: Option<ProductTypeGroupDto>,
    pub(crate) type_groups: Option<Vec<ProductTypeGroupDto>>,
    pub(crate) age_category: Option<ProductAgeCategoryDto>,
    pub(crate) age_categories: Option<Vec<ProductAgeCategoryDto>>,
    pub(crate) source_groups: Option<Vec<ProductSourceGroupDto>>,
    pub(crate) maker_keys: Option<Vec<String>>,
    pub(crate) sort: Option<ProductSortDto>,
    pub(crate) unpack_policy: Option<UnpackPolicyDto>,
    pub(crate) skip_downloaded: Option<bool>,
}

impl BulkWorkDownloadCommandRequest {
    pub(crate) fn concurrency(&self) -> Result<usize, String> {
        let value = self.max_concurrent_works.unwrap_or(2);
        if !(1..=dldr_library::MAX_PARALLEL_WORK_DOWNLOADS).contains(&value) {
            return Err("Simultaneous works per batch must be between 1 and 4".into());
        }
        Ok(value)
    }
    pub(crate) fn selected_work_ids(&self) -> Result<Option<Vec<String>>, String> {
        // Some([]) must never fall back to downloading the whole query.
        self.work_ids
            .as_ref()
            .map(|ids| normalize_optional_ids(Some(ids.clone())))
            .transpose()
    }

    pub(crate) fn to_query(&self) -> Result<ProductListQuery, String> {
        Ok(ProductListQuery {
            playlist_id: None,
            genre_names: normalize_optional_strings(self.genre_names.clone())?,
            excluded_genre_names: normalize_optional_strings(self.excluded_genre_names.clone())?,
            voice_names: normalize_optional_strings(self.voice_names.clone())?,
            excluded_voice_names: normalize_optional_strings(self.excluded_voice_names.clone())?,
            downloaded_only: false,
            search: normalize_optional_string(self.search.clone())?,
            account_id: normalize_optional_id(self.account_id.clone())?,
            account_ids: normalize_optional_ids(self.account_ids.clone())?,
            type_group: self.type_group.map(Into::into),
            type_groups: self
                .type_groups
                .clone()
                .unwrap_or_default()
                .into_iter()
                .map(Into::into)
                .collect(),
            age_category: self.age_category.map(Into::into),
            age_categories: self
                .age_categories
                .clone()
                .unwrap_or_default()
                .into_iter()
                .map(Into::into)
                .collect(),
            source_groups: self
                .source_groups
                .clone()
                .unwrap_or_default()
                .into_iter()
                .map(Into::into)
                .collect(),
            maker_keys: normalize_optional_strings(self.maker_keys.clone())?,
            sort: self.sort.unwrap_or_default().into(),
            language_priority: LanguagePriority::default(),
            limit: BULK_DOWNLOAD_PAGE_LIMIT,
            offset: 0,
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OpenWorkDownloadRequest {
    pub(crate) work_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeleteWorkDownloadRequest {
    pub(crate) work_id: String,
    #[serde(default)]
    pub(crate) completed_only: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MarkWorkDownloadedRequest {
    pub(crate) work_id: String,
    pub(crate) local_path: String,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum UnpackPolicyDto {
    KeepArchives,
    #[default]
    UnpackWhenRecognized,
}

impl From<UnpackPolicyDto> for dldr_download::UnpackPolicy {
    fn from(policy: UnpackPolicyDto) -> Self {
        match policy {
            UnpackPolicyDto::KeepArchives => Self::KeepArchives,
            UnpackPolicyDto::UnpackWhenRecognized => Self::UnpackWhenRecognized,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StartJobResponse {
    pub(crate) job_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JobIdRequest {
    pub(crate) job_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JobLogsRequest {
    pub(crate) job_id: String,
    pub(crate) after_sequence: Option<u64>,
    pub(crate) limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClearFinishedJobsResponse {
    pub(crate) removed_count: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ListAuditEventsRequest {
    pub(crate) run_id: Option<String>,
    pub(crate) limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AuditLogDirDto {
    pub(crate) path: String,
}

impl SaveSettingsRequest {
    pub(crate) fn into_app_settings_patch(self) -> Result<AppSettingsPatch, String> {
        Ok(AppSettingsPatch {
            auto_extract: self.auto_extract,
            title_language_follows_ui: self.title_language_follows_ui,
            ui_locale: self.ui_locale,
            library_root: normalize_path_setting(self.library_root)?,
            download_root: normalize_path_setting(self.download_root)?,
            language_priority: self
                .language_priority
                .map(LanguagePriority::new)
                .transpose()
                .map_err(command_error)?,
        })
    }
}
