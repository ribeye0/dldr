import { translator, type UiLocale } from "$lib/i18n";
import type {
  BulkWorkDownloadPreview,
  ProductTextValue,
  WorkDownloadStatus,
} from "$lib/model/types";

export function bulkDownloadExpectedBytesLabel(preview: BulkWorkDownloadPreview, language: UiLocale = "en") {
  const translate = translator(language);
  if (typeof preview.totalExpectedBytes === "number") {
    return formatBytes(preview.totalExpectedBytes);
  }

  if (preview.knownExpectedBytes > 0) {
    return translate("{0} known, plus {1} unknown file(s)", formatBytes(preview.knownExpectedBytes), preview.unknownSizeCount);
  }

  return preview.unknownSizeCount > 0 ? translate("{0} unknown file(s)", preview.unknownSizeCount) : translate("Unknown");
}

export function formatBytes(value: number) {
  const units = ["B", "KiB", "MiB", "GiB", "TiB"];
  let size = value;
  let unitIndex = 0;

  while (size >= 1024 && unitIndex < units.length - 1) {
    size /= 1024;
    unitIndex += 1;
  }

  const precision = unitIndex === 0 || size >= 100 ? 0 : size >= 10 ? 1 : 2;
  return `${size.toFixed(precision)} ${units[unitIndex]}`;
}

export function textVariantsLabel(values: ProductTextValue[]) {
  return values.map((item) => `${languageLabel(item.language)}: ${item.value}`).join("\n");
}

export function languageLabel(value: string) {
  switch (value) {
    case "en_US":
      return "English";
    case "ja_JP":
      return "Japanese";
    case "ko_KR":
      return "Korean";
    case "zh_CN":
      return "Chinese";
    case "zh_TW":
      return "Taiwanese";
    default:
      return value;
  }
}

export function detailValue(value: string | number | null | undefined) {
  if (value === null || value === undefined || value === "") {
    return "-";
  }

  return String(value);
}

export function detailDate(value: string | null) {
  return value ? shortDate(value) : "-";
}

export function downloadStatusLabel(status: WorkDownloadStatus) {
  switch (status) {
    case "notDownloaded":
      return "Not downloaded";
    case "downloading":
      return "Downloading";
    case "downloaded":
      return "Downloaded";
    case "failed":
      return "Failed";
    case "cancelled":
      return "Cancelled";
  }
}

export function valueOrNull(value: string) {
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

export function shortDate(value: string | null) {
  if (!value) {
    return "";
  }

  return value.replace("T", " ").replace(/\.\d+Z$/, "Z");
}

export function errorMessage(err: unknown) {
  return err instanceof Error ? err.message : String(err);
}

export function appInfoValue(value: string | undefined, loading: boolean) {
  if (value) {
    return value;
  }

  return loading ? "Loading" : "Unavailable";
}
