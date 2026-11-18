<script lang="ts">
  import { t, locale } from "$lib/i18n";
  import Modal from "$lib/components/ui/Modal.svelte";
  import UiButton from "$lib/components/ui/ActionButton.svelte";
  import Choices from "$lib/components/ui/ChoiceGroup.svelte";
  import { DEFAULT_DOWNLOAD_CONCURRENCY, MAX_DOWNLOAD_CONCURRENCY } from "$lib/utils/download-concurrency";
  import type { BulkDownloadDialog } from "$lib/model/types";
  import { bulkDownloadExpectedBytesLabel, formatBytes } from "$lib/utils/format";

  let {
    dialog,
    concurrency = $bindable(DEFAULT_DOWNLOAD_CONCURRENCY),
    onClose,
  }: {
    dialog: BulkDownloadDialog | null;
    concurrency?: number;
    onClose?: (confirmed: boolean) => void;
  } = $props();
</script>

{#if dialog}
  <Modal
    title={dialog.kind === "notice"
      ? $t("No products to download")
      : dialog.selectedProducts
        ? $t("Download selected works?")
      : $t("Start bulk download?")}
    onClose={() => onClose?.(false)}
  >
    <div class="summary" aria-label={$t("Bulk download plan")}>
      <div>
        <span>{$t("Products to download")}</span>
        <strong>{dialog.preview.requestedCount}</strong>
      </div>
      <div>
        <span>{$t("Checked products")}</span>
        <strong>{dialog.preview.plannedCount}</strong>
      </div>
      <div>
        <span>{$t("Already downloaded")}</span>
        <strong>{dialog.preview.skippedDownloadedCount}</strong>
      </div>
      <div>
        <span>{$t("Already queued")}</span>
        <strong>{dialog.preview.skippedQueuedCount}</strong>
      </div>
      <div class="wide">
        <span>{$t("Expected total download")}</span>
        <strong>{bulkDownloadExpectedBytesLabel(dialog.preview, $locale)}</strong>
      </div>
    </div>

    {#if dialog.space}
      <section aria-label={$t("Disk space estimate")}>
        <strong>{$t("磁碟空間預檢")}</strong>
        {#each dialog.space.budgets as budget}
          <p class:warning={!budget.sufficient} class="note">{budget.path}<br />{$t("可用")} {formatBytes(budget.availableBytes)} {$t("· 預估需求")} {formatBytes(budget.requiredBytes)} {$t("· 其他下載預留")} {formatBytes(budget.reservedBytes)}{budget.sufficient ? "" : $t("· 空間不足")}</p>
        {/each}
        <p class="note">{$t("包含解壓與暫存預估及 256 MiB 保留空間；壓縮比未知，實際需求可能更高。")}{dialog.space.unknownCount ? $t("{0} 部容量未知，先採每部 256 MiB 估計。", dialog.space.unknownCount) : ""}{$t("下載、解壓和跨磁碟搬移時會再檢查。")}</p>
      </section>
    {/if}

    {#if dialog.kind === "confirm"}
      <div class="concurrency-control">
        <strong>{$t("Simultaneous works per batch")}</strong>
        <Choices label={$t("Simultaneous works per batch")}
          value={String(concurrency)} onchange={(value) => concurrency = Number(value)}
          options={Array.from({ length: MAX_DOWNLOAD_CONCURRENCY }, (_, i) => ({ value: String(i + 1), label: String(i + 1) }))} />
        <p class="note">{$t("Download up to")} {concurrency} {$t("works at once in this batch. Choose 1 for sequential downloads. All batches also share the global limit on Downloads.")}</p>
      </div>
    {/if}

    {#if dialog.selectedProducts}
      <p class="note">{$t("Only these")} {dialog.selectedProducts.length} {$t("selected works are included. Downloaded or queued works are skipped.")}</p>
      <ul class="selected-works" aria-label={$t("Works in this download selection")}>
        {#each dialog.selectedProducts as product (product.workId)}
          <li>
            {#if product.thumbnailUrl}<img src={product.thumbnailUrl} alt="" loading="lazy" />{/if}
            <div><strong>{product.title}</strong><span>{product.workId} · {product.makerName ?? $t("Unknown maker")}</span><span>{typeof product.contentSizeBytes === "number" ? formatBytes(product.contentSizeBytes) : $t("Size unknown")}</span></div>
          </li>
        {/each}
      </ul>
      <p class="note">{$t("Download size is an estimate. Extracted files and temporary archives may need additional space. Your selection remains saved after queuing.")}</p>
    {/if}

    {#if dialog.preview.failedCount > 0}
      <p class="warning">
        {dialog.preview.failedCount} {$t("product(s) could not be checked before download. They will still be attempted and may fail.")}
      </p>
    {/if}

    {#if dialog.kind === "notice"}
      <p class="note">
        {$t("Matching products were already downloaded, already queued, or unavailable for this action.")}
      </p>
    {/if}

    <div class="actions" class:notice={dialog.kind === "notice"}>
      {#if dialog.kind === "notice"}
        <UiButton onclick={() => onClose?.(false)}>{$t("Close")}</UiButton>
      {:else}
        <UiButton variant="secondary" onclick={() => onClose?.(false)}
          >{$t("Cancel")}</UiButton
        >
        <UiButton disabled={dialog.space?.budgets.some(b => !b.sufficient)} onclick={() => onClose?.(true)}>{$t("Start Download")}</UiButton>
      {/if}
    </div>
  </Modal>
{/if}

<style>
  .concurrency-control { display: grid; gap: 10px; margin-top: 20px; padding: 14px; border: 1px solid var(--border); border-radius: 8px; }
  .concurrency-control .note { margin: 0; color: var(--muted); font-size: 12px; }
  .summary {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 16px;
  }
  .summary div {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .summary span {
    color: var(--muted);
    font-size: 12px;
  }
  .wide {
    grid-column: 1/-1;
  }
  .warning {
    color: var(--danger);
  }
  .actions {
    position: sticky;
    bottom: 0;
    z-index: 1;
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 24px;
    padding: 12px 0;
    border-top: 1px solid var(--dldr-color-ui-line);
    background: var(--dldr-color-ui-surface);
  }
  .selected-works { list-style: none; margin: 16px 0; padding: 0; max-height: 240px; overflow-y: auto; }
  .selected-works li { display: flex; align-items: center; gap: 12px; padding: 8px 0; border-bottom: 1px solid var(--dldr-color-ui-line); }
  .selected-works img { width: 72px; height: 48px; object-fit: cover; border-radius: 4px; }
  .selected-works div { min-width: 0; overflow-wrap: anywhere; }
  .selected-works span { display: block; margin-top: 4px; color: var(--muted); font-size: 12px; }
</style>
