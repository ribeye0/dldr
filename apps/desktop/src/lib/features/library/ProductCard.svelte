<script lang="ts">
  import { genreName } from "$lib/i18n/genres";
  import { t, locale } from "$lib/i18n";
  import Icon from "$lib/components/ui/Icon.svelte";
  import Badges from "$lib/components/ui/WorkBadges.svelte";
  import { jobLabel } from "$lib/utils/jobs";
  import { formatBytes } from "$lib/utils/format";
  import { dlsiteProductPageUrl, productGenreTags } from "$lib/utils/products";
  import type { Product, JobSnapshot } from "$lib/model/types";
  let {
    product,
    activeJob = null,
    detailLoading = false,
    creditsLoading = false,
    selected = false,
    selectionDisabled = false,
    selectionForFiles = false,
    onToggleSelection,
    onOpenDetails,
    onOpenDlsite,
    onOpenOnline,
    onDownload,
    onCopyWorkId,
    onOpenPlayer,
    onOpenFolder,
    onTogglePlaylist,
    onChoosePlaylist,
    inDefaultPlaylist = false,
    playlistBusy = false,
    onTagSearch,
    onPrecisionSearch,
  }: {
    product: Product;
    activeJob?: JobSnapshot | null;
    detailLoading?: boolean;
    creditsLoading?: boolean;
    selected?: boolean;
    selectionDisabled?: boolean;
    selectionForFiles?: boolean;
    onToggleSelection?: (product: Product, range: boolean) => void;
    onOpenDetails: (product: Product) => void;
    onOpenDlsite?: (workId: string) => void;
    onOpenOnline?: (product: Product) => void;
    onDownload?: (product: Product) => void | Promise<void>;
    onCopyWorkId: (workId: string) => void;
    onOpenPlayer?: (product: Product) => void;
    onOpenFolder?: (product: Product) => void;
    onTogglePlaylist?: (product: Product) => void;
    onChoosePlaylist?: (product: Product, point: {x:number;y:number;trigger:HTMLButtonElement}) => void;
    inDefaultPlaylist?: boolean;
    playlistBusy?: boolean;
    onTagSearch?: (name: string) => void;
    onPrecisionSearch?: (kind: "circle" | "va", name: string) => void;
  } = $props();
  let downloadPending = $state(false);
  const downloaded = $derived(product.download.status === "downloaded");
  const purchased = $derived(product.owners.some(owner => owner.accountId !== "__local__"));
  const downloadDisabled = $derived(!onDownload || !purchased || downloadPending || Boolean(activeJob) || product.download.status === "downloading");
  async function download() {
    if (downloadDisabled) return;
    downloadPending = true;
    try { await onDownload?.(product); }
    finally { downloadPending = false; }
  }
  const available = $derived(product.download.status === "downloaded" && Boolean(product.download.localPath?.trim()));
  const playable = $derived(Boolean(onOpenPlayer && available));
  function choosePlaylist(event: MouseEvent | KeyboardEvent) {
    event.preventDefault();
    const trigger = event.currentTarget as HTMLButtonElement;
    const rect = trigger.getBoundingClientRect();
    onChoosePlaylist?.(product, {x:event instanceof MouseEvent ? event.clientX : rect.right,y:event instanceof MouseEvent ? event.clientY : rect.bottom,trigger});
  }
  const voiceNames = $derived([...new Set(product.creditGroups.filter(group => group.kind === "voice").flatMap(group => group.names))]);
  const releaseDate = $derived(product.publishedAt?.match(/^\d{4}-\d{2}-\d{2}/)?.[0] ?? null);
  const status = $derived(
    activeJob
      ? activeJob.status === "queued"
        ? { icon: "clock", label: $t("Queued") }
        : activeJob.status === "cancelling"
          ? { icon: "downloads", label: $t("Cancelling") }
          : activeJob.phase === "unpacking"
            ? { icon: "unpack", label: $t("Unpacking") }
            : { icon: "downloads", label: jobLabel(activeJob, $locale) }
      : product.download.status === "downloading"
          ? { icon: "downloads", label: $t("Downloading") }
          : null,
  );
</script>

<article class="product-card dldr:relative dldr:min-w-0" class:selected data-work-id={product.workId}>
  {#if playable}
    <button type="button" class="card-play-target dldr:ui-focus-image"
      aria-label={$t("Open player for {0}", product.title)} title={$t("Open player for {0}", product.title)}
      onclick={() => onOpenPlayer?.(product)}></button>
  {/if}
  {#if onToggleSelection}
    <label class="selection-toggle" class:checked={selected} class:unavailable={selectionDisabled}
      title={selectionForFiles ? $t("Select downloaded work; Shift-click to select a range") : selectionDisabled ? $t("Only purchased works can be selected") : $t("Select for download; Shift-click to select a range")}>
    <input
      type="checkbox"
      class="selection-checkbox"
      aria-label={$t("Select {0}: {1}", product.workId, product.title)}
      checked={selected}
      disabled={selectionDisabled}
      onclick={(event) => onToggleSelection(product, event.shiftKey)}
    />
    <span class="selection-mark" aria-hidden="true">{selected ? "✓" : ""}</span>
    </label>
  {/if}
  <div class="card-content" class:playable>
  <div class="cover-frame">
  <button
    type="button"
    class="dldr:relative dldr:block dldr:aspect-[3/2] dldr:w-full dldr:overflow-hidden dldr:rounded-lg dldr:border-0 dldr:bg-ui-elevated dldr:p-0 dldr:text-ui-ink dldr:cursor-pointer dldr:ui-focus-image"
    disabled={detailLoading}
    aria-label={$t("Details: {0}", product.title)}
    onclick={() => onOpenDetails(product)}
  >
    {#if product.thumbnailUrl}<img
        src={product.thumbnailUrl}
        alt=""
        loading="lazy"
        class="dldr:block dldr:h-full dldr:w-full dldr:object-cover"
      />{:else}<span class="dldr:text-sm dldr:text-ui-dim">{$t("No cover image")}</span
      >{/if}
    {#if status && !available}<span
        role="img"
        aria-label={$t(status.label)}
        title={$t(status.label)}
        class="dldr:absolute dldr:top-2 dldr:right-2 dldr:grid dldr:size-7 dldr:place-items-center dldr:rounded-full dldr:bg-ui-status-bg dldr:text-ui-accent"
        style:right={onTogglePlaylist ? "44px" : "8px"}
        ><Icon name={status.icon} /></span
      >{/if}
    {#if detailLoading}<span
        class="dldr:absolute dldr:inset-0 dldr:grid dldr:place-items-center dldr:bg-ui-backdrop dldr:text-brand-white"
        >{$t("Loading…")}</span
      >{/if}
  </button>
  {#if onTogglePlaylist}
    <button type="button" class="playlist-toggle dldr:ui-focus-control" disabled={playlistBusy}
      aria-label={$t(inDefaultPlaylist ? "Remove {0} from default playlist" : "Add {0} to default playlist",product.workId)} aria-pressed={inDefaultPlaylist}
      aria-haspopup="menu" title={$t("Click to toggle the default playlist; right-click to choose a playlist")}
      onclick={() => onTogglePlaylist?.(product)} oncontextmenu={choosePlaylist}
      onkeydown={(event) => { if (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) choosePlaylist(event); }}>
      <Icon name={inDefaultPlaylist ? "check" : "playlists"} />
    </button>
  {/if}
  <div class="cover-labels">
    <span><Badges workType={product.workType} ageCategory={product.ageCategory} /></span>
    <button type="button" class="work-id dldr:ui-focus-control"
      aria-label={`${$t("Copy work ID")}: ${product.workId}`} title={$t("Copy work ID")}
      onclick={() => onCopyWorkId(product.workId)}>{product.workId}</button>
  </div>
  </div>
  <button
    type="button"
    disabled={detailLoading}
    onclick={() => onOpenDetails(product)}
    class="card-title dldr:mt-3 dldr:w-full dldr:wrap-anywhere dldr:border-0 dldr:bg-transparent dldr:p-0 dldr:text-left dldr:font-[inherit] dldr:text-base dldr:font-semibold dldr:leading-relaxed dldr:text-ui-ink dldr:cursor-pointer dldr:ui-focus-text"
    >{product.title}</button
  >
  <dl class="card-credits" aria-label={$t("作品資訊：{0}", product.workId)}>
    <div><dt><span class="card-text">{$t("社團")}</span></dt><dd>{#if onPrecisionSearch && product.makerName?.trim()}<button type="button" class="credit-search dldr:ui-focus-text" aria-label={$t("搜尋社團：{0}", product.makerName)} onclick={() => onPrecisionSearch?.("circle",product.makerName!)}>{product.makerName}</button>{:else}<span class="card-text">{product.makerName?.trim() || $t("未提供")}</span>{/if}</dd></div>
    <div class:has-voices={voiceNames.length > 0}><dt><span class="card-text">{$t("聲優")}</span></dt><dd class="voice-list" aria-live="polite">{#each voiceNames as name (name)}{#if onPrecisionSearch}<button type="button" class="voice-chip dldr:ui-focus-control" aria-label={$t("搜尋聲優：{0}", name)} onclick={() => onPrecisionSearch?.("va",name)}>{name}</button>{:else}<span class="voice-chip card-text">{name}</span>{/if}{:else}<span class="card-text">{creditsLoading ? $t("讀取中…") : $t("未提供")}</span>{/each}</dd></div>
    <div class="release-date">
      <dt><span class="card-text">{$t("Release date")}</span></dt>
      <dd>{#if releaseDate}<time class="card-text" datetime={releaseDate}>{releaseDate}</time>{:else}<span class="card-text">{$t("未提供")}</span>{/if}</dd>
    </div>
  </dl>
  <div class="file-meta">
  <p class="file-size" title={$t("Size reported by DLsite; actual transfer and extracted sizes may differ")}>
    <Icon name="downloads" />
    <span class="card-text">{typeof product.contentSizeBytes === "number" ? formatBytes(product.contentSizeBytes) : $t("Size unknown")}</span>
  </p>
  <a class="product-page-link dldr:ui-focus-text" href={dlsiteProductPageUrl(product.workId)}
    target="_blank" rel="noopener noreferrer" aria-label={$t("{0} on DLsite", product.workId)}
    title={$t("Open on DLsite ↗")}
    onclick={(event) => {
      if (onOpenDlsite) {
        event.preventDefault();
        onOpenDlsite(product.workId);
      }
    }}>DLsite ↗</a>
  </div>
  {#if onOpenOnline || onDownload || onOpenFolder}
    <div class="card-actions">
      <button type="button" class="online-play dldr:ui-focus-control" title={$t("Listen on DLsite Play")}
        disabled={!onOpenOnline || !purchased} aria-label={$t("Listen online: {0}",product.workId)} onclick={() => onOpenOnline?.(product)}>
        <span>DLPLAY</span>
      </button>
      {#if downloaded}
        <button type="button" class="work-folder dldr:ui-focus-control" disabled={!available || !onOpenFolder}
          aria-label={$t("Open downloaded folder for {0}",product.workId)} title={$t("Open downloaded folder")}
          onclick={() => onOpenFolder?.(product)}><span>{$t("Open directory")}</span></button>
      {:else}
        <button type="button" class="card-download dldr:ui-focus-control" disabled={downloadDisabled}
          aria-label={`${$t("Download")}: ${product.workId}`} title={status?.label || $t("Download")}
          onclick={download}><span>{$t("Download")}</span></button>
      {/if}
    </div>
  {/if}
  {#if productGenreTags(product).length > 0}
    <p class="genre-label"><span class="card-text">{$t("分類")}</span></p>
    <ul class="product-tags" aria-label={$t("Tags for {0}", product.workId)}>
      {#each productGenreTags(product) as tag (tag.genreId ?? tag.name)}
        {@const displayName = $genreName(tag.name, tag.genreId)}
        <li>
          {#if onTagSearch}
            <button type="button" class="product-tag dldr:ui-focus-control"
              aria-label={$t("Search tag: {0}", displayName)} title={$t("Search for {0}", displayName)}
              onclick={() => onTagSearch(displayName)}>{displayName}</button>
          {:else}
            <span class="product-tag card-text">{displayName}</span>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
  </div>
</article>

<style>
  .card-actions { display:flex; margin-top:8px; border:1px solid var(--dldr-color-ui-line-strong); border-radius:7px; overflow:hidden; }
  .card-actions button { display:flex; flex:1 1 0; min-width:0; justify-content:center; align-items:center; min-height:36px; padding:5px 4px; border:0; color:var(--dldr-color-ui-ink); background:var(--dldr-color-ui-surface); font:inherit; font-size:13px; font-weight:600; line-height:1.3; cursor:pointer; }
  .card-actions button + button { border-left:1px solid var(--dldr-color-ui-line-strong); }
  .card-actions button span { overflow-wrap:anywhere; }
  .card-actions .work-folder { background:var(--dldr-color-ui-selected); }
  .card-actions button:hover:not(:disabled),
  .card-actions button:focus-visible:not(:disabled) { background:var(--dldr-color-ui-accent); color:var(--dldr-color-ui-on-accent); }
  .card-actions button:disabled { opacity:.45; cursor:default; }
  .card-actions button:focus-visible { outline-offset:-3px; }
  .card-play-target { position:absolute; inset:0; width:100%; height:100%; padding:0; border:0; border-radius:inherit; background:transparent; cursor:pointer; }
  .card-content { position:relative; }
  /* Only empty space falls through to the sibling play button. Text stays
     selectable and native controls keep their own actions and keyboard access. */
  .card-content.playable { pointer-events:none; }
  .card-content :global(button),.card-content :global(a),.card-text { pointer-events:auto; }
  .cover-frame { position:relative; }
  .cover-labels { position:absolute; bottom:10px; left:8px; right:8px; display:flex; flex-wrap:wrap; align-items:center; gap:4px; pointer-events:none; }
  .cover-labels > span { display:flex; }
  .work-id { display:inline-flex; align-items:center; justify-content:center; min-height:24px; margin-left:auto; padding:2px 5px; border:1px solid var(--dldr-color-brand-white); border-radius:5px; background:#092a83e6; color:var(--dldr-color-brand-white); font-family:inherit; font-size:12px; font-weight:600; line-height:18px; font-variant-numeric:tabular-nums; text-align:center; cursor:pointer; }
  .playlist-toggle { position:absolute; top:8px; right:8px; display:grid; place-items:center; width:30px; height:30px; padding:0; border:1px solid var(--dldr-color-ui-line); border-radius:50%; background:var(--dldr-color-ui-surface); color:var(--dldr-color-ui-ink); cursor:pointer; }
  .playlist-toggle[aria-pressed=true] { background:var(--dldr-color-ui-selected); border-color:var(--dldr-color-brand-blue); }
  .playlist-toggle:disabled { opacity:.5; cursor:wait; }
  .work-id:hover { background:var(--dldr-color-brand-blue); }
  .product-card {
    box-sizing: border-box;
    padding: 12px;
    border: 1px solid var(--dldr-color-ui-line-subtle);
    border-radius: 14px;
    background: var(--dldr-color-ui-surface);
    box-shadow: 0 2px 6px var(--dldr-color-ui-shadow);
  }
  .product-card:hover, .product-card:focus-within {
    border-color: var(--dldr-color-ui-accent);
    box-shadow: 0 3px 10px var(--dldr-color-ui-shadow);
  }
  .product-card.selected {
    border-color: var(--dldr-color-ui-selected-border);
    box-shadow: inset 0 0 0 2px var(--dldr-color-ui-selected-border),
      0 2px 6px var(--dldr-color-ui-shadow);
  }
  .card-title { min-height: 3.2em; font-size: 14px; line-height: 1.6; }
  .card-credits { display: grid; gap: 6px; margin: 10px 0 0; font-size: 13px; line-height: 1.6; }
  .card-credits div { display: grid; grid-template-columns: minmax(2em, max-content) minmax(0, 1fr); gap: 8px; }
  .card-credits dt, .genre-label { color: var(--dldr-color-ui-dim); }
  .has-voices > dt { padding-top:3px; }
  .release-date { align-items: baseline; }
  .release-date time { font-size: 13px; font-variant-numeric: tabular-nums; white-space: nowrap; }
  .card-credits dd { min-width: 0; margin: 0; overflow-wrap: anywhere; color: var(--dldr-color-ui-ink); }
  .credit-search { display: inline; padding: 0; border: 0; background: transparent; color: inherit; font: inherit; text-align: left; overflow-wrap: anywhere; cursor: pointer; }
  .credit-search:hover { text-decoration: underline; text-underline-offset: 3px; }
  .genre-label { margin: 12px 0 0; font-size: 12px; }
  .product-tags { display: flex; flex-wrap: wrap; gap: 5px; padding: 0; margin: 5px 0 0; list-style: none; }
  .product-tags li { min-width: 0; max-width: 100%; }
  .product-tag { display: block; max-width: 100%; border: 1px solid var(--dldr-color-ui-genre-border); border-radius: 5px; padding: 3px 6px; background: var(--dldr-color-ui-genre-bg); color: var(--dldr-color-ui-genre-text); font: inherit; font-size: 12px; line-height: 1.5; overflow-wrap: anywhere; text-align: left; }
  button.product-tag { cursor: pointer; }
  button.product-tag:hover { border-color: var(--dldr-color-ui-accent); }
  .selection-toggle { position: absolute; top: 21px; left: 21px; z-index: 1; width: 34px; height: 34px; }
  .selection-checkbox { position: absolute; inset: 0; z-index: 1; width: 100%; height: 100%; margin: 0; opacity: 0; cursor: pointer; }
  .selection-mark { opacity: 0.55; display: grid; place-items: center; box-sizing: border-box; width: 100%; height: 100%; border: 2px solid var(--dldr-color-ui-ink); border-radius: 7px; background: var(--dldr-color-ui-surface); color: var(--dldr-color-ui-on-selection-mark); box-shadow: 0 0 0 2px var(--dldr-color-ui-surface), 0 2px 8px var(--dldr-color-ui-shadow-strong); font-size: 27px; font-weight: 900; line-height: 1; }
  .product-card:hover .selection-toggle:not(.unavailable) .selection-mark,
  .checked .selection-mark,
  .selection-checkbox:focus-visible + .selection-mark,
  .unavailable .selection-mark { opacity: 1; }
  .checked .selection-mark { background: var(--dldr-color-ui-selection-mark); }
  .selection-toggle:hover:not(.unavailable) .selection-mark { outline: 3px solid var(--dldr-color-ui-accent); outline-offset: 3px; }
  .unavailable { opacity: 0.4; }
  .selection-checkbox:disabled { cursor: default; }
  .selection-checkbox:focus-visible + .selection-mark { outline: 3px solid var(--dldr-color-ui-accent); outline-offset: 4px; }
  .file-meta { display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 6px 10px; margin: 8px 0 0; }
  .file-size { display: flex; align-items: center; gap: 6px; margin: 0; color: var(--dldr-color-ui-accent); font-size: 12px; font-variant-numeric: tabular-nums; }
  .product-page-link { margin-left: auto; color: var(--dldr-color-ui-accent); font-size: 12px; font-weight: 600; white-space: nowrap; text-decoration: none; }
  .product-page-link:hover { text-decoration: underline; text-underline-offset: 3px; }
</style>
