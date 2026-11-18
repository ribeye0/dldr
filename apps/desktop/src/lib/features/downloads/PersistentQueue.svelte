<script lang="ts">
  import { t, locale } from "$lib/i18n";
  import Button from "$lib/components/ui/Button.svelte";
  import Progress from "$lib/components/ui/Progress.svelte";
  import * as api from "$lib/api/tauri";
  import type { DownloadQueueController } from "$lib/controllers/download-queue-controller.svelte";
  import type { JobSnapshot } from "$lib/model/types";
  import { activeDownloadWorks } from "$lib/utils/download-center";
  import { progressPercent, rateLabel, workBytesLabel, workPhaseLabel, workRate } from "$lib/utils/download-progress";
  let { queue, jobs = [], onChanged }: { queue: DownloadQueueController; jobs?: JobSnapshot[]; onChanged: () => Promise<unknown> } = $props();
  let filter = $state("unfinished");
  const visible = $derived(queue.entries.filter(e => filter === "all" || (filter === "unfinished" ? !["completed","cancelled"].includes(e.status) : e.status === filter)));
  const works = $derived(activeDownloadWorks(jobs));
  const labels: Record<string,string> = { queued: "等待中", running: "下載中", pausing: "正在暫停…", cancelling: "正在取消…", paused: "已暫停", cancelled: "已取消", failed: "失敗", completed: "已完成" };
  const run = (action: () => Promise<unknown>) => queue.run(action, onChanged);
</script>
<section class="queue" aria-label={$t("Saved download queue")}>
  <div class="tools">
    <Button disabled={queue.loading || queue.busy || !queue.entries.some(e => ["queued","running"].includes(e.status))} onclick={() => run(() => queue.pauseAll())}>{$t("Pause all")}</Button>
    <Button disabled={queue.loading || queue.busy || !queue.entries.some(e => e.status === "paused")} onclick={() => run(() => api.controlDownloadQueue("", "resumeAll"))}>{$t("繼續全部暫停項目")}</Button>
    <Button disabled={queue.loading || queue.busy || !queue.entries.some(e => e.status === "failed")} onclick={() => run(api.retryFailedDownloads)}>{$t("重試全部失敗項目")}</Button>
    <Button disabled={queue.loading || queue.busy || !queue.entries.some(e => ["completed","cancelled"].includes(e.status))} onclick={() => run(api.clearDownloadQueue)}>{$t("清除已完成／取消")}</Button>
  </div>
  <label class="queue-filter">{$t("顯示")}
    <select aria-label={$t("Saved queue status")} bind:value={filter}>
      <option value="unfinished">{$t("未完成")}</option><option value="all">{$t("全部")}</option><option value="paused">{$t("已暫停")}</option><option value="failed">{$t("失敗")}</option>
    </select>
  </label>
  {#if queue.loading}<p role="status">{$t("Loading…")}</p>{:else if !visible.length}<p>{$t("沒有符合的下載項目。")}</p>{/if}
  <div class="entries">
  {#each visible as entry (entry.workId)}
    {@const work = works.get(entry.workId)}
    <article class="entry" aria-label={$t("Queue {0}", entry.workId)}>
      {#if entry.thumbnailUrl}<img src={entry.thumbnailUrl} alt="" loading="lazy" />{/if}
      <div class="info"><strong>{entry.title || entry.workId}</strong><span>{entry.workId} · {work && entry.status === "running" ? workPhaseLabel(work.phase, $locale) : $t(labels[entry.status] ?? entry.status)}</span>
        {#if work && ["running","pausing","cancelling"].includes(entry.status)}
          <Progress value={progressPercent(work.bytesReceived, work.bytesTotal)} label={$t("{0} file progress", entry.workId)} />
          <div class="transfer"><span>{workBytesLabel(work, $locale)}</span><b>{rateLabel(workRate(work, queue.now, entry.status === "running"), $locale)}</b></div>
        {:else if ["queued","running","pausing","cancelling"].includes(entry.status)}
          <Progress value={null} label={$t("{0} progress", entry.title || entry.workId)} />
        {/if}
        {#if entry.errorMessage}<small class:error={entry.status === "failed"}>{entry.errorMessage}</small>{/if}
      </div>
      <div class="actions">
        {#if ["queued","running"].includes(entry.status)}
          <Button disabled={queue.busy} onclick={() => run(() => api.controlDownloadQueue(entry.workId,"pause"))}>{$t("暫停")}</Button>
          <Button disabled={queue.busy} onclick={() => run(() => api.controlDownloadQueue(entry.workId,"cancel"))}>{$t("取消")}</Button>
        {:else if ["paused","failed","cancelled"].includes(entry.status)}
          <Button disabled={queue.busy} onclick={() => run(() => api.controlDownloadQueue(entry.workId,"resume"))}>{entry.status === "failed" ? $t("重試") : $t("繼續")}</Button>
        {/if}
      </div>
    </article>
  {/each}
  </div>
</section>
<style>
  .tools,.actions,.transfer { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; }
  .queue-filter { display: flex; align-items: center; gap: 8px; font-size: 12px; margin: 18px 0 10px; }
  select { background: var(--dldr-color-ui-surface); color: inherit; padding: 6px; border: 1px solid var(--dldr-color-ui-line); border-radius: 4px; font: inherit; }
  p,span,small { font-size: 12px; color: var(--dldr-color-ui-dim); }
  .entry { display: flex; align-items: flex-start; gap: 12px; padding: 16px 0; border-top: 1px solid var(--dldr-color-ui-line); }
  .entry img { width: 72px; aspect-ratio: 3/2; object-fit: cover; border-radius: 5px; }
  .info { display: flex; flex: 1; min-width: 0; flex-direction: column; gap: 8px; overflow-wrap: anywhere; }
  .info > strong { font-size: 14px; }
  .transfer { justify-content: space-between; font-size: 12px; font-variant-numeric: tabular-nums; }
  .transfer b { color: var(--dldr-color-ui-accent); }
  .error { color: var(--dldr-color-ui-error); }
  @media(max-width:600px) { .entry { flex-wrap: wrap; } .entry img { width: 60px; } .actions { width: 100%; justify-content: flex-end; } }
</style>
