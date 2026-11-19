export type AppSettings = {
  autoExtract?: boolean;
  uiLocale?: import("$lib/i18n").UiLocale;
  libraryRoot: string | null;
  downloadRoot: string | null;
  languagePriority: string[];
  titleLanguageFollowsUi: boolean;
};

export type AppInfo = {
  name: string;
  version: string;
  identifier: string;
  tauriVersion: string;
};

export type Account = {
  id: string;
  label: string;
  loginName: string | null;
  hasCredential: boolean;
  enabled: boolean;
  createdAt: string;
  updatedAt: string;
  lastLoginAt: string | null;
  lastSyncAt: string | null;
};

export type AccountRemovalReport = {
  accountId: string;
  label: string;
  credentialDeleted: boolean;
};

export type ProductOwner = {
  accountId: string;
  label: string;
  purchasedAt: string | null;
};

export type WorkDownloadStatus =
  | "notDownloaded"
  | "downloading"
  | "downloaded"
  | "failed"
  | "cancelled";

export type ProductDownload = {
  status: WorkDownloadStatus;
  localPath: string | null;
  stagingPath: string | null;
  unpackPolicy: string | null;
  bytesReceived: number;
  bytesTotal: number | null;
  errorCode: string | null;
  errorMessage: string | null;
  startedAt: string | null;
  completedAt: string | null;
  updatedAt: string | null;
};

export type ProductCreditGroup = {
  kind: string;
  label: string;
  names: string[];
};

export type ProductTextValue = {
  language: string;
  value: string;
};

export type ProductTag = {
  class: string;
  name: string;
  genreId?: number | null;
};

export type Product = {
  workId: string;
  title: string;
  makerName: string | null;
  workType: string | null;
  ageCategory: string | null;
  thumbnailUrl: string | null;
  contentSizeBytes?: number | null;
  publishedAt: string | null;
  updatedAt: string | null;
  earliestPurchasedAt: string | null;
  latestPurchasedAt: string | null;
  creditGroups: ProductCreditGroup[];
  tags?: ProductTag[];
  download: ProductDownload;
  owners: ProductOwner[];
};

export type ProductListPage = {
  totalCount: number;
  products: Product[];
};

export type ProductFilterFacets = {
  genres?: ProductNameFacet[];
  voices?: ProductNameFacet[];
  makers: ProductMakerFacet[];
};

export type ProductMakerFacet = {
  key: string;
  name: string;
  count: number;
};

export type ProductNameFacet = {
  name: string;
  count: number;
  genreId?: number | null;
};

export type ProductDetail = {
  workId: string;
  title: string;
  titleVariants: ProductTextValue[];
  makerId: string | null;
  makerName: string | null;
  makerNames: ProductTextValue[];
  workType: string | null;
  ageCategory: string | null;
  thumbnailUrl: string | null;
  contentSizeBytes: number | null;
  registeredAt: string | null;
  publishedAt: string | null;
  updatedAt: string | null;
  lastDetailSyncAt: string;
  earliestPurchasedAt: string | null;
  latestPurchasedAt: string | null;
  creditGroups: ProductCreditGroup[];
  tags: ProductTag[];
  download: ProductDownload;
  owners: ProductOwner[];
};

export type BulkWorkDownloadPreview = {
  totalCount: number;
  requestedCount: number;
  skippedDownloadedCount: number;
  skippedQueuedCount: number;
  plannedCount: number;
  failedCount: number;
  knownExpectedBytes: number;
  totalExpectedBytes: number | null;
  unknownSizeCount: number;
};

export type LocalWorkImportReport = {
  recoveryErrors: string[];
  scannedDirectories: number;
  importedCount: number;
  skippedNoId: number;
  skippedAmbiguous: number;
  skippedNonUtf8: number;
  skippedExisting: number;
  metadataCandidateCount: number;
  metadataUpdatedCount: number;
  metadataMissingCount: number;
  metadataError: string | null;
  importedWorks: { workId: string; localPath: string }[];
};

export type BulkDownloadDialog = {
  kind: "confirm" | "notice";
  preview: BulkWorkDownloadPreview;
  space?: import("$lib/api/tauri").SpaceReport;
  selectedProducts?: SelectedProduct[];
};

export type SelectedProduct = Pick<Product, "workId" | "title" | "makerName" | "thumbnailUrl" | "contentSizeBytes">;

export type DownloadWorkProgress = {
  workId: string;
  title: string;
  phase: string;
  bytesReceived: number;
  bytesTotal: number | null;
  bytesPerSecond: number | null;
  rateUpdatedAt: number;
};

export type DownloadMetrics = {
  deferredWorks?: number;
  totalWorks: number;
  completedWorks: number;
  succeededWorks: number;
  failedWorks: number;
  bytesReceived: number;
  activeWorks: DownloadWorkProgress[];
};

export type TwoFactorRequest = {
  requestId: string;
  accountId: string;
  accountLabel: string;
  attempt: number;
  previousCodeRejected: boolean;
  jobId: string;
};

export type TwoFactorClosed = {
  requestId: string;
};

export type ConfirmationDialog = {
  eyebrow: string;
  title: string;
  message: string;
  confirmLabel: string;
  cancelLabel: string;
  tone: "danger" | "default";
};

export type BulkSucceededWork = {
  workId: string;
  localPath: string | null;
  fileCount: number | null;
  archiveExtracted: boolean | null;
};

export type BulkFailedWork = {
  workId: string;
  errorCode: string | null;
  errorMessage: string | null;
};

export type JobStatus = "queued" | "running" | "cancelling" | "succeeded" | "failed" | "cancelled";

export type JobProgress = {
  current: number | null;
  total: number | null;
  unit: string | null;
};

export type JobFailure = {
  code: string | null;
  message: string;
  details: Record<string, unknown>;
};

export type JobSnapshot = {
  id: string;
  kind: string;
  title: string;
  status: JobStatus;
  phase: string | null;
  progress: JobProgress | null;
  progressDetails?: Record<string, unknown>;
  metadata: Record<string, unknown>;
  output: Record<string, unknown> | null;
  error: JobFailure | null;
  cancellable: boolean;
  createdAt: string;
  startedAt: string | null;
  finishedAt: string | null;
};

export type JobEvent = {
  sequence: number;
  eventKind: string;
  jobId: string;
  kind: string;
  status: JobStatus;
  phase: string | null;
  progress: JobProgress | null;
  message: string | null;
  log: { message: string } | null;
  snapshot: JobSnapshot;
};

export type StartJobResponse = {
  jobId: string;
};

export type AuditOutcome = "queued" | "succeeded" | "failed" | "cancelled" | "unknown";

export type AuditLevel = "info" | "warn" | "error";

export type AuditEvent = {
  kind?: string;
  component?: string;
  jobId?: string | null;
  hint?: string | null;
  runId?: string;
  operationId?: string | null;
  parentOperationId?: string | null;
  sequence?: number;
  stage?: string;
  durationMs?: number | null;
  at: string;
  level: AuditLevel;
  operation: string;
  outcome: AuditOutcome;
  message: string;
  errorCode: string | null;
  errorMessage: string | null;
  details: Record<string, unknown>;
};

export type ToastKind = "success" | "error" | "info";

export type Toast = {
  id: string;
  kind: ToastKind;
  message: string;
};

export type ProductCreditField = {
  key: string;
  label: string;
  value: string;
  missing: boolean;
};

export type ProductCreditFieldDefinition = Pick<ProductCreditField, "key" | "label">;

export type ProductImagePreview = {
  url: string;
  title: string;
  workId: string;
};

export type StartWorkDownloadOptions = {
  unpackPolicy?: "keepArchives" | "unpackWhenRecognized";
  replaceExisting?: boolean;
  queuedMessage?: string;
};

export type ChipTooltip = {
  text: string;
  left: number;
  top: number;
};

export type ProductTypeInfo = {
  label: string;
  tone: string;
  tooltip: string;
};

export type ProductTypeCodeDetail = {
  label: string;
  tone: string;
  group: string;
  description: string;
};

export type View = "library" | "downloaded" | "playlists" | "settings";

export type Playlist = { id: string; name: string; workIds: string[] };
