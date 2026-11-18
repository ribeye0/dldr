<script lang="ts">
  import { onMount } from "svelte";
  import { t, locale } from "$lib/i18n";
  import { diagnosticRuns } from "$lib/api/diagnostics";
  import { listAuditEvents } from "$lib/api/tauri";
  import PageHeader from "$lib/components/layout/PageHeader.svelte";
  import PageToolbar from "$lib/components/layout/PageToolbar.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import Choices from "$lib/components/ui/ChoiceGroup.svelte";
  import Search from "$lib/components/ui/SearchField.svelte";
  import Modal from "$lib/components/ui/Modal.svelte";
  import Icon from "$lib/components/ui/Icon.svelte";
  import DiagnosticsPanel from "./DiagnosticsPanel.svelte";
  import LogEventDetails from "./LogEventDetails.svelte";
  import { logTitle, logFields, logMatches, logTime, technicalEvent, redundantCause } from "$lib/utils/logs";
  import type { AuditEvent } from "$lib/model/types";
  import type { DiagnosticTarget } from "$lib/utils/diagnostic-message";

  let {
    embedded = false, onViewOperation, auditEvents = [], auditLoading = false,
    auditLogDir = "", onOpenAuditFolder, onReloadAudit,
  }: {
    embedded?: boolean;
    onViewOperation?: (target: DiagnosticTarget) => void;
    auditEvents?: AuditEvent[];
    auditLoading?: boolean;
    auditLogDir?: string;
    onOpenAuditFolder: () => void;
    onReloadAudit: () => void | Promise<void>;
  } = $props();
  let filter = $state("all");
  let search = $state("");
  let technical = $state(false);
  let expanded = $state<string | null>(null);
  let exporting = $state(false);
  let runs = $state<{runId: string; current: boolean; startedAt?: string | null}[]>([]);
  let runId = $state("");
  let retainedEvents = $state<AuditEvent[]>([]);
  let historyLoading = $state(false);
  let historyError = $state(false);
  let generation = 0;
  const source = $derived(runId ? retainedEvents : auditEvents);
  const visible = $derived(source.filter((event) =>
    (technical || (!technicalEvent(event) && !redundantCause(event,source))) &&
    (filter === "all" || (filter === "errors" ? event.level === "error" : event.level === "warn")) &&
    logMatches(event,search,$locale)
  ));
  const loading = $derived(runId ? historyLoading : auditLoading);
  function key(event: AuditEvent, index: number) { return event.runId && event.sequence ? `${event.runId}:${event.sequence}` : `legacy:${event.jobId ?? event.operationId ?? event.at}:${index}`; }
  function summary(event: AuditEvent) {
    return logFields(event).filter(([name]) => /(^|\.)(workId|phase|httpStatus|databaseCode|databaseKind|osCode|succeededCount|failedCount|cachedWorkCount)$/.test(name)).map(([name,value]) => `${name.split(".").at(-1)}=${value}`).join(" · ");
  }
  async function selectRun(id: string) {
    const ticket = ++generation;
    runId = id; expanded = null; historyError = false;
    if (!id) { void onReloadAudit(); return; }
    historyLoading = true; retainedEvents = [];
    try { const events = await listAuditEvents(500,id); if (ticket === generation) retainedEvents = events; }
    catch { if (ticket === generation) historyError = true; }
    finally { if (ticket === generation) historyLoading = false; }
  }
  onMount(() => {
    void diagnosticRuns().then((value) => { runs = value; }).catch(() => {});
    const interval = setInterval(() => {
      if (!runId && !auditLoading && document.visibilityState === "visible") void onReloadAudit();
    }, 5000);
    return () => { generation++; clearInterval(interval); };
  });
</script>

<div class="logs-layout dldr:min-w-0">
  <PageHeader level={embedded ? 2 : 1} title="Logs">
    <Button onclick={() => (exporting = true)}><Icon name="downloads" />{$t("Export diagnostics")}</Button>
    {#snippet toolbar()}
      <PageToolbar label={$t("Activity tools")} onReload={() => runId ? selectRun(runId) : onReloadAudit()} reloadDisabled={loading}>
        <div class="dldr:min-w-40 dldr:basis-60 dldr:grow"><Search bind:value={search} label={$t("Search logs")} placeholder={$t("Search operation, RJ number or error code…")} clearable /></div>
        <Choices label={$t("Activity filter")} value={filter} onchange={(value) => (filter = value)} options={[
          {value:"all",label:$t("All")}, {value:"errors",label:$t("Errors")}, {value:"warnings",label:$t("Warnings")},
        ]} />
        {#snippet actions()}
          <Button disabled={!auditLogDir} onclick={onOpenAuditFolder}><Icon name="folder" />{$t("Open log folder")}</Button>
        {/snippet}
      </PageToolbar>
    {/snippet}
  </PageHeader>
  <div class="dldr:my-4 dldr:flex dldr:flex-wrap dldr:items-center dldr:gap-4 dldr:text-sm">
    <label class="dldr:flex dldr:items-center dldr:gap-2">
      <span>{$t("Run")}</span>
      <select aria-label={$t("Run")} value={runId} onchange={(e) => selectRun(e.currentTarget.value)} class="dldr:max-w-full dldr:rounded-md dldr:border dldr:border-solid dldr:border-ui-line dldr:bg-ui-input dldr:p-2 dldr:text-sm dldr:text-ui-ink dldr:ui-focus-control">
        <option value="">{$t("Current run")}</option>
        {#each runs.filter((r) => !r.current) as run}<option value={run.runId}>{run.startedAt ? logTime(run.startedAt,$locale) : run.runId} · {run.runId.slice(-8)}</option>{/each}
      </select>
    </label>
    <label class="dldr:flex dldr:cursor-pointer dldr:items-center dldr:gap-2"><input type="checkbox" bind:checked={technical} />{$t("Include routine queries and operation starts")}</label>
    <span class="dldr:ml-auto dldr:text-ui-dim">{$t("Latest {0} events",source.length)} · {visible.length} {$t("shown")}</span>
  </div>
  <p class="dldr:my-3 dldr:text-sm dldr:text-ui-dim">{$t("One timeline for downloads, playback and application events. Expand a record for the cause, suggested action and diagnostic details.")}</p>
  {#if historyError}<p role="status" class="dldr:text-ui-error">{$t("Could not load saved logs. Try reloading or exporting diagnostics.")}</p>{/if}
  {#if loading && !source.length}<p role="status" class="dldr:py-10 dldr:text-center dldr:text-ui-dim">{$t("Loading…")}</p>
  {:else if !visible.length}<p class="dldr:py-10 dldr:text-center dldr:text-ui-dim">{$t("No matching events")}</p>{/if}
  <ol aria-label={$t("Logs timeline")} class="dldr:m-0 dldr:list-none dldr:p-0">
    {#each visible as event, index (key(event,index))}
      <li class:log-error={event.level === "error"} class="log-record dldr:border-0 dldr:border-b dldr:border-solid dldr:border-ui-line">
        <button type="button" aria-expanded={expanded === key(event,index)} onclick={() => (expanded = expanded === key(event,index) ? null : key(event,index))}
          class="log-row dldr:box-border dldr:flex dldr:w-full dldr:cursor-pointer dldr:gap-3 dldr:border-0 dldr:bg-transparent dldr:px-3 dldr:py-4 dldr:text-left dldr:font-[inherit] dldr:text-ui-ink dldr:ui-focus-row dldr:hover:bg-ui-hover">
          <span class={`dldr:min-w-12 dldr:pt-1 dldr:text-xs dldr:font-bold dldr:uppercase ${event.level === "error" ? "dldr:text-ui-error" : "dldr:text-ui-dim"}`}>{event.level}</span>
          <span class="dldr:flex dldr:min-w-0 dldr:grow dldr:flex-col dldr:gap-1">
            <strong class="dldr:wrap-anywhere dldr:text-base">{logTitle(event,$locale)}</strong>
            {#if event.errorMessage}<span class="dldr:wrap-anywhere dldr:text-sm dldr:text-ui-error">{$t(event.errorMessage)}</span>{/if}
            {#if summary(event)}<span class="dldr:wrap-anywhere dldr:font-mono dldr:text-sm dldr:text-ui-dim">{summary(event)}</span>{/if}
            <span class="dldr:wrap-anywhere dldr:text-xs dldr:text-ui-dim">{event.operation}{#if event.durationMs != null} · {event.durationMs} ms{/if}</span>
          </span>
          <span class="log-time dldr:flex dldr:shrink-0 dldr:flex-col dldr:items-end dldr:gap-2 dldr:text-xs dldr:text-ui-dim"><time datetime={event.at}>{logTime(event.at,$locale)}</time><Icon name={expanded === key(event,index) ? "chevronUp" : "chevronDown"} /></span>
        </button>
        {#if expanded === key(event,index)}<div class="dldr:px-4 dldr:pb-4">
          <LogEventDetails {event} />
          {#if onViewOperation && event.operationId && event.runId}<Button onclick={() => onViewOperation?.({runId:event.runId!,operationId:event.operationId!,message:logTitle(event,$locale)})}>{$t("View complete operation")}</Button>{/if}
        </div>{/if}
      </li>
    {/each}
  </ol>
</div>
{#if exporting}<Modal title={$t("Export diagnostics")} wide onClose={() => (exporting = false)}><DiagnosticsPanel events={source} initialRunId={runId} /></Modal>{/if}

<style>
  .log-error { border-inline-start: 3px solid var(--color-ui-error); }
  @media (max-width: 680px) { .log-row { flex-wrap: wrap; } .log-time { width:100%; flex-direction:row; justify-content:space-between; } }
</style>
