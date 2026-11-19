<script lang="ts">
  import { t } from "$lib/i18n";
  import UiButton from "$lib/components/ui/ActionButton.svelte";
  import Icon from "$lib/components/ui/Icon.svelte";
  import Pagination from "$lib/components/ui/Pagination.svelte";
  import type {
    Product,
    ProductFilterFacets,
    JobSnapshot,
  } from "$lib/model/types";
  import MetadataFilterControls from "./MetadataFilters.svelte";
  import type { MetadataFilters } from "$lib/api/tauri";
  import LibraryControls from "./LibraryControls.svelte";
  import LibraryFilters from "./LibraryFilters.svelte";
  import ProductCard from "./ProductCard.svelte";
  import { canSelectProduct } from "$lib/utils/product-selection";

  let {
    metadataFilters = { genreNames: [], excludedGenreNames: [], voiceNames: [], excludedVoiceNames: [] },
    onCycleMetadataFilter,
    downloadedOnly = false,
    creditLoadingWorkIds = [],
    products = [],
    loading = false,
    search = $bindable(""),
    filtersOpen = false,
    facets = { makers: [] },
    sort = "latestPurchaseDesc",
    selectedSources = [],
    selectedAges = [],
    selectedTypes = [],
    selectedMakers = [],
    rangeLabel = $t("0 products"),
    pageNumber = 1,
    pageCount = 1,
    syncDisabled = false,
    bulkDisabled = false,
    bulkLabel = $t("Download Results"),
    selectedWorkIds = [],
    selectedSizeLabel = "",
    selectedDownloadDisabled = false,
    onToggleSelection,
    onSelectPage,
    onClearPage,
    onClearSelection,
    onReviewSelection,
    onDownloadSelected,
    detailLoadingWorkId = null,
    getActiveJob = () => null,
    onSearch,
    onReset,
    onToggleFilters,
    onReload,
    onSync,
    onBulkDownload,
    onSetSort,
    onClearSources,
    onToggleSource,
    onClearAges,
    onToggleAge,
    onClearTypes,
    onToggleType,
    onClearMakers,
    onToggleMaker,
    onGoToPage,
    onOpenDetails,
    onOpenDlsite,
    onOpenOnline,
    onDownload,
    onCopyWorkId,
    onOpenPlayer,
    onOpenFolder,
    onTogglePlaylist,
    onChoosePlaylist,
    defaultPlaylistWorkIds = [],
    playlistBusy = false,
    onTagSearch,
    onPrecisionSearch,
  }: {
    metadataFilters?: MetadataFilters;
    onCycleMetadataFilter?: (kind: "genre"|"voice", name: string) => void;
    downloadedOnly?: boolean;
    creditLoadingWorkIds?: string[];
    products?: Product[];
    loading?: boolean;
    search?: string;
    filtersOpen?: boolean;
    facets?: ProductFilterFacets;
    sort?: string;
    selectedSources?: string[];
    selectedAges?: string[];
    selectedTypes?: string[];
    selectedMakers?: string[];
    rangeLabel?: string;
    pageNumber?: number;
    pageCount?: number;
    syncDisabled?: boolean;
    bulkDisabled?: boolean;
    bulkLabel?: string;
    selectedWorkIds?: string[];
    selectedSizeLabel?: string;
    selectedDownloadDisabled?: boolean;
    onToggleSelection?: (product: Product, range: boolean) => void;
    onSelectPage?: () => void;
    onClearPage?: () => void;
    onClearSelection?: () => void;
    onReviewSelection?: () => void;
    onDownloadSelected?: () => void;
    detailLoadingWorkId?: string | null;
    getActiveJob?: (product: Product) => JobSnapshot | null;
    onSearch: () => void;
    onReset: () => void;
    onToggleFilters: () => void;
    onReload: () => void;
    onSync: () => void;
    onBulkDownload: () => void;
    onSetSort: (value: string) => void;
    onClearSources: () => void;
    onToggleSource: (value: string) => void;
    onClearAges: () => void;
    onToggleAge: (value: string) => void;
    onClearTypes: () => void;
    onToggleType: (value: string) => void;
    onClearMakers: () => void;
    onToggleMaker: (key: string) => void;
    onGoToPage: (page: number) => void | Promise<void>;
    onOpenDetails: (product: Product) => void;
    onOpenDlsite?: (workId: string) => void;
    onOpenOnline?: (product: Product) => void;
    onDownload?: (product: Product) => void | Promise<void>;
    onCopyWorkId: (workId: string) => void;
    onOpenPlayer?: (product: Product) => void;
    onOpenFolder?: (product: Product) => void;
    onTogglePlaylist?: (product: Product) => void;
    onChoosePlaylist?: (product: Product, point:{x:number;y:number;trigger:HTMLButtonElement}) => void;
    defaultPlaylistWorkIds?: string[];
    playlistBusy?: boolean;
    onTagSearch?: (name: string) => void;
    onPrecisionSearch?: (kind: "circle" | "va", name: string) => void;
  } = $props();
  let results = $state<HTMLDivElement>();
  function showResults() {
    results?.focus({ preventScroll: true });
    results?.scrollIntoView({
      block: "start",
      behavior: matchMedia("(prefers-reduced-motion: reduce)").matches
        ? "instant"
        : "smooth",
    });
  }
</script>

<section class="product-area" aria-label={downloadedOnly ? $t("Downloaded works") : $t("Library")}>
  <LibraryControls
    {downloadedOnly}
    {facets}
    bind:search
    {filtersOpen}
    searchDisabled={loading}
    reloadDisabled={loading}
    {syncDisabled}
    {bulkDisabled}
    {bulkLabel}
    {onSearch}
    {onReset}
    {onToggleFilters}
    {onReload}
    {onSync}
    {onBulkDownload}
  />

  {#if downloadedOnly}
    <div class="downloaded-player-hint" role="note">
      <span class="player-hint-icon" aria-hidden="true"><Icon name="play" /></span>
      <div><strong>{$t("Click the blank area of a work card to open the player.")}</strong>
        <p>{$t("Completed downloads and imported local works. Click a tag to search this list.")}</p></div>
    </div>
  {/if}

  {#if filtersOpen}
    <div class="dldr:mb-6 dldr:flex dldr:flex-col dldr:gap-3">
      <LibraryFilters
        {facets}
        {sort}
        {selectedSources}
        {selectedAges}
        {selectedTypes}
        {selectedMakers}
        {onSetSort}
        {onClearSources}
        {onToggleSource}
        {onClearAges}
        {onToggleAge}
        {onClearTypes}
        {onToggleType}
        {onClearMakers}
        {onToggleMaker}
      />
      <MetadataFilterControls {facets} value={metadataFilters} onCycle={onCycleMetadataFilter} />
      <div class="dldr:flex dldr:justify-end">
        <UiButton
          variant="secondary"
          responsiveWidth="auto"
          onclick={showResults}>{$t("Show results ↓")}</UiButton
        >
      </div>
    </div>
  {/if}

  <div bind:this={results} class="list-header" tabindex="-1">
    <span>{rangeLabel}</span>
    {#if onSelectPage}
      <div class="dldr:flex dldr:flex-wrap dldr:gap-2">
        <UiButton size="small" variant="secondary" disabled={loading || !products.some(canSelectProduct)} onclick={onSelectPage}>{$t("Select page")}</UiButton>
        <UiButton size="small" variant="secondary" disabled={loading || !products.some((p) => selectedWorkIds.includes(p.workId))} onclick={onClearPage}>{$t("Deselect page")}</UiButton>
      </div>
    {/if}
    <Pagination page={pageNumber} {pageCount} {loading} label={downloadedOnly ? $t("Downloaded pages") : $t("Library pages")} onChange={onGoToPage} />
  </div>

  {#if loading}
    <div class="empty-state">{$t("Loading")}</div>
  {:else if products.length === 0}
    <div class="empty-state">{downloadedOnly ? $t("No downloaded works match these filters.") : $t("No products")}</div>
  {:else}
    <div class="product-table work-grid" aria-label={downloadedOnly ? $t("Downloaded products") : $t("Cached products")}>
      {#each products as product (product.workId)}
        <ProductCard
          {product}
          creditsLoading={creditLoadingWorkIds.includes(product.workId)}
          activeJob={getActiveJob(product)}
          detailLoading={detailLoadingWorkId === product.workId}
          selected={selectedWorkIds.includes(product.workId)}
          selectionDisabled={!canSelectProduct(product)}
          {onToggleSelection}
          {onOpenDetails}
          {onOpenDlsite}
          {onOpenOnline}
          {onDownload}
          {onCopyWorkId}
          {onOpenPlayer}
          {onOpenFolder}
          {onTogglePlaylist}
          {onChoosePlaylist}
          {playlistBusy}
          inDefaultPlaylist={defaultPlaylistWorkIds.includes(product.workId)}
          {onTagSearch}
          {onPrecisionSearch}
        />
      {/each}
    </div>
  {/if}

  <Pagination page={pageNumber} {pageCount} {loading} bottom scrollTarget={results}
    label={downloadedOnly ? $t("Downloaded pages") : $t("Library pages")} onChange={onGoToPage} />

  {#if selectedWorkIds.length > 0}
    <div class="selection-bar" role="region" aria-label={$t("Download selection")}>
      <div><span aria-live="polite">{selectedWorkIds.length} {$t("selected across all pages")}</span>
        {#if selectedSizeLabel}<p class="dldr:mb-0 dldr:mt-1 dldr:text-xs dldr:text-ui-dim">{selectedSizeLabel}</p>{/if}
      </div>
      <div class="dldr:flex dldr:flex-wrap dldr:gap-2">
        <UiButton variant="secondary" responsiveWidth="auto" onclick={onReviewSelection}>{$t("Review selected")}</UiButton>
        <UiButton variant="secondary" responsiveWidth="auto" onclick={onClearSelection}>{$t("Clear selection")}</UiButton>
        <UiButton responsiveWidth="auto" disabled={selectedDownloadDisabled} onclick={onDownloadSelected}>{$t("Download selected (")}{selectedWorkIds.length})</UiButton>
      </div>
    </div>
  {/if}
</section>

<style>
  .downloaded-player-hint { display:flex; align-items:center; gap:12px; margin:0 0 20px; padding:14px 16px; border:1px solid var(--dldr-color-ui-line-subtle); border-left:4px solid var(--dldr-color-ui-accent); border-radius:12px; background:var(--dldr-color-ui-status-bg); color:var(--dldr-color-ui-ink); line-height:1.6; }
  .player-hint-icon { display:grid; place-items:center; flex:none; width:36px; height:36px; border-radius:50%; background:var(--dldr-color-ui-accent); color:var(--dldr-color-ui-on-accent); }
  .player-hint-icon :global(svg) { width:20px; height:20px; }
  .downloaded-player-hint strong { display:block; font-size:15px; font-weight:700; }
  .downloaded-player-hint p { margin:3px 0 0; font-size:13px; color:var(--dldr-color-ui-dim); }
  .product-area {
    display: flex;
    flex-direction: column;
    gap: 0;
    min-width: 0;
  }
  .list-header {
    margin: 0 0 20px;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    color: var(--muted);
    font-size: 12px;
    outline: none;
  }
  .product-table {
    overflow-anchor: none;
  }
  .empty-state {
    padding: 48px 16px;
    text-align: center;
    color: var(--muted);
  }
  .selection-bar {
    position: sticky;
    bottom: 12px;
    width: 640px;
    max-width: 100%;
    box-sizing: border-box;
    align-self: flex-start;
    z-index: 5;
    margin-top: 24px;
    padding: 12px;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    border: 1px solid var(--dldr-color-ui-selected-border);
    border-radius: 8px;
    background: var(--dldr-color-ui-surface);
    box-shadow: 0 -4px 16px var(--dldr-color-ui-shadow);
    font-size: 13px;
  }
</style>
