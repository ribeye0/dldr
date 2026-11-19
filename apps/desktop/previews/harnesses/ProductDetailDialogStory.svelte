<script lang="ts">
  import ProductDetailDialog from "$lib/features/library/ProductDetailDialog.svelte";
  import ToastStack from "$lib/components/ui/ToastStack.svelte";
  import type { Toast, ProductCreditField, ProductDetail } from "$lib/model/types";

  let { detail }: { detail: ProductDetail } = $props();

  let lastAction = $state("Detail open");
  let toasts = $state<Toast[]>([]);
  let sequence = 0;
  function copied(message: string) {
    toasts = [{ id: `copy-${sequence++}`, kind: "success" as const, message }, ...toasts].slice(0, 5);
  }

  function copyText(label: string, value: string | null, workId: string) {
    lastAction = `Copy ${label} for ${workId}: ${value ?? "-"}`;
    copied(lastAction);
  }

  function copyCredit(field: ProductCreditField, workId: string) {
    lastAction = `Copy ${field.label} for ${workId}`;
    copied(lastAction);
  }
</script>

<ProductDetailDialog
  {detail}
  onClose={() => (lastAction = "Close detail")}
  onPreview={() => (lastAction = `Preview ${detail.workId}`)}
  onCopyText={copyText}
  onCopyWorkId={() => { lastAction = `Copy ${detail.workId}`; copied(lastAction); }}
  onCopyCredit={copyCredit}
  onOpenDlsite={() => (lastAction = `Open ${detail.workId} on DLsite`)}

/>

<ToastStack {toasts} onDismiss={(id) => { toasts = toasts.filter(toast => toast.id !== id); }} />

<p aria-live="polite">{lastAction}</p>

<style>
  p {
    position: fixed;
    z-index: 55;
    right: 14px;
    bottom: 10px;
    max-width: calc(100vw - 28px);
    margin: 0;
    color: var(--text-subtle);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
