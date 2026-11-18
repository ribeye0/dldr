<script lang="ts">
  import { t } from "$lib/i18n";
  import { onMount } from "svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import { getDownloadRateLimit, setDownloadRateLimit } from "$lib/api/tauri";
  import { downloadRateBytes, MIB } from "$lib/utils/download-rate-limit";
  import { errorMessage, formatBytes } from "$lib/utils/format";

  let value = $state<number | undefined>(0);
  let current = $state<number | null>(null);
  let loading = $state(true), saving = $state(false), loaded = $state(false);
  let error = $state("");
  let disposed = false;
  onMount(() => { void load(); return () => { disposed = true; }; });

  async function load() {
    loading = true; error = "";
    try {
      const result = await getDownloadRateLimit();
      if (disposed) return;
      current = result.bytesPerSecond;
      value = current === null ? 0 : current / MIB;
      loaded = true;
    } catch (cause) { if (!disposed) error = errorMessage(cause); }
    finally { if (!disposed) loading = false; }
  }

  async function apply(event: SubmitEvent) {
    event.preventDefault();
    if (!loaded || saving) return;
    error = "";
    try {
      const bytes = downloadRateBytes(value);
      saving = true;
      const result = await setDownloadRateLimit(bytes);
      if (disposed) return;
      current = result.bytesPerSecond;
      value = current === null ? 0 : current / MIB;
    } catch (cause) { if (!disposed) error = errorMessage(cause); }
    finally { if (!disposed) saving = false; }
  }
</script>

<form class="rate-control" aria-label={$t("Download speed limit")} onsubmit={apply}>
  <div class="rate-fields">
    <label for="download-rate-limit">{$t("Total download limit (MiB/s)")}</label>
    <input id="download-rate-limit" type="number" min="0" max="1024" step="any" required
      bind:value disabled={loading || saving || !loaded} />
    <Button type="submit" disabled={loading || saving || !loaded}>{saving ? $t("Applying…") : $t("Apply limit")}</Button>
    <span class="current-limit" role="status">{loading ? $t("Loading limit…") : loaded ? $t("Current limit: {0}", current === null ? $t("Unlimited") : `${formatBytes(current)}/s`) : $t("Current limit unavailable")}</span>
  </div>
  <p class="rate-help">{$t("Shared by all works and batches. Set 0 for unlimited. Applies immediately and is remembered.")}</p>
  {#if error}<div class="rate-error" role="alert">{$t(error)}
    {#if !loaded}<Button disabled={loading} onclick={load}>{$t("Retry rate limit")}</Button>{/if}
  </div>{/if}
</form>

<style>
  .rate-control { margin-bottom: 20px; padding: 16px; border: 1px solid var(--dldr-color-ui-line); border-radius: 8px; background: var(--dldr-color-ui-surface); }
  .rate-fields { display: flex; align-items: center; flex-wrap: wrap; gap: 12px; font-size: var(--dldr-settings-body-size, 12px); }
  input { box-sizing: border-box; width: 110px; min-height: 36px; border: 1px solid var(--dldr-color-ui-line-strong); border-radius: 5px; padding: 8px; color: var(--dldr-color-ui-ink); background: var(--dldr-color-ui-input); font: inherit; }
  input:focus-visible { outline: 2px solid var(--dldr-color-ui-accent); outline-offset: 2px; }
  .current-limit { color: var(--dldr-color-ui-accent); font-variant-numeric: tabular-nums; }
  .rate-help { margin: 12px 0 0; font-size: var(--dldr-settings-detail-size, 11px); line-height: 1.5; color: var(--dldr-color-ui-dim); }
  .rate-error { display: flex; gap: 12px; align-items: center; flex-wrap: wrap; margin-top: 12px; color: var(--dldr-color-ui-error); font-size: var(--dldr-settings-body-size, 12px); }
</style>
