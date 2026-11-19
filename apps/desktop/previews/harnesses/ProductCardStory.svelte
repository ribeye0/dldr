<script lang="ts">
  import { runningDownloadJob } from "../fixtures/jobs";
  import ProductCard from "$lib/features/library/ProductCard.svelte";
  import type { Product } from "$lib/model/types";

  let {
    product,
    downloadDisabled = false,
    detailLoading = false,
  }: {
    product: Product;
    downloadDisabled?: boolean;
    detailLoading?: boolean;
  } = $props();

  let lastAction = $state("No action yet");

  function action(label: string) {
    lastAction = label;
  }


</script>

<main class="story-surface">
  <section class="card-frame">
    <ProductCard
      {product}
      activeJob={downloadDisabled
        ? { ...runningDownloadJob, metadata: { workId: product.workId } }
        : null}
      {detailLoading}
      onOpenDetails={() => action(`Open details for ${product.workId}`)}
      onCopyWorkId={(id) => action(`Copy ${id}`)}
    />
  </section>
  <p aria-live="polite">{lastAction}</p>
</main>

<style>
  .story-surface {
    display: grid;
    gap: 12px;
    width: min(1120px, calc(100vw - 48px));
    margin: 0 auto;
    padding: 24px 0;
  }

  .card-frame {
    min-width: 0;
    border: 1px solid var(--border);
    border-radius: 9px;
    background: var(--panel);
    box-shadow: 0 16px 40px rgb(0 0 0 / 18%);
    overflow: hidden;
  }

  p {
    margin: 0;
    color: var(--text-subtle);
    font-size: 12px;
    text-align: right;
  }

  @media (max-width: 720px) {
    .story-surface {
      width: calc(100vw - 24px);
      padding: 12px 0;
    }
  }
</style>
