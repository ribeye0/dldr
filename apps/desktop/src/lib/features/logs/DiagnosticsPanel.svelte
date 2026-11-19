<script lang="ts">
  import { t } from "$lib/i18n";
  import { onMount, untrack } from "svelte";
  import UiButton from "$lib/components/ui/ActionButton.svelte";
  import {
    diagnosticRuns,
    exportDiagnostics,
    getDiagnosticSummary,
    operationEvents,
    supportSummary,
  } from "$lib/api/diagnostics";
  import type { AuditEvent } from "$lib/model/types";
  let {
    events = [],
    initialOperationId = "",
    initialRunId = "",
  }: { events?: AuditEvent[]; initialOperationId?: string; initialRunId?: string } = $props();
  let runs = $state<{ runId: string; current: boolean }[]>([]);
  let runId = $state("");
  let operationId = $state("");
  let health = $state($t("Checking diagnostics…"));
  let busy = $state(false);
  let status = $state("");
  let savedPath = $state("");
  let summary = $state("");
  let detail = $state<AuditEvent[]>([]);
  const operations = $derived([
    ...new Map(
      events.filter((e) => e.operationId).map((e) => [e.operationId!, e]),
    ).values(),
  ]);
  async function refresh() {
    try {
      const value = await getDiagnosticSummary();
      health =
        value.health.state === "available" &&
        value.health.droppedCritical + value.health.droppedRoutine === 0
          ? ""
          : $t("Logging: {0} · Dropped: {1} · Older records removed from memory: {2}", value.health.state, value.health.droppedCritical + value.health.droppedRoutine, value.health.ringEvicted);
      runs = await diagnosticRuns();
    } catch {
      health =
        $t("Diagnostics storage unavailable. You can still copy a support summary.");
    }
  }
  onMount(() => {
    runId = initialRunId;
    void refresh();
  });
  $effect(() => {
    const id = initialOperationId;
    untrack(() => {
      operationId = id;
      void inspect();
    });
  });
  async function save() {
    busy = true;
    status = "";
    try {
      const path = await exportDiagnostics(runId, operationId);
      if (path) {
        savedPath = path;
        status = $t("Saved to {0}", path);
      }
    } catch {
      status =
        $t("Could not save diagnostics. Try a new filename in a writable folder, or copy the summary below.");
    } finally {
      busy = false;
      await refresh();
    }
  }
  async function copyPath() {
    try {
      await navigator.clipboard.writeText(savedPath);
      status = $t("Saved path copied.");
    } catch {
      status = $t("Saved to {0}", savedPath);
    }
  }
  async function copy() {
    summary = await supportSummary();
    try {
      await navigator.clipboard.writeText(summary);
      status = $t("Support summary copied.");
    } catch {
      status = $t("Copy the support summary from the text field below.");
    }
  }
  async function inspect() {
    if (!operationId) {
      detail = [];
      return;
    }
    try {
      detail = await operationEvents(operationId,runId);
    } catch {
      status = $t("Operation details unavailable. Try the support summary.");
    }
  }
</script>

<section class="diagnostics" aria-label={$t("Diagnostics")}>
  <div class="actions">
    <strong>{$t("Diagnostics")}</strong>
    <UiButton disabled={busy} onclick={save}
      >{$t("Export diagnostics")}</UiButton
    >
    <UiButton variant="secondary" onclick={copy}
      >{$t("Copy support summary")}</UiButton
    >
    <UiButton variant="secondary" onclick={refresh}
      >{$t("Refresh")}</UiButton
    >
  </div>
  {#if health}<p role="status">{health}</p>{/if}
  <div class="actions">
    <label
      >{$t("Run")} <select
        bind:value={runId}
        onchange={() => {
          operationId = "";
          detail = [];
        }}
      >
        <option value="">{$t("Current run")}</option>
        {#each runs.filter((r) => !r.current) as run}<option value={run.runId}
            >{run.runId}</option
          >{/each}
      </select></label
    >
    {#if !runId}
      <label
        >{$t("Operation")} <select bind:value={operationId} onchange={inspect}>
          <option value="">{$t("All recent operations")}</option>
          {#each operations as event}<option value={event.operationId!}
              >{event.operation} · {event.outcome} · {event.at}</option
            >{/each}
        </select></label
      >
    {/if}
  </div>
  {#if operationId}<pre aria-label={$t("Operation details")}>{JSON.stringify(
        detail,
        null,
        2,
      )}</pre>{/if}
  {#if status}<p role="status">{status}</p>{/if}
  {#if savedPath}<UiButton variant="secondary" onclick={copyPath}
      >{$t("Copy saved path")}</UiButton
    >{/if}
  {#if summary}<textarea
      aria-label={$t("Support summary")}
      readonly
      value={summary}
      rows="6"></textarea>{/if}
</section>

<style>
  .diagnostics {
    padding: 0;
    min-width: 0;
  }
  .actions {
    display: flex;
    gap: 10px;
    flex-wrap: wrap;
    align-items: center;
  }
  p {
    color: var(--muted);
    font-size: var(--dldr-settings-detail-size, 12px);
    overflow-wrap: anywhere;
    margin: 8px 0;
  }
  label {
    font-size: var(--dldr-settings-detail-size, 12px);
    max-width: 100%;
  }
  select {
    background: var(--panel);
    color: var(--text);
    border: 1px solid var(--border);
    max-width: 100%;
  }
  textarea,
  pre {
    width: 100%;
    box-sizing: border-box;
    max-height: 180px;
    overflow: auto;
    background: var(--panel);
    color: var(--text);
    font-size: var(--dldr-settings-detail-size, 12px);
  }
</style>
