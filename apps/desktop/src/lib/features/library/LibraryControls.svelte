<script lang="ts">
  import { t } from "$lib/i18n";
  import PageHeader from "$lib/components/layout/PageHeader.svelte";
  import PageToolbar from "$lib/components/layout/PageToolbar.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import Icon from "$lib/components/ui/Icon.svelte";
  import SearchField from "$lib/components/ui/SearchField.svelte";
  import { parseProductSearch, productSearchCompletions, removeSearchTerm, searchLabels, type SearchTerm } from "$lib/utils/product-search";
  import { genreName, genreSearchNames } from "$lib/i18n/genres";
  import type { ProductFilterFacets } from "$lib/model/types";

  let syntaxOpen = $state(false);

  let {
    downloadedOnly = false,
    search = $bindable(""),
    facets = { makers: [] },
    filtersOpen = false,
    searchDisabled = false,
    reloadDisabled = false,
    syncDisabled = false,
    bulkDisabled = false,
    bulkLabel = $t("Download Results"),
    onSearch,
    onReset,
    onToggleFilters,
    onReload,
    onSync,
    onBulkDownload,
  }: {
    downloadedOnly?: boolean;
    search?: string;
    facets?: ProductFilterFacets;
    filtersOpen?: boolean;
    searchDisabled?: boolean;
    reloadDisabled?: boolean;
    syncDisabled?: boolean;
    bulkDisabled?: boolean;
    bulkLabel?: string;
    onSearch?: () => void;
    onReset?: () => void;
    onToggleFilters?: () => void;
    onReload?: () => void;
    onSync?: () => void;
    onBulkDownload?: () => void;
  } = $props();
  const parsed = $derived(parseProductSearch(search));
  const conditions = $derived(parsed.terms.filter(term => term.kind !== "text"));
  const searchValues = $derived([
    ...facets.makers.map(item => ({ kind: "circle" as const, name: item.name })),
    ...(facets.voices ?? []).map(item => ({ kind: "va" as const, name: item.name })),
    ...(facets.genres ?? []).map(item => ({ kind: "tag" as const, name: $genreName(item.name, item.genreId), aliases: genreSearchNames(item.name, item.genreId) })),
  ]);
  function suggest(value: string, caret: number) {
    return productSearchCompletions(value, caret, searchValues).map(item => ({ ...item,
      description: item.command ? $t({ circle: "社團名稱精準比對", va: "聲優名稱精準比對", tag: "包含分類，可用已收錄的中文／日文別名", "-tag": "排除分類" }[item.kind]) : $t(searchLabels[item.kind]),
    }));
  }

  function submitSearch(event: SubmitEvent) {
    event.preventDefault();
    if (!parsed.error) onSearch?.();
  }
  function removeCondition(term: SearchTerm) {
    search = removeSearchTerm(search,term);
    if (!parseProductSearch(search).error) onSearch?.();
  }
</script>

<div class="library-controls">
  <PageHeader title={downloadedOnly ? $t("Downloaded") : $t("Library")}>
    {#if !downloadedOnly}
      <Button variant="primary" disabled={syncDisabled} onclick={onSync}
        ><Icon name="refresh" />{$t("Sync")}</Button
      >
    {/if}
    {#snippet toolbar()}
      <PageToolbar label={downloadedOnly ? $t("Downloaded tools") : $t("Library tools")} {onReload} {reloadDisabled}>
        <form
          onsubmit={submitSearch}
          class="dldr:flex dldr:min-w-0 dldr:flex-1 dldr:flex-wrap dldr:items-center dldr:gap-2"
        >
          <div class="dldr:flex dldr:min-w-0 dldr:basis-48 dldr:grow">
            <SearchField
              bind:value={search}
              label={downloadedOnly ? $t("Search downloaded works") : $t("Search library")}
              placeholder={$t("Search keywords, or type $ for exact conditions")}
              {suggest}
              clearable
              disabled={searchDisabled}
              invalid={!!parsed.error}
              describedBy={parsed.error ? "product-search-error" : syntaxOpen ? "search-syntax-help" : undefined}
              onClear={onSearch}
            />
          </div>
          <Button type="submit" variant="primary" disabled={searchDisabled || !!parsed.error}>{$t("Search")}</Button>
          <Button variant="text" aria-expanded={syntaxOpen} aria-controls="search-syntax-help" onclick={() => syntaxOpen = !syntaxOpen}>{$t("搜尋語法")}</Button>
          <Button
            aria-expanded={filtersOpen}
            aria-controls="library-filter-grid"
            onclick={onToggleFilters}><Icon name="filter" />{$t("Filters")}</Button
          >
          <Button variant="text" onclick={onReset}>{$t("Reset filters")}</Button>
        </form>
        {#snippet actions()}
          {#if !downloadedOnly}
            <Button disabled={bulkDisabled || !!parsed.error} onclick={onBulkDownload}>
              <Icon name="downloads" />{$t(bulkLabel)}</Button
            >
          {/if}
        {/snippet}
      </PageToolbar>
    {/snippet}
  </PageHeader>
  {#if parsed.error}<p id="product-search-error" class="search-error" role="alert">{$t(parsed.error)}</p>{/if}
  {#if conditions.length}
    <div class="search-conditions" aria-label={$t("精準搜尋條件")}>
      {#each conditions as term}
        <button type="button" class="condition dldr:ui-focus-control" onclick={() => removeCondition(term)} aria-label={$t("移除{0}條件：{1}", $t(searchLabels[term.kind]), term.value)}>
          <span class="condition-kind">{$t(searchLabels[term.kind])}</span><span>{term.value}</span><Icon name="close" />
        </button>
      {/each}
      <span class="search-note">{$t("所有條件需同時符合；按 Search 或 Enter 套用。")}</span>
    </div>
  {/if}
  {#if syntaxOpen}
    <section id="search-syntax-help" class="syntax-help" aria-label={$t("搜尋語法說明")}>
      <p>{$t("直接輸入作品名、社團、聲優或分類；空格分隔的關鍵字需全部符合。用雙引號保留完整片語，例如")} <code>"A Long Evening"</code>。</p>
      <dl>
        <div><dt><code>{'$circle:バイコーンの森$'}</code></dt><dd>{$t("社團名稱精準比對")}</dd></div>
        <div><dt><code>{'$va:小花衣こっこ$'}</code></dt><dd>{$t("聲優名稱精準比對")}</dd></div>
        <div><dt><code>{'$tag:女性主导$'}</code></dt><dd>{$t("包含分類，可用已收錄的中文／日文別名")}</dd></div>
        <div><dt><code>{'$-tag:NTR$'}</code></dt><dd>{$t("排除分類")}</dd></div>
      </dl>
      <p>{$t("可混用文字和多個精準條件。重複聲優條件代表每位聲優都要符合。其他篩選仍可從 Filters 開啟。")}</p>
      <p>{$t("搜尋使用已快取的作品資訊；沒有聲優資料的作品無法由聲優條件命中。")}</p>
      <p>{$t("Type $ to complete syntax; names come from the current cached filters. Choosing a suggestion does not run the search.")}</p>
    </section>
  {/if}
</div>

<style>
  .search-error { margin: -4px 0 18px; color: var(--dldr-color-ui-error); font-size: 13px; }
  .search-conditions { display: flex; align-items: center; flex-wrap: wrap; gap: 8px; margin: -4px 0 20px; }
  .condition { display: inline-flex; align-items: center; gap: 8px; max-width: 100%; padding: 6px 9px; border: 1px solid var(--dldr-color-ui-selected-border); border-radius: 6px; background: var(--dldr-color-ui-selected); color: var(--dldr-color-ui-selected-text); font: inherit; font-size: 12px; cursor: pointer; overflow-wrap: anywhere; }
  .condition-kind { flex-shrink: 0; font-weight: 600; }
  .condition :global(svg) { flex-shrink: 0; }
  .search-note { font-size: 11px; color: var(--dldr-color-ui-dim); }
  .syntax-help { padding: 16px; margin: -4px 0 20px; border: 1px solid var(--dldr-color-ui-line-subtle); border-radius: 8px; background: var(--dldr-color-ui-surface); font-size: 12px; line-height: 1.7; }
  .syntax-help p { margin: 0; color: var(--dldr-color-ui-dim); }
  dl { display: grid; gap: 7px; margin: 12px 0; }
  dl div { display: flex; flex-wrap: wrap; gap: 6px 20px; }
  dt { min-width: 220px; }
  dd { margin: 0; }
  code { overflow-wrap: anywhere; }
</style>
