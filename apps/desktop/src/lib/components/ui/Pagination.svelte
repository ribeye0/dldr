<script lang="ts">
  import { tick } from "svelte";
  import { t } from "$lib/i18n";
  import Button from "./Button.svelte";
  let { page = 1, pageCount = 1, loading = false, label, bottom = false, scrollTarget, onChange }: {
    page?: number;
    pageCount?: number;
    loading?: boolean;
    label: string;
    bottom?: boolean;
    scrollTarget?: HTMLElement;
    onChange: (page: number) => void | Promise<void>;
  } = $props();
  let requestedPage = $state<number | undefined>(1);
  $effect(() => { requestedPage = Math.min(page, pageCount); });
  const pages = $derived([...new Set([1, ...Array.from({length: Math.min(5, pageCount)}, (_, i) =>
    Math.max(1, Math.min(page - 2, pageCount - 4)) + i), pageCount])].sort((a, b) => a - b));
  const validJump = $derived(requestedPage !== undefined && Number.isInteger(requestedPage) && requestedPage >= 1 && requestedPage <= pageCount && requestedPage !== page);

  async function change(next: number) {
    if (loading || !Number.isInteger(next) || next < 1 || next > pageCount || next === page) return;
    await onChange(next);
    if (scrollTarget) {
      await tick();
      scrollTarget.focus({preventScroll: true});
      scrollTarget.scrollIntoView({block: "start", behavior: matchMedia("(prefers-reduced-motion: reduce)").matches ? "instant" : "smooth"});
    }
  }
</script>

<nav class="pagination" class:bottom aria-label={bottom ? $t("{0} at bottom", label) : label}>
  <span class="page-status" aria-live="polite">{$t("Page {0} of {1}", page, pageCount)}</span>
  <div class="page-buttons">
    <Button disabled={loading || page <= 1} onclick={() => change(page - 1)}>{$t("Previous")}</Button>
    {#each pages as number, index (number)}
      {#if index > 0 && number > pages[index - 1] + 1}<span aria-hidden="true">…</span>{/if}
      <Button variant={page === number ? "primary" : "secondary"}
        aria-current={page === number ? "page" : undefined}
        aria-label={$t("Go to page {0}", number)}
        disabled={loading || page === number} onclick={() => change(number)}>{number}</Button>
    {/each}
    <Button disabled={loading || page >= pageCount} onclick={() => change(page + 1)}>{$t("Next")}</Button>
  </div>
  <form onsubmit={event => {event.preventDefault(); if (validJump) void change(requestedPage!);}}>
    <label>{$t("Page number")}<input type="number" min="1" max={pageCount} step="1" required inputmode="numeric"
      bind:value={requestedPage} disabled={loading || pageCount <= 1} /></label>
    <Button type="submit" disabled={loading || !validJump}>{$t("Go")}</Button>
  </form>
</nav>

<style>
  .pagination, .page-buttons, form, label { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; min-width: 0; }
  .pagination { justify-content: flex-end; color: var(--dldr-color-ui-dim); font-size: 14px; }
  .bottom { justify-content: center; margin-top: 28px; padding-top: 20px; border-top: 1px solid var(--dldr-color-ui-line-subtle); }
  .page-status { font-variant-numeric: tabular-nums; }
  .page-buttons { gap: 5px; }
  .pagination :global(button) { min-width: 32px; justify-content: center; font-size: 14px; }
  .pagination :global(button[aria-current="page"]) {
    opacity: 1;
    background: var(--dldr-color-brand-blue);
    color: var(--dldr-color-brand-white);
  }
  label { gap: 6px; }
  input { box-sizing: border-box; width: 76px; padding: 7px 8px; border: 1px solid var(--dldr-color-ui-line-strong); border-radius: 6px; background: var(--dldr-color-ui-input); color: var(--dldr-color-ui-ink); font: inherit; }
  input:focus-visible { outline: 2px solid var(--dldr-color-ui-focus); outline-offset: 2px; }
  @media (max-width: 620px) { .pagination { justify-content: center; width: 100%; } }
</style>
