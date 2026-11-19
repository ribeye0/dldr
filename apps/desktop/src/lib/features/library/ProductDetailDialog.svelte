<script lang="ts">
  import { genreName } from "$lib/i18n/genres";
  import { t, locale } from "$lib/i18n";
  import type { ProductDetail, JobSnapshot } from "$lib/model/types";
  import {
    productCreditFields,
    productIsLocalOnly,
    productGenreTags,
  } from "$lib/utils/products";
  import {
    downloadStatusLabel,
    formatBytes,
  } from "$lib/utils/format";
  import { isActiveJob, jobLabel } from "$lib/utils/jobs";
  import BackToTop from "$lib/components/layout/BackToTop.svelte";
  import Modal from "$lib/components/ui/Modal.svelte";
  import Icon from "$lib/components/ui/Icon.svelte";
  import Button from "$lib/components/ui/Button.svelte";

  import { getProductPageContent } from "$lib/api/tauri";
  import { untrack } from "svelte";
  import { openExternalUrl } from "$lib/api/native";
  import WorkBadges from "$lib/components/ui/WorkBadges.svelte";
  import type { MetadataSearchKind } from "$lib/utils/filter-history.svelte";
  let {
    detail,
    activeJob = null,
    onClose,
    onPreview,
    onCopyText,
    onCopyWorkId,
    onCopyCredit,
    onOpenDlsite,
    onOpenPlayer,
    onDownload,
    onDownloadArchives,
    onLocateDownloaded,
    onRedownload,
    onDeleteDownload,
    downloadDisabled = false,
    onFilter,
  }: {
    detail: ProductDetail | null;
    activeJob?: JobSnapshot | null;
    onClose?: () => void;
    onPreview?: (detail: ProductDetail) => void;
    onCopyText?: (label: string, value: string | null, workId: string) => void;
    onCopyWorkId?: (id: string) => void;
    onCopyCredit?: (
      field: import("$lib/model/types").ProductCreditField,
      id: string,
    ) => void;
    onOpenDlsite?: (id: string) => void;
    onOpenPlayer?: (detail: ProductDetail) => void;
    onDownload?: () => void;
    onDownloadArchives?: () => void;
    onLocateDownloaded?: () => void;
    onRedownload?: () => void;
    onDeleteDownload?: () => void;
    downloadDisabled?: boolean;
    onFilter?: (kind: MetadataSearchKind, name: string) => void;
  } = $props();
  const job = $derived(activeJob && isActiveJob(activeJob) ? activeJob : null);
  const voiceNames = $derived([...new Set(detail?.creditGroups.filter(group => group.kind === "voice").flatMap(group => group.names) ?? [])]);
  const statusLabel = $derived(
    job
      ? job.status === "running" && job.phase === "downloading"
        ? $t("Downloading")
        : job.status === "running" && job.phase === "unpacking"
          ? $t("Unpacking")
          : jobLabel(job, $locale)
      : downloadStatusLabel(detail?.download.status ?? "notDownloaded"),
  );
  function copy(label: string, value: string | null) {
    if (detail && value) onCopyText?.(label, value, detail.workId);
  }
  const productionCredits = $derived(detail ? productCreditFields(detail).filter(field => field.key !== "maker" && field.key !== "voice" && !field.missing) : []);
  let pageHtml = $state<string | null>(null);
  let pageLoading = $state(false);
  let pageError = $state(false);
  let contentOpen = $state(false);
  let contentWorkId = "";
  let contentGeneration = 0;
  $effect(() => {
    const id = detail?.workId ?? "";
    untrack(() => {
      if (contentWorkId === id) return;
      contentWorkId = id;
      contentGeneration++;
      contentOpen = false;
      pageHtml = null;
      pageLoading = false;
      pageError = false;
    });
  });
  async function loadPageContent() {
    const id = detail?.workId;
    if (!id || pageLoading || pageHtml !== null) return;
    const generation = contentGeneration;
    pageLoading = true;
    pageError = false;
    try {
      const html = await getProductPageContent(id, $locale);
      if (contentGeneration === generation) pageHtml = html;
    } catch {
      if (contentGeneration === generation) pageError = true;
    } finally {
      if (contentGeneration === generation) pageLoading = false;
    }
  }
  // Public metadata can carry the original upload date as its update date.
  const updateDate = $derived(detail ? date(detail.updatedAt) : "—");
  const releaseDate = $derived(detail ? date(detail.publishedAt ?? detail.registeredAt) : "—");
  const hasUpdate = $derived(updateDate !== "—" && (releaseDate === "—" || updateDate > releaseDate));
  function productLinks(node: HTMLElement) {
    const open = (event: MouseEvent) => {
      const target = event.target instanceof Element ? event.target.closest("a[href]") : null;
      if (!(target instanceof HTMLAnchorElement)) return;
      event.preventDefault();
      event.stopPropagation();
      if (event.type === "auxclick" && event.button !== 1) return;
      if (target.protocol === "https:") void openExternalUrl(target.href).catch(() => { pageError = true; });
    };
    node.addEventListener("click", open);
    node.addEventListener("auxclick", open);
    return { destroy() { node.removeEventListener("click", open); node.removeEventListener("auxclick", open); } };
  }
  function date(value: string | null) {
    return value?.match(/^\d{4}-\d{2}-\d{2}/)?.[0] ?? "—";
  }
</script>

{#if detail}
  <Modal title={$t("Work details")} wide onClose={() => onClose?.()}>
    <div class="product-content">
      <div class="work-links" aria-label={$t("Work links")}>
        <button type="button" class="work-id dldr:ui-focus-control" title={$t("Copy work ID")}
          onclick={() => onCopyWorkId ? onCopyWorkId(detail.workId) : copy($t("Work ID"), detail.workId)}>
          {detail.workId}<Icon name="copy" />
        </button>
        <span class="link-divider" aria-hidden="true"></span>
        <Button variant="text" onclick={() => detail && onOpenDlsite?.(detail.workId)}>{$t("Open on DLsite ↗")}</Button>
      </div>
      <header class="identity-heading">
        {#if detail.thumbnailUrl}
          <button type="button" class="art dldr:ui-focus-image" aria-label={$t("Preview cover image")}
            aria-haspopup="dialog" onclick={() => detail && onPreview?.(detail)}>
            <img src={detail.thumbnailUrl} alt={$t("{0} cover", detail.title)} />
          </button>
        {:else}<span class="missing-art"><Icon name="library" />{$t("No image")}</span>{/if}
        <div class="identity-info">
          <WorkBadges workType={detail.workType} ageCategory={detail.ageCategory} />
          <h3><button type="button" class="copy-title dldr:ui-focus-text" title={$t("Copy title")}
            onclick={() => copy($t("Title"), detail.title)}>{detail.title}</button></h3>
          <dl class="identity-credits" aria-label={$t("作品資訊")}>
            {#each productCreditFields(detail).filter((field) => field.key === "maker" || field.key === "voice") as field}
              <div><dt>{$t(field.label)}</dt><dd class:voice-list={field.key === "voice"}>
                {#if field.key === "voice" && voiceNames.length}
                  {#each voiceNames as name (name)}
                    <button type="button" class="voice-chip dldr:ui-focus-control" title={onFilter ? $t("搜尋聲優：{0}", name) : $t("Copy {0}", name)}
                      aria-label={onFilter ? $t("搜尋聲優：{0}", name) : undefined}
                      onclick={() => onFilter ? onFilter("va", name) : onCopyCredit ? onCopyCredit({ ...field, value: name }, detail.workId) : copy(field.label, name)}>{name}</button>
                  {/each}
                {:else}<button type="button" class="copy-value dldr:ui-focus-text"
                  disabled={field.missing} title={onFilter ? $t(field.key === "maker" ? "搜尋社團：{0}" : "搜尋聲優：{0}", field.value) : $t("複製{0}", $t(field.label))}
                  aria-label={onFilter && !field.missing ? $t(field.key === "maker" ? "搜尋社團：{0}" : "搜尋聲優：{0}", field.value) : undefined}
                  onclick={() => onFilter ? onFilter(field.key === "maker" ? "circle" : "va", field.value) : onCopyCredit ? onCopyCredit(field, detail.workId) : copy(field.label, field.value)}
                  >{field.missing ? $t("未提供") : field.value}</button>{/if}
              </dd></div>
            {/each}
          </dl>
        </div>
      </header>
      <div class="download-panel" role="group" aria-label={$t("Work actions")}>
        <div class="download-primary">
          <div class="download-summary">
            <span class="download-status" class:downloaded={detail.download.status === "downloaded" && !job}>
              <span class="status-dot" aria-hidden="true"></span>{$t(statusLabel)}
            </span>
            <span class="file-size"><Icon name="downloads" />{detail.contentSizeBytes === null ? $t("Size unavailable") : formatBytes(detail.contentSizeBytes)}
              {#if productIsLocalOnly(detail)}<span>· {$t("Local Only")}</span>{/if}
            </span>
          </div>
          <div class="primary-actions">
            {#if onOpenPlayer && detail.download.status === "downloaded" && detail.download.localPath}
              <Button variant="primary" onclick={() => detail && onOpenPlayer?.(detail)}><Icon name="play" />{$t("Listen / tracks")}</Button>
            {/if}
            <Button variant={onOpenPlayer && detail.download.status === "downloaded" && detail.download.localPath ? "secondary" : "primary"}
              disabled={downloadDisabled || Boolean(job) || (detail.download.status === "downloaded" && !detail.download.localPath)}
              onclick={onDownload}>
              <Icon name={detail.download.status === "downloaded" ? "folder" : "downloads"} />
              {detail.download.status === "downloaded" ? $t("Open folder") : job ? $t(statusLabel) : $t("Download")}
            </Button>
          </div>
        </div>
        <div class="secondary-actions">
          <div class="download-options">
            {#if detail.download.status !== "downloaded"}
              <Button variant="text" disabled={downloadDisabled || Boolean(job)} onclick={onDownloadArchives}>
                <Icon name="unpack" />{$t("Download archives")}
              </Button>
              <Button variant="text" disabled={Boolean(job)} title={$t("Link this work to an existing local folder.")} onclick={onLocateDownloaded}>
                <Icon name="search" />{$t("Locate downloaded work")}
              </Button>
            {:else}
              <Button variant="text" disabled={Boolean(job)} onclick={onRedownload}><Icon name="refresh" />{$t("Re-download")}</Button>
            {/if}
          </div>
          {#if detail.download.status !== "notDownloaded"}
            <Button variant="text" disabled={Boolean(job)} tone="error" onclick={onDeleteDownload}>{$t("Delete download")}</Button>
          {/if}
        </div>
      </div>
      {#if detail.download.errorMessage}<p class="download-error" role="status">
          <strong>{$t("Download needs attention")}</strong>{detail.download
            .errorMessage} ({detail.download.errorCode ?? "unknown"})
        </p>{/if}
      <div class="tag-overview">
        {#if productGenreTags(detail).length}<section aria-labelledby="detail-tags">
            <h4 id="detail-tags">{$t("分類")}</h4>
            <div class="chips">
              {#each productGenreTags(detail) as tag}<button type="button"
                  class="dldr:ui-focus-control"
                  aria-label={onFilter ? $t("Search tag: {0}", $genreName(tag.name, tag.genreId)) : undefined}
                  title={onFilter ? $t("Search for {0}", $genreName(tag.name, tag.genreId)) : $t("Copy {0}", $genreName(tag.name, tag.genreId))}
                  onclick={() => onFilter ? onFilter("tag", $genreName(tag.name, tag.genreId)) : copy($t("分類"), $genreName(tag.name, tag.genreId))}>{$genreName(tag.name, tag.genreId)}</button
                >{/each}
            </div>
          </section>{/if}
      </div>
      {#if productionCredits.length}<section aria-labelledby="detail-credits">
        <h4 id="detail-credits">{$t("其他製作資訊")}</h4>
        <dl class="credits-grid">
          {#each productionCredits as field}<div>
              <dt>{$t(field.label)}</dt>
              <dd>
                <button
                  class="copy-value dldr:ui-focus-text"
                  disabled={field.missing}
                  title={$t("Copy {0}", $t(field.label))}
                  onclick={() =>
                    onCopyCredit
                      ? onCopyCredit(field, detail.workId)
                      : copy(field.label, field.value)}>{field.value}</button
                >
              </dd>
            </div>{/each}
        </dl>
      </section>{/if}
      <section class="dates-section" aria-labelledby="detail-dates">
        <h4 id="detail-dates">{$t("Dates")}</h4>
        <dl class="dates-grid">
          <div><dt>{$t("Registered")}</dt><dd>{date(detail.registeredAt)}</dd></div>
          <div><dt>{$t("Published")}</dt><dd>{date(detail.publishedAt)}</dd></div>
          {#if hasUpdate}<div><dt>{$t("Updated")}</dt><dd>{updateDate}</dd></div>{/if}
          <div><dt>{$t("Purchased")}</dt><dd>{date(detail.earliestPurchasedAt ?? detail.latestPurchasedAt)}</dd></div>
        </dl>
      </section>
      <div class="metadata">
        <details class="product-page-section" open={contentOpen} ontoggle={(event) => {
          contentOpen = event.currentTarget.open;
          if (contentOpen) void loadPageContent();
        }}>
          <summary class="dldr:ui-focus-row"><Icon name="arrow" />{$t("Product page content")}</summary>
          {#if contentOpen}<div class="product-page-body">
            {#if pageLoading}<p role="status">{$t("Loading…")}</p>
            {:else if pageError}<p role="alert">{$t("Could not load product page content.")}</p>
              <Button variant="secondary" onclick={loadPageContent}>{$t("Retry")}</Button>
            {:else if pageHtml}<div class="product-page-content" use:productLinks>{@html pageHtml}</div>
            {:else}<p>{$t("Product page content is unavailable.")}</p>{/if}
            <Button variant="text" onclick={() => detail && onOpenDlsite?.(detail.workId)}>{$t("Open on DLsite ↗")}</Button>
          </div>{/if}
        </details>
      </div>
    </div>
    {#snippet actions()}<BackToTop />{/snippet}
  </Modal>
{/if}

<style>
  .product-content { color: var(--dldr-color-ui-ink); font-size: 14px; }
  button { font: inherit; cursor: pointer; }
  button:disabled { opacity: .55; cursor: default; }
  .work-links { display: flex; justify-content: flex-end; align-items: center; gap: 8px; margin: -4px 0 20px; flex-wrap: wrap; }
  .work-id { display: inline-flex; align-items: center; gap: 8px; min-height: 32px; padding: 4px 8px; border: 0; border-radius: 6px; background: transparent; color: var(--dldr-color-ui-ink); font-family: ui-monospace, monospace; font-size: 13px; font-weight: 700; }
  .work-id:hover { background: var(--dldr-color-ui-hover); }
  .work-id :global(svg) { width: 14px; height: 14px; color: var(--dldr-color-ui-dim); }
  .link-divider { width: 1px; height: 16px; background: var(--dldr-color-ui-line); }
  .identity-heading { display: grid; grid-template-columns: 210px minmax(0, 1fr); gap: 24px; align-items: start; }
  .identity-info { min-width: 0; }
  .art, .missing-art { box-sizing: border-box; display: grid; place-items: center; width: 100%; min-height: 158px; padding: 0; overflow: hidden; border: 1px solid var(--dldr-color-ui-line); border-radius: 12px; background: var(--dldr-color-ui-elevated); }
  .art img { display: block; width: 100%; max-height: 260px; object-fit: contain; }
  .art:hover { border-color: var(--dldr-color-ui-accent); }
  .missing-art { gap: 8px; align-content: center; aspect-ratio: 4 / 3; color: var(--dldr-color-ui-dim); }
  .missing-art :global(svg) { width: 30px; height: 30px; }
  h3 { margin: 10px 0 16px; }
  h4 { margin: 0 0 14px; font-size: 14px; font-weight: 700; }
  .copy-title, .copy-value { background: none; border: 0; padding: 0; text-align: left; color: inherit; overflow-wrap: anywhere; }
  .copy-title { font-size: 21px; line-height: 1.5; font-weight: 700; }
  .copy-value { line-height: inherit; }
  .copy-value:hover:not(:disabled), .copy-title:hover { color: var(--dldr-color-ui-accent); text-decoration: underline; text-underline-offset: 3px; }
  dl { margin: 0; }
  dt { color: var(--dldr-color-ui-dim); }
  dd { margin: 0; min-width: 0; overflow-wrap: anywhere; }
  .identity-credits { display: grid; grid-template-columns: max-content minmax(0, 1fr); align-items: baseline; gap: 10px 12px; font-size: 14px; line-height: 1.8; }
  .identity-credits > div { display: contents; }
  .voice-list { display: flex; flex-wrap: wrap; align-items: baseline; gap: 5px; }
  .download-panel { margin-top: 24px; border: 1px solid var(--dldr-color-ui-line); border-radius: 12px; background: var(--dldr-color-ui-elevated); overflow: hidden; }
  .download-primary { display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 18px; flex-wrap: wrap; }
  .download-summary { display: grid; gap: 7px; }
  .download-status { display: flex; align-items: center; gap: 7px; font-weight: 600; }
  .status-dot { width: 7px; height: 7px; border-radius: 50%; background: var(--dldr-color-ui-dim); }
  .downloaded .status-dot { background: var(--dldr-color-ui-accent); }
  .file-size { display: flex; align-items: center; flex-wrap: wrap; gap: 6px; color: var(--dldr-color-ui-dim); font-size: 13px; }
  .file-size :global(svg) { width: 15px; height: 15px; }
  .primary-actions, .download-options { display: flex; flex-wrap: wrap; gap: 8px; }
  .secondary-actions { display: flex; justify-content: space-between; align-items: center; flex-wrap: wrap; gap: 8px; padding: 8px; border-top: 1px solid var(--dldr-color-ui-line); }
  .secondary-actions :global(.ui-button) { font-size: 13px; }
  section { padding: 22px 0; border-bottom: 1px solid var(--dldr-color-ui-line); }
  .chips { display: flex; gap: 7px; flex-wrap: wrap; }
  .chips button { display: inline-flex; align-items: center; min-height: 30px; padding: 4px 10px; border: 1px solid var(--dldr-color-ui-genre-border); border-radius: 7px; background: var(--dldr-color-ui-genre-bg); color: var(--dldr-color-ui-genre-text); font-size: 13px; line-height: 1.6; overflow-wrap: anywhere; text-align: left; }
  .chips button:hover { filter: brightness(.95); }
  .credits-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 16px 24px; line-height: 1.8; }
  .credits-grid > div, .dates-grid > div { min-width: 0; }
  .credits-grid dt, .dates-grid dt { font-size: 12px; margin-bottom: 4px; }
  .dates-grid { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 16px; }
  .dates-grid dd { font-size: 14px; font-variant-numeric: tabular-nums; line-height: 1.8; }
  .download-error { padding: 14px; background: var(--dldr-color-ui-error-bg); border-radius: 8px; color: var(--dldr-color-ui-error); line-height: 1.6; overflow-wrap: anywhere; }
  .download-error strong { display: block; margin-bottom: 4px; }
  .product-page-section { margin-top: 20px; border: 1px solid var(--dldr-color-ui-line); border-radius: 10px; overflow: hidden; }
  .product-page-section > summary { display: flex; align-items: center; gap: 10px; cursor: pointer; padding: 16px; min-height: 24px; list-style: none; font-size: 14px; font-weight: 700; }
  .product-page-section > summary:hover { background: var(--dldr-color-ui-hover); }
  .product-page-section > summary::-webkit-details-marker { display: none; }
  .product-page-section > summary :global(svg) { transition: transform .15s ease; }
  .product-page-section[open] > summary :global(svg) { transform: rotate(90deg); }
  .product-page-body { padding: 0 16px 16px; }
  .product-page-content { overflow-wrap: anywhere; line-height: 1.8; font-size: 14px; }
  .product-page-content :global(img) { display: block; max-width: 100%; height: auto; margin: 12px auto; }
  .product-page-content :global(h1), .product-page-content :global(h2), .product-page-content :global(h3) { font-size: 16px; line-height: 1.5; margin: 20px 0 10px; }
  .product-page-content :global(section) { margin: 16px 0; }
  .product-page-content :global(a) { color: var(--dldr-color-ui-accent); }
  .product-page-content :global(table) { display: block; overflow-x: auto; max-width: 100%; }
  @media (max-width: 620px) {
    .identity-heading { grid-template-columns: 120px minmax(0, 1fr); gap: 16px; }
    .art, .missing-art { min-height: 90px; border-radius: 8px; }
    .copy-title { font-size: 18px; }
    .identity-credits { grid-template-columns: 1fr; gap: 2px; }
    .identity-credits > div + div dt { margin-top: 6px; }
    .dates-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .download-primary { padding: 14px; }
  }
  @media (max-width: 420px) {
    .identity-heading { grid-template-columns: 1fr; }
    .art, .missing-art { max-width: 210px; margin: 0 auto; }
    .identity-credits { grid-template-columns: max-content minmax(0, 1fr); gap: 8px 10px; }
    .identity-credits > div + div dt { margin-top: 0; }
    .work-links { gap: 4px; }
    .credits-grid { grid-template-columns: 1fr; }
    .primary-actions { width: 100%; }
    .primary-actions :global(.ui-button) { flex: 1; }
  }
</style>
