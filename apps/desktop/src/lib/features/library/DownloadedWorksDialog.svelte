<script lang="ts">
  import { t } from "$lib/i18n";
  import Modal from "$lib/components/ui/Modal.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import type { SelectedProduct } from "$lib/model/types";
  import type { LocalWorkProgress } from "$lib/utils/local-work-batch";

  let { products, busy, progress, onRemove, onClose, onDelete, onExport }: {
    products: SelectedProduct[];
    busy: boolean;
    progress: LocalWorkProgress | null;
    onRemove: (workId: string) => void;
    onClose: () => void;
    onDelete: () => void;
    onExport: () => void;
  } = $props();
</script>

<Modal title={$t("Selected downloaded works ({0})", products.length)} {onClose} wide>
  <p class="dldr:mt-0 dldr:text-sm dldr:text-ui-dim">
    {$t("Export copies each work folder to your chosen location and keeps the original files. Existing folders are never overwritten.")}
  </p>
  {#if busy && progress}
    <p role="status" class="dldr:text-sm dldr:text-ui-dim">
      {$t("You can close this window. The operation will continue in the background.")}
    </p>
  {/if}
  {#if progress}
    <div role="status" aria-live="polite" class="dldr:mb-4">
      <p>{progress.operation === "delete" ? $t("Deleting works: {0} / {1}", progress.completed, progress.total) : $t("Exporting works: {0} / {1}", progress.completed, progress.total)}</p>
      <progress value={progress.completed} max={progress.total} class="dldr:w-full" aria-label={$t("Batch progress")}></progress>
      {#if progress.completed === progress.total}
        <p>{$t("{0} succeeded, {1} failed", progress.succeeded.length, progress.failures.length)}</p>
      {/if}
    </div>
    {#if progress.failures.length}
      <ul class="dldr:pl-5 dldr:text-sm dldr:text-ui-error" aria-label={$t("Failed works")}>
        {#each progress.failures as failure}
          <li class="dldr:mb-2 dldr:wrap-anywhere"><strong>{failure.workId}</strong>: {failure.message}</li>
        {/each}
      </ul>
    {/if}
  {/if}
  <ul class="dldr:m-0 dldr:list-none dldr:p-0" aria-label={$t("Selected works")}>
    {#each products as product (product.workId)}
      <li class="dldr:flex dldr:items-center dldr:gap-3 dldr:border-0 dldr:border-b dldr:border-solid dldr:border-ui-line dldr:py-3">
        <div class="dldr:min-w-0 dldr:flex-1 dldr:wrap-anywhere">
          <strong>{product.title}</strong>
          <p class="dldr:my-1 dldr:text-sm dldr:text-ui-dim">{product.workId} · {product.makerName ?? $t("Unknown maker")}</p>
        </div>
        <Button disabled={busy} aria-label={$t("Remove {0} from selection", product.workId)} onclick={() => onRemove(product.workId)}>{$t("Remove")}</Button>
      </li>
    {/each}
  </ul>
  {#snippet actions()}
    <Button onclick={onClose}>{$t("Close")}</Button>
    <Button disabled={busy || !products.length} tone="error" onclick={onDelete}>{$t("Delete selected files")}</Button>
    <Button disabled={busy || !products.length} variant="primary" onclick={onExport}>{$t("Export selected folders")}</Button>
  {/snippet}
</Modal>
