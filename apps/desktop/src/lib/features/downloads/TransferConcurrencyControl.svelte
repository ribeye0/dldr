<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "$lib/i18n";
  import Button from "$lib/components/ui/Button.svelte";
  import * as api from "$lib/api/tauri";
  import { errorMessage } from "$lib/utils/format";
  let limit = $state(2), loading = $state(true), saving = $state(false), loaded = $state(false);
  let error = $state("");
  let disposed = false;
  async function load() {
    loading = true; error = "";
    try { const next = await api.getTransferSettings(); if (!disposed) { limit = next; loaded = true; } }
    catch (cause) { if (!disposed) error = errorMessage(cause); }
    finally { if (!disposed) loading = false; }
  }
  async function apply(event: SubmitEvent) {
    event.preventDefault();
    if (!loaded || saving) return;
    saving = true; error = "";
    try { await api.setTransferConcurrency(limit); }
    catch (cause) { if (!disposed) error = errorMessage(cause); }
    finally { if (!disposed) saving = false; }
  }
  onMount(() => { void load(); return () => { disposed = true; }; });
</script>
<form class="concurrency-control" aria-label={$t("Parallel downloads")} onsubmit={apply}>
  <div class="fields">
    <label for="transfer-concurrency">{$t("全域同時下載上限")}</label>
    <select id="transfer-concurrency" aria-label={$t("Global simultaneous works")} bind:value={limit} disabled={loading || saving || !loaded}>
      {#each [1,2,3,4] as value}<option {value}>{value}</option>{/each}
    </select>
    <Button type="submit" disabled={loading || saving || !loaded}>{saving ? $t("Applying…") : $t("套用上限")}</Button>
  </div>
  <p>{$t("上限共用於所有批次及單部下載。降低上限後，現有下載先完成；程式重啟後，未完成項目會保留為暫停。")}</p>
  {#if error}<div role="alert">{error}{#if !loaded}<Button disabled={loading} onclick={load}>{$t("Retry")}</Button>{/if}</div>{/if}
</form>
<style>
  .concurrency-control { margin: 0 0 20px; padding: 16px; border: 1px solid var(--dldr-color-ui-line); border-radius: 8px; background: var(--dldr-color-ui-surface); }
  .fields { display: flex; flex-wrap: wrap; align-items: center; gap: 12px; font-size: var(--dldr-settings-body-size, 12px); }
  select { background: var(--dldr-color-ui-surface); color: inherit; padding: 8px; border: 1px solid var(--dldr-color-ui-line); border-radius: 4px; font: inherit; }
  p { margin: 12px 0 0; font-size: var(--dldr-settings-detail-size, 12px); color: var(--dldr-color-ui-dim); }
  [role=alert] { display: flex; flex-wrap: wrap; align-items: center; gap: 12px; margin-top: 12px; color: var(--dldr-color-ui-error); }
</style>
