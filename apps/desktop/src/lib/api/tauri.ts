import { diagnosticInvoke as invoke } from "./diagnostics";
import type {
  Account,
  AccountRemovalReport,
  AppSettings,
  AuditEvent,
  BulkWorkDownloadPreview,
  JobSnapshot,
  ProductDetail,
  ProductDownload,
  ProductFilterFacets,
  ProductListPage,
  Playlist,
  StartJobResponse,
} from "$lib/model/types";

export type SaveSettingsRequest = {
  autoExtract?: boolean;
  uiLocale?: import("$lib/i18n").UiLocale;
  libraryRoot: string | null;
  downloadRoot: string | null;
  languagePriority?: string[];
};

export type SaveAccountRequest = {
  id: string | null;
  label: string;
  loginName: string | null;
  password: string | null;
};

export type MetadataFilters = { genreNames: string[]; excludedGenreNames: string[]; voiceNames: string[]; excludedVoiceNames: string[]; };

export type ProductListRequest = Partial<MetadataFilters> & {
  playlistId?: string;
  downloadedOnly?: boolean;
  search: string | null;
  accountIds: string[];
  typeGroups: string[];
  ageCategories: string[];
  sourceGroups: string[];
  makerKeys: string[];
  sort: string;
  limit: number;
  offset: number;
};

export type BulkWorkDownloadRequest = Omit<
  ProductListRequest,
  "limit" | "offset" | "downloadedOnly" | "playlistId"
> & {
  workIds?: string[];
  maxConcurrentWorks?: number;
  unpackPolicy: "keepArchives" | "unpackWhenRecognized";
  skipDownloaded: boolean;
};

export type StartWorkDownloadRequest = {
  workId: string;
  accountId: string | null;
  password: string | null;
  unpackPolicy: "keepArchives" | "unpackWhenRecognized";
  replaceExisting: boolean;
};

export type CancelJobResult = {
  outcome: "requested" | "alreadyFinished";
  snapshot: JobSnapshot;
};

export type ClearFinishedJobsResult = {
  removedCount: number;
};

export function getSettings() {
  return invoke<AppSettings>("get_settings");
}

export const getSetupFolders = () => invoke<{ libraryRoot: string; downloadRoot: string }>("get_setup_folders");
export const configureDownloadFolders = (libraryRoot: string, downloadRoot: string | null, uiLocale: import("$lib/i18n").UiLocale) =>
  invoke<AppSettings>("configure_download_folders", { libraryRoot, downloadRoot, uiLocale });

export function listNameLanguages() {
  return invoke<string[]>("list_name_languages");
}

export function saveSettings(settings: SaveSettingsRequest) {
  return invoke<AppSettings>("save_settings", { settings });
}

export function listAccounts() {
  return invoke<Account[]>("list_accounts");
}

export function saveAccount(request: SaveAccountRequest) {
  return invoke<Account>("save_account", { request });
}

export function setAccountEnabled(accountId: string, enabled: boolean) {
  return invoke<void>("set_account_enabled", {
    request: { accountId, enabled },
  });
}

export function removeAccount(accountId: string) {
  return invoke<AccountRemovalReport>("remove_account", {
    request: { accountId },
  });
}

export function listProducts(request: ProductListRequest) {
  return invoke<ProductListPage>("list_products", { request });
}

export type PlaylistEdit = {action:"create";name:string} | {action:"rename";playlistId:string;name:string} | {action:"add"|"remove";playlistId:string;workId:string};
export const listPlaylists = () => invoke<Playlist[]>("list_playlists");
export const editPlaylist = (request: PlaylistEdit) => invoke<{playlists:Playlist[];playlistId:string}>("edit_playlist",{request});

export function listProductFilterFacets(request: ProductListRequest) {
  return invoke<ProductFilterFacets>("list_product_filter_facets", { request });
}

export function getProductDetail(workId: string) {
  return invoke<ProductDetail>("get_product_detail", { request: { workId } });
}

export function getProductPageContent(workId: string, locale: import("$lib/i18n").UiLocale) {
  return invoke<string>("get_product_page_content", { workId, locale });
}

export function startAccountSync(accountId: string, password: string | null) {
  return invoke<StartJobResponse>("start_account_sync", {
    request: { accountId, password },
  });
}

export function startWorkDownload(request: StartWorkDownloadRequest) {
  return invoke<StartJobResponse>("start_work_download", { request });
}

export function previewBulkWorkDownload(request: BulkWorkDownloadRequest) {
  return invoke<BulkWorkDownloadPreview>("preview_bulk_work_download", {
    request,
  });
}

export function startBulkWorkDownload(request: BulkWorkDownloadRequest) {
  return invoke<StartJobResponse>("start_bulk_work_download", { request });
}

export function openWorkDownload(workId: string) {
  return invoke<void>("open_work_download", { request: { workId } });
}

export function deleteWorkDownload(workId: string) {
  return invoke<ProductDownload>("delete_work_download", {
    request: { workId },
  });
}

export function markWorkDownloaded(workId: string, localPath: string) {
  return invoke<ProductDownload>("mark_work_downloaded", {
    request: { workId, localPath },
  });
}

export function listJobs() {
  return invoke<JobSnapshot[]>("list_jobs");
}

export function cancelJob(jobId: string) {
  return invoke<CancelJobResult>("cancel_job", { request: { jobId } });
}

export function clearFinishedJobs() {
  return invoke<ClearFinishedJobsResult>("clear_finished_jobs");
}

export function listAuditEvents(limit: number, runId?: string) {
  return invoke<AuditEvent[]>("list_audit_events", { request: { limit, runId } });
}

export function getAuditLogDir() {
  return invoke<{ path: string }>("get_audit_log_dir");
}

export function openAuditLogDir() {
  return invoke<void>("open_audit_log_dir");
}

export function openDownloadFolder() {
  return invoke<void>("open_download_folder");
}

export function getDownloadRateLimit() {
  return invoke<{ bytesPerSecond: number | null }>("get_download_rate_limit");
}

export function setDownloadRateLimit(bytesPerSecond: number | null) {
  return invoke<{ bytesPerSecond: number | null }>("set_download_rate_limit", { request: { bytesPerSecond } });
}

export function submitTwoFactorCode(requestId: string, code: string) {
  return invoke<void>("submit_two_factor_code", {
    request: { requestId, code },
  });
}

export function cancelTwoFactor(requestId: string) {
  return invoke<void>("cancel_two_factor", { request: { requestId } });
}

export const saveProductCover = (workId: string) =>
  invoke<boolean>("save_product_cover", { workId });

export type DownloadQueueEntry = { workId: string; title: string; thumbnailUrl: string | null; batchId: string; position: number; accountId: string|null; libraryRoot: string; downloadRoot: string; unpackPolicy: string; replaceExisting: boolean; status: string; errorMessage: string|null; };
export type DiskBudget = { path: string; availableBytes: number; requiredBytes: number; reservedBytes: number; sufficient: boolean; };
export type SpaceReport = { budgets: DiskBudget[]; unknownCount: number; };
export type VerificationReport = { checked: number; present: number; missing: number; issues: { workId: string; path: string; status: string; }[]; };
export const getTransferSettings = () => invoke<number>("get_transfer_settings");
export const setTransferConcurrency = (limit: number) => invoke<void>("set_transfer_concurrency",{limit});
export const listDownloadQueue = () => invoke<DownloadQueueEntry[]>("list_download_queue");
export const controlDownloadQueue = (workId: string,action: string) => invoke<void>("control_download_queue",{workId,action});
export const retryFailedDownloads = () => invoke<number>("retry_failed_downloads");
export const clearDownloadQueue = () => invoke<void>("clear_download_queue");
export const checkDownloadSpace = (request: BulkWorkDownloadRequest) => invoke<SpaceReport>("check_download_space",{request});
export const verifyLibrary = () => invoke<VerificationReport>("verify_library");
export const exportLibraryBackup = (path: string) => invoke<number>("export_library_backup",{path});
export const restoreLibraryBackup = (path: string) => invoke<number>("restore_library_backup",{path});
