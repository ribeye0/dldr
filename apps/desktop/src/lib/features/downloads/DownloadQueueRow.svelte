<script lang="ts">
  import { t, locale } from "$lib/i18n";
  import JobDetails from "$lib/features/logs/JobDetails.svelte";
  import type { JobSnapshot } from "$lib/model/types";
  import {
    downloadQueueProgressPercent,
    jobLabel,
    isActiveJob,
  } from "$lib/utils/jobs";
  import Button from "$lib/components/ui/Button.svelte";
  import Progress from "$lib/components/ui/Progress.svelte";
  import Modal from "$lib/components/ui/Modal.svelte";
  import DownloadJobProgress from "./DownloadJobProgress.svelte";
  import { downloadMetrics } from "$lib/utils/download-progress";
  let {
    job,
    title,
    detail,
    onCancel,
  }: {
    job: JobSnapshot;
    title: string;
    detail: string;
    onCancel: (job: JobSnapshot) => void;
  } = $props();
  let expanded = $state(false);
  const percent = $derived(downloadQueueProgressPercent(job));
  const disabled = $derived(
    !job.cancellable || job.status === "cancelling" || !isActiveJob(job),
  );
</script>

<article
  class="download-queue-row dldr:border-0 dldr:border-b dldr:border-solid dldr:border-ui-line dldr:px-4 dldr:py-4"
>
  <p class="dldr:mt-0 dldr:mb-2 dldr:text-xs dldr:text-ui-dim">{jobLabel(job, $locale)}</p>
  <div class="dldr:mb-3 dldr:flex dldr:items-start dldr:justify-between dldr:gap-3">
    <button
      type="button"
      onclick={() => (expanded = true)}
      class="dldr:min-w-0 dldr:wrap-anywhere dldr:border-0 dldr:bg-transparent dldr:p-0 dldr:text-left dldr:font-[inherit] dldr:font-semibold dldr:text-ui-ink dldr:ui-focus-text"
      >{title}</button
    >
    <Button variant="text" tone="muted" {disabled} onclick={() => onCancel(job)}
      >{$t("Cancel")}</Button
    >
  </div>
  {#if downloadMetrics(job)}
    <DownloadJobProgress {job} />
  {:else if job.status !== "queued"}<Progress
      value={percent}
      label={$t("{0} progress", title)}
    />
    <div
      class="dldr:mt-2 dldr:flex dldr:justify-between dldr:gap-3 dldr:text-xs dldr:text-ui-dim"
    >
      <span class="dldr:wrap-anywhere">{detail}</span>{#if percent !== null}<span
          >{percent}%</span
        >{/if}
    </div>{/if}
</article>
{#if expanded}<Modal
    title={$t("Download details")}
    onClose={() => (expanded = false)}
  >
    <JobDetails {job} {title} {detail} />
    {#snippet actions()}<Button {disabled} onclick={() => onCancel(job)}
        >{$t("Cancel download")}</Button
      >{/snippet}
  </Modal>{/if}
