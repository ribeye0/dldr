import { describe, expect, it } from "vitest";
import type { Product } from "$lib/model/types";
import {
  addProducts, restoreSelection, selectedDownloadRequest,
  selectedProduct, toggleProductSelection, selectionSizeLabel,
} from "./product-selection";

function work(id: string, local = false): Product {
  return {
    workId: id, title: `Title ${id}`, makerName: "Maker", thumbnailUrl: null,
    owners: [{ accountId: local ? "__local__" : "account-a", label: "Account", purchasedAt: null }],
  } as Product;
}
const a = work("RJ000001"), b = work("RJ000002"), c = work("RJ000003");

describe("download selection", () => {
  it("keeps earlier pages, deduplicates, and excludes local-only works", () => {
    const firstPage = addProducts([], [a, b]);
    const secondPage = addProducts(firstPage, [b, c, work("RJ000004", true)]);
    expect(secondPage.map((p) => p.workId)).toEqual([a.workId, b.workId, c.workId]);
    expect(firstPage).toHaveLength(2);
  });

  it("selects and deselects a Shift range in either direction without losing hidden selections", () => {
    const hidden = work("RJ999999");
    const selected = toggleProductSelection([selectedProduct(hidden)], a, [a, b, c], c.workId, true);
    expect(selected.map((p) => p.workId)).toEqual([hidden.workId, a.workId, b.workId, c.workId]);
    expect(toggleProductSelection(selected, c, [a, b, c], a.workId, true)).toEqual([selectedProduct(hidden)]);
  });

  it("does not extend a range from an anchor on another page", () => {
    expect(toggleProductSelection([], b, [b, c], a.workId, true)).toEqual([selectedProduct(b)]);
  });

  it("restores a saved selection and drops malformed records and extra fields", () => {
    const saved = JSON.stringify([selectedProduct(a), null, { workId: "invalid" }, { ...selectedProduct(a), password: "discard" }, selectedProduct(b)]);
    expect(restoreSelection(saved)).toEqual([selectedProduct(a), selectedProduct(b)]);
    expect(restoreSelection(null)).toEqual([]);
    expect(() => restoreSelection("{bad json")).toThrow();
    expect(() => restoreSelection("{}")).toThrow();
  });

  it("snapshots only selected IDs without narrowing them to the current page or filters", () => {
    const selection = [selectedProduct(a), selectedProduct(c)];
    const request = selectedDownloadRequest(selection);
    selection.splice(0, 1);
    expect(request.workIds).toEqual([a.workId, c.workId]);
    expect(request.search).toBeNull();
    for (const key of ["accountIds", "typeGroups", "ageCategories", "sourceGroups", "makerKeys"] as const) {
      expect(request[key]).toEqual([]);
    }
    expect(request.skipDownloaded).toBe(true);
    expect(request.unpackPolicy).toBe("unpackWhenRecognized");
    expect(selectedDownloadRequest(selection, "keepArchives").unpackPolicy).toBe("keepArchives");
    expect(() => selectedDownloadRequest([])).toThrow();
  });

  it("restores old saved selections with unknown size and sums known sizes", () => {
    const old = { workId: "RJ000001", title: "Old selection", makerName: null, thumbnailUrl: null };
    expect(restoreSelection(JSON.stringify([old]))[0].contentSizeBytes).toBeNull();
    expect(selectionSizeLabel([{ ...old, contentSizeBytes: 1024 }, { ...old, workId: "RJ000002" }])).toBe("1.00 KiB known · 1 size unknown");
    expect(restoreSelection(JSON.stringify([{ ...old, contentSizeBytes: -1 }]))[0].contentSizeBytes).toBeNull();
  });
});
