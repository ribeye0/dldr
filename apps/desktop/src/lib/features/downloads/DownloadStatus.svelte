<script lang="ts">
  import { t, locale } from "$lib/i18n";
  import type { JobSnapshot } from "$lib/model/types";
  import type { DownloadQueueController } from "$lib/controllers/download-queue-controller.svelte";
  import { downloadCenterSummary } from "$lib/utils/download-center";
  import { rateLabel } from "$lib/utils/download-progress";
  import Icon from "$lib/components/ui/Icon.svelte";
  import Progress from "$lib/components/ui/Progress.svelte";
  let { queue, jobs, open, onToggle }: { queue: DownloadQueueController; jobs: JobSnapshot[]; open: boolean; onToggle: () => void } = $props();
  const summary = $derived(downloadCenterSummary(queue.entries, jobs, queue.now));
</script>
<button type="button" class="download-status" class:attention={summary.failed || queue.loadError || queue.actionError}
  aria-label={$t("Download progress")} aria-haspopup="dialog" aria-expanded={open} aria-controls="download-progress-panel" onclick={onToggle}>
  <span class="status-title"><Icon name="downloads" /><strong>{$t("Downloads navigation")}</strong><span class="status-toggle" aria-hidden="true">{open ? "−" : "+"}</span></span>
  {#if queue.loading}<span>{$t("Loading…")}</span>
  {:else if summary.running || summary.queued}
    <span class="status-count">{$t("{0} active · {1} queued", summary.running, summary.queued)}</span>
    <Progress compact value={summary.percent} label={$t("Overall work progress")} />
    <span class="status-rate"><span>{rateLabel(summary.rate, $locale)}</span>{#if summary.percent !== null}<b>{summary.percent}%</b>{/if}</span>
  {:else}<span>{$t("No active downloads")}</span>{/if}
  {#if summary.paused}<span>{$t("{0} paused", summary.paused)}</span>{/if}
  {#if summary.failed}<span class="status-warning">{$t("{0} failed · review downloads", summary.failed)}</span>{/if}
  {#if queue.loadError || queue.actionError}<span class="status-warning">{$t("Download queue needs attention")}</span>{/if}
</button>
<style>
  .download-status { display: grid; gap: 9px; width: 100%; min-width: 0; padding: 12px 10px; border: 1px solid var(--dldr-color-ui-selected-border); border-radius: 9px; color: var(--dldr-color-ui-ink); background: color-mix(in srgb, var(--dldr-color-brand-sky) 10%, transparent); font: inherit; text-align: left; cursor: pointer; }
  .download-status:hover, .download-status[aria-expanded=true] { background: var(--dldr-color-brand-blue); }
  .download-status:focus-visible { outline: 2px solid var(--dldr-color-brand-white); outline-offset: 2px; }
  .status-title, .status-rate { display: flex; align-items: center; justify-content: space-between; gap: 7px; }
  .status-title strong { flex: 1; font-size: 14px; }
  .status-title :global(svg) { width: 18px; height: 18px; }
  .status-toggle { display: grid; place-items: center; flex-shrink: 0; width: 28px; height: 28px; font-size: 28px; font-weight: 600; line-height: 1; }
  .download-status > span:not(.status-title) { font-size: 12px; line-height: 1.5; color: var(--dldr-color-ui-dim); overflow-wrap: anywhere; }
  .status-rate { font-variant-numeric: tabular-nums; }
  .attention { border-color: var(--dldr-color-brand-pink); }
  .download-status .status-warning { color: #ffb6d0; }
</style>
