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
  import Disclosure from "$lib/components/ui/Disclosure.svelte";

  import { getProductPageContent } from "$lib/api/tauri";
  import { untrack } from "svelte";
  import { openExternalUrl } from "$lib/api/native";
  import WorkBadges from "$lib/components/ui/WorkBadges.svelte";
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
    onMarkDownloaded,
    onRedownload,
    onDeleteDownload,
    downloadDisabled = false,
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
    onMarkDownloaded?: () => void;
    onRedownload?: () => void;
    onDeleteDownload?: () => void;
    downloadDisabled?: boolean;
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
      <header class="identity-heading">
        {#if detail.thumbnailUrl}<button
            class="art dldr:ui-focus-image"
            aria-label={$t("Preview cover image")}
            aria-haspopup="dialog"
            onclick={() => detail && onPreview?.(detail)}
            ><img
              src={detail.thumbnailUrl}
              alt={$t("{0} cover", detail.title)}
            /></button
          >{:else}<span class="missing-art">{$t("No image")}</span>{/if}
        <div>
          <WorkBadges
            workType={detail.workType}
            ageCategory={detail.ageCategory}
          />
          <h3>
            <button
              class="copy-title dldr:ui-focus-text"
              title={$t("Copy title")}
              onclick={() => copy($t("Title"), detail.title)}>{detail.title}</button
            >
          </h3>
          <dl class="identity-credits" aria-label={$t("作品資訊")}>
            {#each productCreditFields(detail).filter((field) => field.key === "maker" || field.key === "voice") as field}
              <div><dt>{$t(field.label)}</dt><dd class:voice-list={field.key === "voice"}>
                {#if field.key === "voice" && voiceNames.length}
                  {#each voiceNames as name (name)}
                    <button type="button" class="voice-chip dldr:ui-focus-control" title={$t("Copy {0}", name)}
                      onclick={() => onCopyCredit ? onCopyCredit({ ...field, value: name }, detail.workId) : copy(field.label, name)}>{name}</button>
                  {/each}
                {:else}<button class="copy-value dldr:ui-focus-text"
                disabled={field.missing} title={$t("複製{0}", $t(field.label))}
                onclick={() => onCopyCredit ? onCopyCredit(field, detail.workId) : copy(field.label, field.value)}
                >{field.missing ? $t("未提供") : field.value}</button>{/if}</dd></div>
            {/each}
          </dl>
          <p class="quick-facts">
            {detail.contentSizeBytes === null
              ? $t("Size unavailable")
              : formatBytes(detail.contentSizeBytes)}
            {#if productIsLocalOnly(detail)}<span>·</span>{$t("Local Only")}{/if}
          </p>
          <div class="identity-actions">
            <Button
              variant="secondary"
              class="work-id"
              title={$t("Copy work ID")}
              onclick={() =>
                onCopyWorkId
                  ? onCopyWorkId(detail.workId)
                  : copy($t("Work ID"), detail.workId)}
              >{detail.workId} <Icon name="copy" /></Button
            ><Button
              variant="text"
              class="text-button"
              onclick={() => detail && onOpenDlsite?.(detail.workId)}
              >{$t("Open on DLsite ↗")}</Button
            >
          </div>
        </div>
      </header>
      <div
        class="detail-actions dldr:mt-4 dldr:border-0 dldr:border-b dldr:border-solid dldr:border-ui-line dldr:pb-4"
      >
        <span class="dldr:block dldr:mb-2 dldr:text-xs dldr:text-ui-dim"
          >{$t(statusLabel)}</span
        >
        <div
          class="download-actions dldr:flex dldr:flex-wrap dldr:items-center dldr:gap-2"
          role="group"
          aria-label={$t("Work actions")}
        >
          {#if onOpenPlayer && detail.download.status === "downloaded" && detail.download.localPath}
            <Button variant="secondary" onclick={() => detail && onOpenPlayer?.(detail)}><Icon name="play" />{$t("Listen / tracks")}</Button>
          {/if}
          <Button
            variant="primary"
            disabled={downloadDisabled ||
              Boolean(job) ||
              (detail.download.status === "downloaded" &&
                !detail.download.localPath)}
            onclick={onDownload}
            ><Icon
              name={detail.download.status === "downloaded"
                ? "folder"
                : "downloads"}
            />{detail.download.status === "downloaded"
              ? $t("Open folder")
              : job
                ? $t(statusLabel)
                : $t("Download")}</Button
          >
          {#if detail.download.status !== "downloaded"}<Button
              variant="text"
              disabled={Boolean(job)}
              onclick={onDownloadArchives}>{$t("Download archives only")}</Button
            ><Button
              variant="text"
              disabled={Boolean(job)}
              onclick={onMarkDownloaded}>{$t("Mark as downloaded")}</Button
            >{:else}<Button
              variant="text"
              disabled={Boolean(job)}
              onclick={onRedownload}>{$t("Re-download")}</Button
            >{/if}{#if detail.download.status !== "notDownloaded"}<Button
              variant="text"
              disabled={Boolean(job)}
              tone="error"
              onclick={onDeleteDownload}>{$t("Delete download")}</Button
            >{/if}
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
              {#each productGenreTags(detail) as tag}<button
                  class="dldr:ui-focus-control"
                  onclick={() => copy($t("分類"), $genreName(tag.name, tag.genreId))}>{$genreName(tag.name, tag.genreId)}</button
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
      <div class="metadata">
        <Disclosure title={$t("Dates")}>
          <div class="metadata-body">
            <dl>
              <dt>{$t("Registered")}</dt>
              <dd>{date(detail.registeredAt)}</dd>
              <dt>{$t("Published")}</dt>
              <dd>{date(detail.publishedAt)}</dd>
              {#if hasUpdate}<dt>{$t("Updated")}</dt>
                <dd>{updateDate}</dd>{/if}
              <dt>{$t("Purchased")}</dt>
              <dd>{date(detail.earliestPurchasedAt ?? detail.latestPurchasedAt)}</dd>
            </dl>
          </div>
        </Disclosure>
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
  .identity-credits { display: grid; grid-template-columns: max-content minmax(0, 1fr); align-items: baseline; gap: 6px 10px; margin: 10px 0; font-size: 13px; line-height: 1.6; }
  .identity-credits > div { display: contents; }
  .identity-credits dt { color: var(--dldr-color-ui-dim); }
  .identity-credits dd { min-width: 0; margin: 0; line-height: inherit; overflow-wrap: anywhere; }
  .identity-credits .voice-list { align-items: baseline; }
  .identity-credits button.copy-value { font-size: inherit; line-height: inherit; }
  .product-content {
    color: var(--dldr-color-ui-ink);
    font-size: 13px;
  }
  button {
    font: inherit;
  }
  button:not(.voice-chip) {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 7px 11px;
    min-height: 32px;
    border: 1px solid var(--dldr-color-ui-line-strong);
    border-radius: 6px;
    background: var(--dldr-color-ui-elevated);
    color: var(--dldr-color-ui-ink);
    cursor: pointer;
  }
  button:disabled {
    opacity: 0.55;
    cursor: default;
  }
  .identity-heading {
    display: grid;
    grid-template-columns: 144px minmax(0, 1fr);
    gap: 20px;
    align-items: start;
  }
  .art {
    padding: 0;
    overflow: hidden;
    background: none;
  }
  .art img {
    width: 100%;
    aspect-ratio: 1;
    object-fit: contain;
    display: block;
  }
  .missing-art {
    display: grid;
    place-items: center;
    aspect-ratio: 1;
    background: var(--dldr-color-ui-elevated);
    color: var(--dldr-color-ui-dim);
    border-radius: 6px;
  }
  h3 {
    margin: 9px 0 7px;
  }
  h4 {
    margin: 0 0 10px;
    font-size: 14px;
    font-weight: 600;
  }
  p {
    color: var(--dldr-color-ui-dim);
    line-height: 1.6;
    font-size: 12px;
    margin: 8px 0;
  }
  button.copy-title,
  button.copy-value {
    background: none;
    border: 0;
    border-radius: 0;
    padding: 0;
    min-height: 0;
    text-align: left;
    overflow-wrap: anywhere;
  }
  button.copy-title {
    font-size: 21px;
    line-height: 1.4;
    font-weight: 600;
  }
  button.copy-value {
    font-size: 12px;
    line-height: 1.7;
  }
  button.copy-value:hover:not(:disabled),
  button.copy-title:hover {
    color: var(--dldr-color-ui-accent);
    text-decoration: underline;
  }
  .identity-actions {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
    margin-top: 12px;
  }
  section {
    padding: 18px 0;
    border-bottom: 1px solid var(--dldr-color-ui-line);
  }
  dl {
    display: grid;
    grid-template-columns: 135px minmax(0, 1fr);
    gap: 11px 20px;
    margin: 0;
    font-size: 12px;
  }
  dt {
    color: var(--dldr-color-ui-dim);
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
    line-height: 1.7;
  }
  .chips {
    display: flex;
    gap: 7px;
    flex-wrap: wrap;
  }
  .chips button {
    font-size: 11px;
    overflow-wrap: anywhere;
    text-align: left;
  }
  .quick-facts {
    display: flex;
    flex-wrap: wrap;
    gap: 7px;
    margin-top: 7px;
    font-size: 11px;
  }
  .tag-overview { width: 100%; }
  .chips > button {
    background: var(--dldr-color-ui-genre-bg);
    border-color: var(--dldr-color-ui-genre-border);
    color: var(--dldr-color-ui-genre-text);
  }
  .credits-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px 24px;
  }
  .credits-grid > div {
    min-width: 0;
  }
  .credits-grid dt {
    font-size: 12px;
    margin-bottom: 2px;
  }
  .metadata-body > dl {
    margin-top: 0;
  }
  .download-error {
    padding: 12px;
    background: var(--dldr-color-ui-error-bg);
    border-radius: 6px;
    color: var(--dldr-color-ui-error);
    overflow-wrap: anywhere;
  }
  .download-error strong {
    display: block;
    margin-bottom: 4px;
  }
  @media (max-width: 620px) {
    .identity-heading {
      grid-template-columns: 78px minmax(0, 1fr);
      gap: 13px;
    }
    button.copy-title {
      font-size: 17px;
    }
    dl {
      grid-template-columns: 100px minmax(0, 1fr);
      gap: 10px;
    }
    .identity-actions {
      gap: 5px;
    }
  }
  .product-page-section { border-bottom: 1px solid var(--dldr-color-ui-line); }
  .product-page-section > summary { display: flex; align-items: center; gap: 8px; cursor: pointer; padding: 14px 12px; min-height: 44px; list-style: none; font-size: 14px; }
  .product-page-section > summary::-webkit-details-marker { display: none; }
  .product-page-section[open] > summary :global(svg) { transform: rotate(90deg); }
  .product-page-body { padding: 8px 12px 16px; }
  .product-page-content { overflow-wrap: anywhere; line-height: 1.8; font-size: 14px; }
  .product-page-content :global(img) { display: block; max-width: 100%; height: auto; margin: 12px auto; }
  .product-page-content :global(h1), .product-page-content :global(h2), .product-page-content :global(h3) { font-size: 16px; line-height: 1.5; margin: 20px 0 10px; }
  .product-page-content :global(section) { margin: 16px 0; }
  .product-page-content :global(a) { color: var(--dldr-color-ui-accent); }
  .product-page-content :global(table) { display: block; overflow-x: auto; max-width: 100%; }
</style>
