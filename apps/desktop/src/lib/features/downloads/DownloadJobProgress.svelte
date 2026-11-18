<script lang="ts">
  import { t, locale } from "$lib/i18n";
  import { onMount } from "svelte";
  import type { JobSnapshot } from "$lib/model/types";
  import Progress from "$lib/components/ui/Progress.svelte";
  import { isActiveJob } from "$lib/utils/jobs";
  import { metadataNumber } from "$lib/utils/jobs";
  import { downloadMetrics, progressPercent, rateLabel, workBytesLabel, workPhaseLabel, workRate } from "$lib/utils/download-progress";

  let { job }: { job: JobSnapshot } = $props();
  let now = $state(Date.now());
  onMount(() => {
    const timer = setInterval(() => { now = Date.now(); }, 500);
    return () => clearInterval(timer);
  });
  const metrics = $derived(downloadMetrics(job));
</script>

{#if metrics}
  <div class="download-progress" aria-label={$t("Detailed download progress")}>
    {#if job.kind === "bulkWorkDownload"}
      <div class="progress-heading">
        <strong>{$t("Batch progress")}</strong>
        <span>{metrics.completedWorks} / {metrics.totalWorks} {$t("works processed")}</span>
      </div>
      <Progress value={progressPercent(metrics.completedWorks, metrics.totalWorks)} label={$t("Batch work progress")} />
      <p class="progress-note">{metrics.succeededWorks} {$t("downloaded ·")} {metrics.failedWorks} {$t("failed")}{#if metrics.deferredWorks} · {metrics.deferredWorks} {$t("paused/cancelled individually")}{/if}</p>
      {#if metadataNumber(job, "maxConcurrentWorks")}<p class="progress-note">{$t("Up to")} {metadataNumber(job, "maxConcurrentWorks")} {$t("simultaneous works ·")} {isActiveJob(job) ? metrics.activeWorks.length : 0} {$t("active")}</p>{/if}
    {/if}
    {#each metrics.activeWorks as work (work.workId)}
      <section class="current-work" aria-label={$t("Current work {0}", work.workId)}>
        <div class="progress-heading"><strong>{work.title}</strong><span>{workPhaseLabel(work.phase, $locale)}</span></div>
        <p class="progress-note">{work.workId}</p>
        <Progress value={progressPercent(work.bytesReceived, work.bytesTotal)} label={$t("{0} file progress", work.workId)} />
        <div class="transfer-details">
          <span>{workBytesLabel(work, $locale)}</span>
          <strong class="rate" aria-label={$t("{0} download speed", work.workId)}>{rateLabel(workRate(work, now, isActiveJob(job)), $locale)}</strong>
          {#if progressPercent(work.bytesReceived, work.bytesTotal) !== null}<span>{progressPercent(work.bytesReceived, work.bytesTotal)}%</span>{/if}
        </div>
      </section>
    {/each}
  </div>
{/if}

<style>
  .download-progress { display: grid; gap: 10px; }
  .progress-heading, .transfer-details { display: flex; align-items: baseline; flex-wrap: wrap; justify-content: space-between; gap: 8px 16px; font-size: 12px; }
  .progress-heading strong { min-width: 0; overflow-wrap: anywhere; }
  .progress-heading span, .transfer-details span { color: var(--dldr-color-ui-dim); }
  .progress-note { margin: 0; color: var(--dldr-color-ui-dim); font-size: 11px; }
  .current-work { display: grid; gap: 8px; margin-top: 8px; padding: 14px; border: 1px solid var(--dldr-color-ui-line); border-radius: 8px; background: var(--dldr-color-ui-surface-muted); }
  .rate { color: var(--dldr-color-ui-accent); font-variant-numeric: tabular-nums; white-space: nowrap; }
</style>
