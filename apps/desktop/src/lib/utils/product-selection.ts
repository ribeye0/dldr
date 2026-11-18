import { translator, type UiLocale } from "$lib/i18n";
import type { BulkWorkDownloadRequest } from "$lib/api/tauri";
import type { Product, SelectedProduct } from "$lib/model/types";
import { formatBytes } from "$lib/utils/format";

export const SELECTION_STORAGE_KEY = "dldr-selected-works-v1";

export function canSelectProduct(product: Product) {
  return product.owners.some((owner) => owner.accountId !== "__local__");
}

export function selectedProduct(product: Product): SelectedProduct {
  return {
    workId: product.workId,
    title: product.title,
    makerName: product.makerName,
    thumbnailUrl: product.thumbnailUrl,
    contentSizeBytes: product.contentSizeBytes ?? null,
  };
}

export function addProducts(selection: SelectedProduct[], products: Product[]) {
  const entries = new Map(selection.map((item) => [item.workId, item]));
  for (const product of products) {
    if (canSelectProduct(product)) entries.set(product.workId, selectedProduct(product));
  }
  return [...entries.values()];
}

export function toggleProductSelection(
  selection: SelectedProduct[],
  product: Product,
  visible: Product[],
  anchorId: string | null,
  range = false,
) {
  if (!canSelectProduct(product)) return selection;
  const checked = !selection.some((item) => item.workId === product.workId);
  const anchorIndex = visible.findIndex((item) => item.workId === anchorId);
  const targetIndex = visible.findIndex((item) => item.workId === product.workId);
  const affected = range && anchorIndex >= 0 && targetIndex >= 0
    ? visible.slice(Math.min(anchorIndex, targetIndex), Math.max(anchorIndex, targetIndex) + 1)
    : [product];
  if (checked) return addProducts(selection, affected);
  const removed = new Set(affected.map((item) => item.workId));
  return selection.filter((item) => !removed.has(item.workId));
}

export function restoreSelection(value: string | null): SelectedProduct[] {
  if (!value) return [];
  const parsed: unknown = JSON.parse(value);
  if (!Array.isArray(parsed)) throw new Error("Invalid saved selection");
  const entries = new Map<string, SelectedProduct>();
  for (const item of parsed) {
    if (!item || typeof item !== "object" || typeof item.workId !== "string" ||
      !/^(?:RJ|VJ|BJ)\d+$/.test(item.workId) || typeof item.title !== "string" ||
      !(item.makerName === null || typeof item.makerName === "string") ||
      !(item.thumbnailUrl === null || typeof item.thumbnailUrl === "string")) continue;
    entries.set(item.workId, {
      workId: item.workId, title: item.title,
      makerName: item.makerName, thumbnailUrl: item.thumbnailUrl,
      contentSizeBytes: typeof item.contentSizeBytes === "number" && Number.isFinite(item.contentSizeBytes) && item.contentSizeBytes >= 0 ? item.contentSizeBytes : null,
    });
  }
  return [...entries.values()];
}

export function selectionSizeLabel(selection: SelectedProduct[], language: UiLocale = "en") {
  let known = 0, unknown = 0;
  for (const product of selection) {
    if (typeof product.contentSizeBytes === "number" && Number.isFinite(product.contentSizeBytes) && product.contentSizeBytes >= 0) known += product.contentSizeBytes;
    else unknown++;
  }
  const translate = translator(language);
  return unknown ? translate("{0} known · {1} size unknown", formatBytes(known), unknown) : translate("{0} estimated", formatBytes(known));
}

export function selectedDownloadRequest(selection: SelectedProduct[], unpackPolicy: BulkWorkDownloadRequest["unpackPolicy"] = "unpackWhenRecognized"): BulkWorkDownloadRequest {
  if (!selection.length) throw new Error("Select at least one work to download");
  // Selection spans pages and queries. Current Library filters must not narrow it.
  return {
    workIds: [...new Set(selection.map((item) => item.workId))],
    search: null,
    accountIds: [], typeGroups: [], ageCategories: [], sourceGroups: [],
    makerKeys: [],
    sort: "latestPurchaseDesc",
    unpackPolicy,
    skipDownloaded: true,
  };
}
