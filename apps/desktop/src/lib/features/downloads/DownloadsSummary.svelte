<script lang="ts">
  import { t, locale } from "$lib/i18n";
  import { onMount } from "svelte";
  import type { JobSnapshot } from "$lib/model/types";
  import Progress from "$lib/components/ui/Progress.svelte";
  import { rateLabel } from "$lib/utils/download-progress";
  import type { DownloadQueueEntry } from "$lib/api/tauri";
  import { downloadCenterSummary } from "$lib/utils/download-center";
  import { formatBytes } from "$lib/utils/format";
  let { jobs, entries = [] }: { jobs: JobSnapshot[]; entries?: DownloadQueueEntry[] } = $props();
  let now = $state(Date.now());
  onMount(() => {
    const timer = setInterval(() => { now = Date.now(); }, 500);
    return () => clearInterval(timer);
  });
  const summary = $derived(downloadCenterSummary(entries, jobs, now));
</script>

{#if jobs.length || entries.length}
  <section class="downloads-summary" aria-label={$t("Overall download progress")}>
    <div class="summary-heading"><strong>{$t("Overall progress")}</strong><span>{summary.completed} / {summary.total} {$t("works processed")}</span></div>
    <Progress value={summary.percent} label={$t("Overall work progress")} />
    <div class="summary-stats">
      <div><span>{$t("Total download speed")}</span><strong>{rateLabel(summary.rate, $locale)}</strong></div>
      <div><span>{$t("Current works")}</span><strong>{summary.activeWorks}</strong></div>
      {#if jobs.length}<div><span>{$t("Downloaded data, including resumed files")}</span><strong>{formatBytes(summary.bytes)}</strong></div>{/if}
      <div><span>{$t("Failed works")}</span><strong>{summary.failed}</strong></div>
    </div>
  </section>
{/if}

<style>
  .downloads-summary { display: grid; gap: 16px; margin: 0 0 24px; padding: 18px; border: 1px solid var(--dldr-color-ui-selected-border); border-radius: 10px; background: var(--dldr-color-ui-surface); }
  .summary-heading { display: flex; flex-wrap: wrap; justify-content: space-between; gap: 8px; font-size: 14px; }
  .summary-heading span { font-size: 12px; color: var(--dldr-color-ui-dim); }
  .summary-stats { display: grid; grid-template-columns: repeat(auto-fit, minmax(140px, 1fr)); gap: 16px; }
  .summary-stats div { display: grid; gap: 6px; }
  .summary-stats span { font-size: 11px; color: var(--dldr-color-ui-dim); }
  .summary-stats strong { font-size: 16px; color: var(--dldr-color-ui-accent); font-variant-numeric: tabular-nums; }
</style>
