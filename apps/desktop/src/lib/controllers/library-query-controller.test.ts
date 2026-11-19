import { RequestGeneration } from "$lib/utils/request-generation";
import { describe, expect, it, vi } from "vitest";
import { LibraryQueryController } from "./library-query-controller";
import type { ProductListRequest } from "$lib/api/tauri";
import type { ProductListPage } from "$lib/model/types";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
function query(overrides: Partial<ProductListRequest> = {}): ProductListRequest {
  return { search: null, accountIds: [], typeGroups: [], ageCategories: [], sourceGroups: [], makerKeys: [], sort: "latestPurchaseDesc", limit: 100, offset: 0, ...overrides };
}
const page = (totalCount: number): ProductListPage => ({ products: [], totalCount });
function fixture() {
  const port = { listProducts: vi.fn(), listProductFilterFacets: vi.fn().mockResolvedValue({ makers: [] }) };
  const view = { page: vi.fn(), facets: vi.fn(), loading: vi.fn(), error: vi.fn() };
  return { port, view, controller: new LibraryQueryController(port, view) };
}

describe("Library query lifecycle", () => {
  it("keeps the downloaded scope through page clamping and facet refresh", async () => {
    const { controller, port, view } = fixture();
    port.listProducts.mockResolvedValue(page(101));
    await controller.load(query({ downloadedOnly: true, search: "ASMR", offset: 200 }), true);
    expect(port.listProducts.mock.calls.map(([request]) => [request.downloadedOnly, request.offset])).toEqual([[true, 200], [true, 100]]);
    expect(port.listProductFilterFacets).toHaveBeenCalledWith(query({ downloadedOnly: true, search: "ASMR", offset: 100 }));
    expect(view.page).toHaveBeenCalledWith(page(101), 1);
  });
  it("retains last known content when an authoritative terminal-event refresh fails", async () => {
    const { controller, port, view } = fixture();
    port.listProducts.mockResolvedValueOnce(page(1)).mockRejectedValueOnce(new Error("refresh unavailable"));
    await controller.load(query());
    controller.invalidate();
    await controller.load(query(), true);
    expect(view.page).toHaveBeenCalledExactlyOnceWith(page(1), 0);
    expect(view.error).toHaveBeenCalledTimes(1);
    expect(view.loading).toHaveBeenLastCalledWith(false);
  });

  it("clamps an emptied page after a mutation changes filter membership", async () => {
    const { controller, port, view } = fixture();
    port.listProducts.mockResolvedValueOnce(page(0));
    await controller.load(query({ makerKeys: ["removed"], offset: 200 }), true);
    expect(port.listProducts).toHaveBeenCalledTimes(1);
    expect(view.page).toHaveBeenCalledExactlyOnceWith(page(0), 0);
    expect(port.listProductFilterFacets).toHaveBeenCalledWith(query({ makerKeys: ["removed"], offset: 0 }));
  });

  it("updates a background refresh without clearing visible content or facets", async () => {
    const { controller, port, view } = fixture();
    port.listProducts.mockResolvedValueOnce(page(4));
    await controller.load(query());
    vi.clearAllMocks();
    const update = deferred<ProductListPage>();
    const facets = deferred<{ makers: [] }>();
    port.listProducts.mockReturnValueOnce(update.promise);
    port.listProductFilterFacets.mockReturnValueOnce(facets.promise);
    const refresh = controller.load(query(), true, { background: true });
    expect(view.loading).not.toHaveBeenCalled();
    expect(view.page).not.toHaveBeenCalled();
    update.resolve(page(5));
    await Promise.resolve();
    expect(view.page).toHaveBeenCalledExactlyOnceWith(page(5), 0);
    expect(view.facets).not.toHaveBeenCalled();
    facets.resolve({ makers: [] });
    await refresh;
    expect(view.loading).not.toHaveBeenCalledWith(true);
    expect(view.loading).toHaveBeenLastCalledWith(false);
    expect(view.facets).toHaveBeenCalledTimes(1);
  });

  it("preserves facets if a background facet refresh fails", async () => {
    const { controller, port, view } = fixture();
    port.listProducts.mockResolvedValue(page(3));
    port.listProductFilterFacets.mockRejectedValueOnce(new Error("temporary failure"));
    await controller.load(query(), true, { background: true });
    expect(view.facets).not.toHaveBeenCalled();
    expect(view.error).toHaveBeenCalledTimes(1);
    expect(view.loading).not.toHaveBeenCalledWith(true);
  });

  it("keeps a new foreground query safe from an older background response", async () => {
    const { controller, port, view } = fixture();
    const old = deferred<ProductListPage>();
    port.listProducts.mockReturnValueOnce(old.promise).mockResolvedValueOnce(page(8));
    const refresh = controller.load(query(), true, { background: true });
    await controller.load(query({ search: "new" }));
    const loadingCalls = [...view.loading.mock.calls];
    old.resolve(page(2));
    await refresh;
    expect(view.page).toHaveBeenCalledExactlyOnceWith(page(8), 0);
    expect(view.loading.mock.calls).toEqual(loadingCalls);
  });

  for (const order of ["old-first", "new-first"]) {
    it(`rejects stale page/finally after filter change: ${order}`, async () => {
      const { controller, port, view } = fixture();
      const a = deferred<ProductListPage>(), b = deferred<ProductListPage>();
      port.listProducts.mockReturnValueOnce(a.promise).mockReturnValueOnce(b.promise);
      const first = controller.load(query({ search: "old" }));
      const second = controller.load(query({ search: "new" }));
      if (order === "old-first") {
        a.resolve(page(10)); await first;
        expect(view.page).not.toHaveBeenCalled();
        expect(view.loading).not.toHaveBeenCalledWith(false);
      }
      b.resolve(page(20)); await second;
      a.resolve(page(10)); await first;
      expect(view.page).toHaveBeenCalledExactlyOnceWith(page(20), 0);
      expect(view.loading).toHaveBeenLastCalledWith(false);
    });
  }

  it("invalidates old responses and errors after download/tag mutation", async () => {
    const { controller, port, view } = fixture();
    const a = deferred<ProductListPage>();
    port.listProducts.mockReturnValueOnce(a.promise).mockResolvedValueOnce(page(2));
    const first = controller.load(query({}));
    controller.invalidate();
    await controller.load(query({}));
    a.reject(new Error("obsolete query")); await first;
    expect(view.error).not.toHaveBeenCalled();
    expect(view.page).toHaveBeenCalledExactlyOnceWith(page(2), 0);
  });

  it("keeps clamp retry on its immutable request and rejects it after reset", async () => {
    const { controller, port, view } = fixture();
    const clamp = deferred<ProductListPage>();
    port.listProducts.mockResolvedValueOnce(page(2)).mockReturnValueOnce(clamp.promise).mockResolvedValueOnce(page(8));
    const input = query({ search: "old", makerKeys: ["original"], limit: 100, offset: 200 });
    const first = controller.load(input, true);
    input.makerKeys[0] = "mutated";
    await Promise.resolve();
    expect(port.listProducts).toHaveBeenNthCalledWith(2, { ...input, makerKeys: ["original"], offset: 0 });
    await controller.load(query({ search: "reset", offset: 0 }));
    clamp.resolve(page(2)); await first;
    expect(view.page).toHaveBeenCalledExactlyOnceWith(page(8), 0);
  });

  it("ignores stale facets and clears facets on a current failure", async () => {
    const { controller, port, view } = fixture();
    const facets = deferred<{ makers: [] }>();
    port.listProducts.mockResolvedValue(page(2));
    port.listProductFilterFacets.mockReturnValueOnce(facets.promise).mockRejectedValueOnce(new Error("current facets"));
    const first = controller.load(query({ search: "old" }));
    await Promise.resolve();
    await controller.load(query({ search: "new" }));
    facets.resolve({ makers: [] }); await first;
    expect(view.facets).toHaveBeenCalledTimes(2); // each current page clears facets
    expect(view.error).toHaveBeenCalledTimes(1);
  });

  it("invalidates detail requests on close, mutation, or work switching", () => {
    const detail = new RequestGeneration();
    const old = detail.invalidate();
    const next = detail.invalidate();
    expect(detail.current(old)).toBe(false);
    expect(detail.current(next)).toBe(true);
    detail.invalidate();
    expect(detail.current(next)).toBe(false);
  });
});
