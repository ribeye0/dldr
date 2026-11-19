<script lang="ts">
  import { t } from "$lib/i18n";
  import Button from "./Button.svelte";
  import Icon from "./Icon.svelte";
  import { onMount } from "svelte";
  import { hasOpenModal } from "./modal-stack";
  import { installFilterBackNavigation } from "$lib/utils/filter-history.svelte";
  let { visible = false, disabled = false, onBack }: {
    visible?: boolean; disabled?: boolean; onBack: () => void;
  } = $props();
  onMount(() => installFilterBackNavigation(window, () => visible && !disabled && !hasOpenModal(), onBack));
</script>

{#if visible}
  <nav class="filter-back" aria-label={$t("Filter navigation")}>
    <Button {disabled} onclick={onBack} aria-keyshortcuts="Alt+ArrowLeft" title={$t("Alt+← or the mouse Back button")}><span class="back-icon"><Icon name="arrow" /></span>{$t("Back to previous results")}</Button>
    <span>{$t("Restore the previous filters, page and reading position.")}</span>
  </nav>
{/if}

<style>
  .filter-back { display:flex; flex-wrap:wrap; align-items:center; gap:8px 14px; margin-bottom:20px; }
  .filter-back > span { color:var(--dldr-color-ui-dim); font-size:13px; line-height:1.5; }
  .back-icon { display:flex; transform:rotate(180deg); }
</style>
