import * as commands from "$lib/api/tauri";
import type { Product, ProductFilterFacets, ProductListPage } from "$lib/model/types";
import { PRODUCT_PAGE_SIZE } from "$lib/model/constants";
import { valueOrNull } from "$lib/utils/format";
import type { translator } from "$lib/i18n";
import { LibraryQueryController } from "./library-query-controller";

type Scope = "library" | "downloaded";
type Filter = "typeGroups" | "ageCategories" | "sourceGroups" | "makerKeys";
export type ProductLoadOptions = { resetPage?: boolean; clampInvalidPage?: boolean; background?: boolean };
const emptyMetadata = (): commands.MetadataFilters => ({ genreNames: [], excludedGenreNames: [], voiceNames: [], excludedVoiceNames: [] });
type GalleryPort = Pick<typeof commands, "listProducts" | "listProductFilterFacets">;

/** Query state belongs to its gallery, independent of selections and navigation. */
export class GalleryController {
  products = $state<Product[]>([]);
  total = $state(0);
  loading = $state(true);
  pageIndex = $state(0);
  scope = $state<Scope>("library");
  search = $state("");
  sort = $state("latestPurchaseDesc");
  typeGroups = $state<string[]>([]);
  ageCategories = $state<string[]>([]);
  sourceGroups = $state<string[]>([]);
  makerKeys = $state<string[]>([]);
  metadata = $state(emptyMetadata());
  filtersOpen = $state(false);
  facets = $state<ProductFilterFacets>({ makers: [] });
  pageCount = $derived(Math.max(1, Math.ceil(this.total / PRODUCT_PAGE_SIZE)));
  private saved: Partial<Record<Scope, { request: commands.ProductListRequest; filtersOpen: boolean }>> = {};
  private requests: LibraryQueryController;
  private disposed = false;

  constructor(private readonly view: {
    beforeLoad(): void;
    page(page: ProductListPage): void;
    error(error: unknown): void;
  }, api: GalleryPort = {
    listProducts: commands.listProducts,
    listProductFilterFacets: commands.listProductFilterFacets,
  }) {
    this.requests = new LibraryQueryController(api, {
      loading: value => { this.loading = value; },
      facets: value => { this.facets = value; },
      page: (page, index) => {
        this.products = page.products; this.total = page.totalCount; this.pageIndex = index;
        this.view.page(page);
      },
      error: error => this.view.error(error),
    });
  }

  request(): commands.ProductListRequest {
    return {
      ...this.metadata, downloadedOnly: this.scope === "downloaded", search: valueOrNull(this.search),
      accountIds: [], typeGroups: this.typeGroups, ageCategories: this.ageCategories,
      sourceGroups: this.sourceGroups, makerKeys: this.makerKeys, sort: this.sort,
      limit: PRODUCT_PAGE_SIZE, offset: this.pageIndex * PRODUCT_PAGE_SIZE,
    };
  }

  load = async (options: ProductLoadOptions = {}) => {
    if (this.disposed) return;
    this.view.beforeLoad();
    if (options.resetPage) this.pageIndex = 0;
    await this.requests.load(this.request(), options.clampInvalidPage, { background: options.background });
  };

  invalidate() { this.requests.invalidate(); }
  dispose() { this.disposed = true; this.invalidate(); }

  setScope(scope: Scope) {
    if (scope === this.scope) return false;
    this.invalidate();
    // Query values may be Svelte proxies. Save independent arrays before switching.
    this.saved[this.scope] = { request: JSON.parse(JSON.stringify(this.request())), filtersOpen: this.filtersOpen };
    const saved = this.saved[scope];
    const query = saved?.request;
    this.scope = scope;
    this.metadata = { genreNames: query?.genreNames ?? [], excludedGenreNames: query?.excludedGenreNames ?? [], voiceNames: query?.voiceNames ?? [], excludedVoiceNames: query?.excludedVoiceNames ?? [] };
    this.search = query?.search ?? "";
    this.typeGroups = query?.typeGroups ?? [];
    this.ageCategories = query?.ageCategories ?? [];
    this.sourceGroups = query?.sourceGroups ?? [];
    this.makerKeys = query?.makerKeys ?? [];
    this.sort = query?.sort ?? "latestPurchaseDesc";
    this.pageIndex = Math.floor((query?.offset ?? 0) / PRODUCT_PAGE_SIZE);
    this.filtersOpen = saved?.filtersOpen ?? false;
    this.products = []; this.total = 0; this.facets = { makers: [] };
    return true;
  }

  async toggleFilter(field: Filter, value: string) {
    this[field] = this[field].includes(value) ? this[field].filter(item => item !== value) : [...this[field], value];
    await this.load({ resetPage: true });
  }

  async clearFilter(field: Filter) { this[field] = []; await this.load({ resetPage: true }); }

  cycleMetadata = (kind: "genre" | "voice", name: string) => {
    const include = kind === "genre" ? "genreNames" : "voiceNames";
    const exclude = kind === "genre" ? "excludedGenreNames" : "excludedVoiceNames";
    if (this.metadata[include].includes(name)) {
      this.metadata[include] = this.metadata[include].filter(value => value !== name);
      this.metadata[exclude] = [...this.metadata[exclude], name];
    } else if (this.metadata[exclude].includes(name)) {
      this.metadata[exclude] = this.metadata[exclude].filter(value => value !== name);
    } else this.metadata[include] = [...this.metadata[include], name];
    void this.load({ resetPage: true });
  };

  setSort = async (sort: string) => { this.sort = sort; await this.load({ resetPage: true }); };
  resetFilters = async () => {
    this.metadata = emptyMetadata(); this.search = "";
    this.typeGroups = []; this.ageCategories = []; this.sourceGroups = []; this.makerKeys = [];
    this.sort = "latestPurchaseDesc";
    await this.load({ resetPage: true });
  };

  goToPage = async (page: number) => {
    if (this.loading || !Number.isInteger(page) || page < 1 || page > this.pageCount) return;
    this.pageIndex = page - 1;
    await this.load({ clampInvalidPage: true });
  };

  rangeLabel(translate: ReturnType<typeof translator>) {
    const total = translate(this.total === 1 ? "{0} work" : "{0} works", this.total);
    if (this.loading) return this.total > 0 ? translate("Loading {0}", total) : translate("Loading products");
    if (!this.total || !this.products.length) return total;
    const offset = this.pageIndex * PRODUCT_PAGE_SIZE;
    return translate("{0}-{1} of {2}", Math.min(offset + 1, this.total), Math.min(offset + this.products.length, this.total), this.total);
  }
}
