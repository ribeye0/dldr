import { describe, expect, it, vi } from "vitest";
import type { ProductListPage } from "$lib/model/types";
import type { ProductListRequest } from "$lib/api/tauri";
import { GalleryController } from "./gallery-controller.svelte";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(yes => { resolve = yes; });
  return { promise, resolve };
}
function fixture() {
  const api = { listProducts: vi.fn(async (_request: ProductListRequest): Promise<ProductListPage> => ({ products: [], totalCount: 200 })), listProductFilterFacets: vi.fn(async () => ({ makers: [] })) };
  const view = { beforeLoad: vi.fn(), page: vi.fn(), error: vi.fn() };
  return { api, view, controller: new GalleryController(view, api) };
}

describe("gallery state boundaries", () => {
  it("keeps independent filters/pages for Library and Downloaded without aliasing their arrays", async () => {
    const { controller, api } = fixture();
    controller.search = "ASMR"; controller.typeGroups = ["audio"]; controller.pageIndex = 3;
    controller.metadata.genreNames = ["Whispering"]; controller.filtersOpen = true;
    expect(controller.setScope("downloaded")).toBe(true);
    expect(controller.search).toBe(""); expect(controller.pageIndex).toBe(0);
    controller.search = "local"; controller.metadata.voiceNames = ["Fixture"];
    await controller.load();
    expect(api.listProducts.mock.calls[0][0]).toMatchObject({ downloadedOnly: true, search: "local", voiceNames: ["Fixture"], limit: 30, offset: 0 });
    controller.setScope("library");
    expect(controller.request()).toMatchObject({ search: "ASMR", downloadedOnly: false, genreNames: ["Whispering"], typeGroups: ["audio"], offset: 90 });
    expect(controller.filtersOpen).toBe(true);
    controller.metadata.voiceNames.push("unrelated");
    controller.setScope("downloaded");
    expect(controller.metadata.voiceNames).toEqual(["Fixture"]);
    expect(controller.search).toBe("local");
  });

  it("discards a previous gallery response before the next scope finishes loading", async () => {
    const { controller, api, view } = fixture();
    const old = deferred<ProductListPage>(); api.listProducts.mockReturnValueOnce(old.promise);
    const loading = controller.load(); controller.setScope("downloaded");
    old.resolve({ products: [], totalCount: 999 }); await loading;
    expect(controller.total).toBe(0); expect(view.page).not.toHaveBeenCalled();
    await controller.load(); expect(controller.total).toBe(200);
  });

  it("centralizes filter changes and rejects invalid page jumps", async () => {
    const { controller, api } = fixture();
    await controller.load(); controller.pageIndex = 4;
    await controller.toggleFilter("typeGroups", "audio");
    expect(controller.pageIndex).toBe(0); expect(controller.typeGroups).toEqual(["audio"]);
    await controller.toggleFilter("typeGroups", "audio"); expect(controller.typeGroups).toEqual([]);
    controller.cycleMetadata("genre", "Whispering"); expect(controller.metadata.genreNames).toEqual(["Whispering"]);
    controller.cycleMetadata("genre", "Whispering"); expect(controller.metadata.excludedGenreNames).toEqual(["Whispering"]);
    controller.cycleMetadata("genre", "Whispering"); expect(controller.metadata.excludedGenreNames).toEqual([]);
    await controller.load(); api.listProducts.mockClear();
    await controller.goToPage(0); await controller.goToPage(1.5); await controller.goToPage(99);
    expect(api.listProducts).not.toHaveBeenCalled();
    await controller.goToPage(3); expect(controller.request().offset).toBe(60);
    controller.dispose(); await controller.load(); expect(api.listProducts).toHaveBeenCalledOnce();
  });
});
