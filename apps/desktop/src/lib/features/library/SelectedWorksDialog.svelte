<script lang="ts">
  import { t } from "$lib/i18n";
  import Modal from "$lib/components/ui/Modal.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import type { SelectedProduct } from "$lib/model/types";
  import { formatBytes } from "$lib/utils/format";

  let { products, disabled = false, onRemove, onClose, onDownload }: {
    products: SelectedProduct[];
    disabled?: boolean;
    onRemove: (workId: string) => void;
    onClose: () => void;
    onDownload: () => void;
  } = $props();
</script>

<Modal title={$t("Selected works ({0})", products.length)} {onClose} wide>
  <p class="dldr:mt-0 dldr:text-sm dldr:text-ui-dim">
    {$t("Selections are kept across searches, pages and app restarts. Review download size before starting.")}
  </p>
  <ul class="dldr:m-0 dldr:list-none dldr:p-0" aria-label={$t("Selected works")}>
    {#each products as product (product.workId)}
      <li class="dldr:flex dldr:items-center dldr:gap-3 dldr:border-0 dldr:border-b dldr:border-solid dldr:border-ui-line dldr:py-3">
        {#if product.thumbnailUrl}
          <img src={product.thumbnailUrl} alt="" loading="lazy" class="dldr:h-14 dldr:w-20 dldr:rounded dldr:object-cover" />
        {/if}
        <div class="dldr:min-w-0 dldr:flex-1 dldr:wrap-anywhere">
          <strong class="dldr:text-base">{product.title}</strong>
          <p class="dldr:my-1 dldr:text-sm dldr:text-ui-dim">{product.workId} · {product.makerName ?? $t("Unknown maker")}</p>
          <p class="dldr:my-1 dldr:text-sm dldr:text-ui-accent">{typeof product.contentSizeBytes === "number" ? formatBytes(product.contentSizeBytes) : $t("Size unknown")}</p>
        </div>
        <Button aria-label={$t("Remove {0} from selection", product.workId)} onclick={() => onRemove(product.workId)}>{$t("Remove")}</Button>
      </li>
    {/each}
  </ul>
  {#snippet actions()}
    <Button onclick={onClose}>{$t("Close")}</Button>
    <Button variant="primary" disabled={disabled || !products.length} onclick={onDownload}>{$t("Review download size")}</Button>
  {/snippet}
</Modal>
