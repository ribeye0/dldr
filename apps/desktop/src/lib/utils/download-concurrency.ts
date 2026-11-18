export const DOWNLOAD_CONCURRENCY_KEY = "dldr-download-concurrency-v1";
export const DEFAULT_DOWNLOAD_CONCURRENCY = 2;
export const MAX_DOWNLOAD_CONCURRENCY = 4;

export function downloadConcurrency(value: unknown) {
  const parsed = typeof value === "string" && value.trim() ? Number(value) : value;
  return typeof parsed === "number" && Number.isInteger(parsed) && parsed >= 1 && parsed <= MAX_DOWNLOAD_CONCURRENCY
    ? parsed : DEFAULT_DOWNLOAD_CONCURRENCY;
}
