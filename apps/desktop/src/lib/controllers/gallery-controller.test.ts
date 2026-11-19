import { describe, expect, it, vi } from "vitest";
import type { ProductListPage } from "$lib/model/types";
import type { ProductListRequest } from "$lib/api/tauri";
import { GalleryController } from "./gallery-controller.svelte";
import { FilterHistory, refineSearch, installFilterBackNavigation } from "$lib/utils/filter-history.svelte";

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

describe("metadata drill-down navigation", () => {
  it("returns through nested filters with the original sort, facets, draft and page", async () => {
    const { controller, api } = fixture();
    controller.search = "ASMR"; controller.typeGroups = ["audio"]; controller.ageCategories = ["adult"];
    controller.sourceGroups = ["purchased"]; controller.makerKeys = ["maker-1"];
    controller.metadata.genreNames = ["Binaural"]; controller.sort = "titleAsc";
    controller.filtersOpen = true; controller.pageIndex = 3;
    await controller.load();
    controller.search = "$tag:unfinished";
    expect(await controller.filterBy("circle", "バイコーンの森", { scrollTop: 820, workId: "RJ123456" })).toBe(true);
    expect(api.listProducts.mock.lastCall?.[0]).toMatchObject({ search: "ASMR $circle:バイコーンの森$", offset: 0, typeGroups: ["audio"], genreNames: ["Binaural"] });
    controller.pageIndex = 1;
    expect(await controller.filterBy("va", "小花衣こっこ", { scrollTop: 320 })).toBe(true);
    expect(controller.search).toBe("ASMR $circle:バイコーンの森$ $va:小花衣こっこ$");
    controller.metadata.genreNames.push("later edit"); controller.sort = "titleDesc";
    expect(await controller.back()).toEqual({ scrollTop: 320 });
    expect(controller.request()).toMatchObject({ search: "ASMR $circle:バイコーンの森$", offset: 30, genreNames: ["Binaural"], sort: "titleAsc" });
    expect(await controller.back()).toEqual({ scrollTop: 820, workId: "RJ123456" });
    expect(controller.search).toBe("$tag:unfinished");
    expect(api.listProducts.mock.lastCall?.[0]).toMatchObject({ search: "ASMR", offset: 90, sort: "titleAsc", typeGroups: ["audio"], ageCategories: ["adult"], sourceGroups: ["purchased"], makerKeys: ["maker-1"], genreNames: ["Binaural"] });
    expect(controller.filtersOpen).toBe(true);
    expect(controller.history.entries).toHaveLength(0);
    // Background completion events and page changes must not submit that draft.
    await controller.load({ background: true });
    await controller.goToPage(2);
    expect(api.listProducts.mock.lastCall?.[0].search).toBe("ASMR");
  });

  it("keeps independent return paths for Library and Downloaded", async () => {
    const { controller, api } = fixture();
    await controller.load();
    await controller.filterBy("tag", "Whispering", { scrollTop: 111 });
    controller.setScope("downloaded"); await controller.load();
    expect(controller.history.entries).toHaveLength(0);
    await controller.filterBy("va", "Fixture", { scrollTop: 222 });
    expect(api.listProducts.mock.lastCall?.[0].downloadedOnly).toBe(true);
    expect(await controller.back()).toEqual({ scrollTop: 222 });
    controller.setScope("library"); await controller.load();
    expect(controller.search).toBe("$tag:Whispering$");
    expect(await controller.back()).toEqual({ scrollTop: 111 });
    expect(api.listProducts.mock.lastCall?.[0]).toMatchObject({ downloadedOnly: false, search: null });
  });

  it("does not add duplicate, malformed or busy navigation entries", async () => {
    const { controller, api } = fixture();
    await controller.load();
    await controller.filterBy("tag", "ASMR", { scrollTop: 0 });
    const count = api.listProducts.mock.calls.length;
    expect(await controller.filterBy("tag", "asmr", { scrollTop: 0 })).toBe(false);
    expect(await controller.filterBy("va", "name$", { scrollTop: 0 })).toBe(false);
    expect(controller.history.entries).toHaveLength(1);
    expect(api.listProducts).toHaveBeenCalledTimes(count);
    const pending = deferred<ProductListPage>(); api.listProducts.mockReturnValueOnce(pending.promise);
    const loading = controller.load();
    expect(await controller.filterBy("circle", "Fixture", { scrollTop: 0 })).toBe(false);
    expect(await controller.back()).toBeUndefined();
    pending.resolve({ products: [], totalCount: 100 }); await loading;
    await controller.resetFilters();
    expect(controller.history.entries).toHaveLength(0);
  });

  it("clamps a restored page when works have been removed in the meantime", async () => {
    const { controller, api } = fixture();
    controller.pageIndex = 5; await controller.load();
    await controller.filterBy("tag", "ASMR", { scrollTop: 1000 });
    api.listProducts.mockResolvedValue({ products: [], totalCount: 31 });
    await controller.back();
    expect(controller.pageIndex).toBe(1);
    expect(api.listProducts.mock.lastCall?.[0].offset).toBe(30);
  });

  it("does not restore scroll after a newer query supersedes the drill-down", async () => {
    const { controller, api } = fixture(); await controller.load();
    const old = deferred<ProductListPage>(); api.listProducts.mockReturnValueOnce(old.promise);
    const filtering = controller.filterBy("tag", "ASMR", { scrollTop: 300 });
    controller.setScope("downloaded"); await controller.load();
    old.resolve({ products: [], totalCount: 999 });
    expect(await filtering).toBe(false);
    expect(controller.scope).toBe("downloaded");
    expect(controller.total).toBe(200);
  });

  it("shares immutable history and exact-token rules with playlist navigation", () => {
    const history = new FilterHistory<{ ids: string[]; page: number }>();
    const state = { ids: ["0"], page: 3 };
    history.push(state, { scrollTop: 88 }); state.ids.push("changed");
    expect(history.pop()).toEqual({ state: { ids: ["0"], page: 3 }, position: { scrollTop: 88 } });
    expect(history.pop()).toBeUndefined();
    expect(refineSearch("$tag:ASMR$", "va", "Two Words")).toBe("$tag:ASMR$ $va:Two Words$");
    expect(refineSearch("$tag:unfinished", "tag", "ASMR")).toBeNull();
    expect(refineSearch("", "circle", "\n")).toBeNull();
  });
});

describe("filter Back input", () => {
  function input(target: EventTarget, type: string, values: Record<string, unknown> = {}) {
    const event = Object.assign(new Event(type, { cancelable: true }), values);
    target.dispatchEvent(event);
    return event;
  }

  it("uses Alt+Left and BrowserBack without claiming ordinary arrows or other modifier shortcuts", () => {
    const target = new EventTarget(), back = vi.fn();
    const dispose = installFilterBackNavigation(target as unknown as Window, () => true, back);
    expect(input(target, "keydown", { key: "ArrowLeft", altKey: true }).defaultPrevented).toBe(true);
    input(target, "keydown", { key: "ArrowLeft", altKey: true, repeat: true });
    expect(back).toHaveBeenCalledTimes(1);
    for (const values of [{ key: "ArrowLeft" }, { key: "ArrowRight", altKey: true },
      { key: "ArrowLeft", altKey: true, ctrlKey: true }, { key: "ArrowLeft", altKey: true, metaKey: true },
      { key: "ArrowLeft", altKey: true, shiftKey: true }, { key: "ArrowLeft", altKey: true, isComposing: true }]) {
      expect(input(target, "keydown", values).defaultPrevented).toBe(false);
    }
    input(target, "keydown", { key: "BrowserBack" });
    expect(back).toHaveBeenCalledTimes(2);
    dispose();
    expect(input(target, "keydown", { key: "ArrowLeft", altKey: true }).defaultPrevented).toBe(false);
    expect(back).toHaveBeenCalledTimes(2);
  });

  it("maps a complete mouse Back click to one step while leaving forward, middle and right buttons alone", () => {
    const target = new EventTarget(), back = vi.fn();
    const dispose = installFilterBackNavigation(target as unknown as Window, () => true, back);
    for (const type of ["mousedown", "mouseup", "auxclick"]) expect(input(target, type, { button: 3 }).defaultPrevented).toBe(true);
    expect(back).toHaveBeenCalledTimes(1);
    for (const button of [0, 1, 2, 4]) {
      for (const type of ["mousedown", "mouseup", "auxclick"]) expect(input(target, type, { button }).defaultPrevented).toBe(false);
    }
    input(target, "mousedown", { button: 3 }); input(target, "blur"); input(target, "mouseup", { button: 3 });
    expect(back).toHaveBeenCalledTimes(1);
    dispose();
    expect(input(target, "mousedown", { button: 3 }).defaultPrevented).toBe(false);
  });

  it("consumes Back without leaving the app when unavailable and checks guards on both mouse press and release", () => {
    const target = new EventTarget(), back = vi.fn();
    let available = false;
    const dispose = installFilterBackNavigation(target as unknown as Window, () => available, back);
    expect(input(target, "keydown", { key: "ArrowLeft", altKey: true }).defaultPrevented).toBe(true);
    input(target, "mousedown", { button: 3 }); available = true; input(target, "mouseup", { button: 3 });
    expect(back).not.toHaveBeenCalled();
    input(target, "mousedown", { button: 3 }); available = false; input(target, "mouseup", { button: 3 });
    expect(back).not.toHaveBeenCalled();
    dispose();
  });
});
