<script lang="ts">
  import { t, locale } from "$lib/i18n";
  import type { JobSnapshot } from "$lib/model/types";
  import { bulkDownloadResult, jobLabel } from "$lib/utils/jobs";
  import { detailDate } from "$lib/utils/format";
  import Disclosure from "$lib/components/ui/Disclosure.svelte";
  import DownloadJobProgress from "$lib/features/downloads/DownloadJobProgress.svelte";
  let {
    job,
    title,
    detail,
  }: { job: JobSnapshot; title: string; detail: string } = $props();
  const bulk = $derived(bulkDownloadResult(job));
</script>

<h3 class="dldr:mt-0 dldr:wrap-anywhere">{title}</h3>
<p>{jobLabel(job, $locale)}</p>
<p class="dldr:wrap-anywhere">{detail}</p>
<DownloadJobProgress {job} />
{#if job.error}<p class="dldr:wrap-anywhere dldr:text-ui-error">
    {job.error.code} · {job.error.message}
  </p>{/if}
{#if bulk.failedWorks.length}<Disclosure
    title={$t("Failed works ({0})", bulk.failedWorks.length)}
    open
  >
    <ul class="dldr:space-y-3 dldr:pl-4">
      {#each bulk.failedWorks as work}<li class="dldr:wrap-anywhere">
          <strong>{work.workId}</strong>
          <p class="dldr:m-0 dldr:text-ui-error">
            {work.errorMessage ?? work.errorCode ?? $t("Failed")}
          </p>
        </li>{/each}
    </ul>
  </Disclosure>{/if}
{#if bulk.succeededWorks.length}<Disclosure
    title={$t("Downloaded works ({0})", bulk.succeededWorks.length)}
  >
    <ul class="dldr:space-y-3 dldr:pl-4">
      {#each bulk.succeededWorks as work}<li class="dldr:wrap-anywhere">
          <strong>{work.workId}</strong>
          <p class="dldr:m-0 dldr:text-ui-dim">{work.localPath ?? "—"}</p>
        </li>{/each}
    </ul>
  </Disclosure>{/if}
<Disclosure title={$t("Job details")}
  ><dl class="dldr:grid dldr:grid-cols-[auto_minmax(0,1fr)] dldr:gap-3 dldr:text-xs">
    <dt>{$t("Created")}</dt>
    <dd class="dldr:m-0">{detailDate(job.createdAt)}</dd>
    <dt>{$t("Started")}</dt>
    <dd class="dldr:m-0">{detailDate(job.startedAt)}</dd>
    <dt>{$t("Finished")}</dt>
    <dd class="dldr:m-0">{detailDate(job.finishedAt)}</dd>
  </dl>
  {#if job.output}<pre
      class="dldr:whitespace-pre-wrap dldr:wrap-anywhere dldr:text-xs">{JSON.stringify(
        job.output,
        null,
        2,
      )}</pre>{/if}
</Disclosure>
