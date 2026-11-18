<script lang="ts">
  import { onMount } from "svelte";
  import { t } from "$lib/i18n";
  import type { JobSnapshot } from "$lib/model/types";
  import type { DownloadQueueController } from "$lib/controllers/download-queue-controller.svelte";
  import { isActiveJob } from "$lib/utils/jobs";
  import Button from "$lib/components/ui/Button.svelte";
  import Icon from "$lib/components/ui/Icon.svelte";
  import DownloadsSummary from "./DownloadsSummary.svelte";
  import PersistentQueue from "./PersistentQueue.svelte";
  import Row from "./DownloadQueueRow.svelte";
  let { queue, jobs, summaryJobs, getTitle, getDetail, onReload, onCancel, onClose, onSettings, onLogs }: {
    queue: DownloadQueueController; jobs: JobSnapshot[]; summaryJobs: JobSnapshot[];
    getTitle: (job: JobSnapshot) => string; getDetail: (job: JobSnapshot) => string;
    onReload: () => Promise<unknown>; onCancel: (job: JobSnapshot) => void;
    onClose: () => void; onSettings: () => void; onLogs: () => void;
  } = $props();
  let heading: HTMLHeadingElement;
  onMount(() => heading?.focus());
</script>
<div id="download-progress-panel" class="download-panel" role="dialog" aria-modal="false" aria-labelledby="download-progress-title"
  tabindex="-1" onkeydown={(event) => { if (event.key === "Escape") { event.stopPropagation(); onClose(); } }}>
  <header>
    <div><h2 id="download-progress-title" bind:this={heading} tabindex="-1">{$t("Download progress")}</h2><p>{$t("Closing this panel keeps downloads running.")}</p></div>
    <Button variant="text" aria-label={$t("Close download progress")} onclick={onClose}><Icon name="close" /></Button>
  </header>
  <div class="panel-content">
    <DownloadsSummary jobs={summaryJobs} entries={queue.entries} />
    {#if queue.loadError || queue.actionError}<div role="alert" class="queue-error">
      <p>{queue.actionError || queue.loadError}</p><Button onclick={() => queue.reload()}>{$t("Reload")}</Button>
    </div>{/if}
    <PersistentQueue {queue} {jobs} onChanged={onReload} />
    {#if jobs.some(isActiveJob)}
      <details class="job-details" open={!queue.entries.length}>
        <summary>{$t("Download tasks")}</summary>
        {#each jobs.filter(isActiveJob) as job (job.id)}<Row {job} title={getTitle(job)} detail={getDetail(job)} {onCancel} />{/each}
      </details>
    {/if}
  </div>
  <footer><Button onclick={onSettings}><Icon name="settings" />{$t("Download settings")}</Button><Button onclick={onLogs}>Logs</Button>
    <Button onclick={async () => { await queue.reload(); await onReload(); }}>{$t("Reload")}</Button></footer>
</div>
<style>
  .download-panel { position: fixed; z-index: 30; left: 208px; bottom: 16px; width: min(760px, calc(100vw - 224px)); max-height: min(800px, calc(100dvh - 32px)); display: flex; flex-direction: column; border: 1px solid var(--dldr-color-ui-line-strong); border-radius: 14px; background: var(--dldr-color-ui-surface); color: var(--dldr-color-ui-ink); box-shadow: 0 12px 48px var(--dldr-color-ui-shadow-strong); overflow: hidden; }
  header { display: flex; justify-content: space-between; align-items: flex-start; gap: 12px; padding: 18px 20px; border-bottom: 1px solid var(--dldr-color-ui-line); }
  h2 { margin: 0; font-size: 20px; outline: none; }
  header p { margin: 6px 0 0; font-size: 12px; color: var(--dldr-color-ui-dim); }
  .panel-content { min-height: 0; overflow-y: auto; overscroll-behavior: contain; padding: 20px; }
  footer { display: flex; flex-wrap: wrap; gap: 8px; padding: 12px 20px; border-top: 1px solid var(--dldr-color-ui-line); }
  footer :global(button:last-child) { margin-left: auto; }
  .queue-error { margin-bottom: 16px; color: var(--dldr-color-ui-error); white-space: pre-wrap; overflow-wrap: anywhere; }
  .job-details { margin-top: 16px; font-size: 13px; }
  summary { cursor: pointer; padding: 8px 0; }
  @media(max-width:620px) { .download-panel { left: 12px; bottom: 12px; width: calc(100vw - 24px); max-height: calc(100dvh - 24px); } header, footer { padding: 14px; } .panel-content { padding: 14px; } }
</style>
