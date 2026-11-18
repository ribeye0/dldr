<script lang="ts">
  import { t, locale } from "$lib/i18n";
  import { exportDiagnostics, operationEvents } from "$lib/api/diagnostics";
  import Button from "$lib/components/ui/Button.svelte";
  import Disclosure from "$lib/components/ui/Disclosure.svelte";
  import Icon from "$lib/components/ui/Icon.svelte";
  import Modal from "$lib/components/ui/Modal.svelte";
  import type { AuditEvent } from "$lib/model/types";
  import type { DiagnosticTarget } from "$lib/utils/diagnostic-message";
  import { logTitle, logTime } from "$lib/utils/logs";
  import LogEventDetails from "./LogEventDetails.svelte";
  import { auditOutcomeLabel } from "$lib/utils/jobs";

  let { target, onClose }: { target: DiagnosticTarget; onClose: () => void } = $props();
  let events = $state<AuditEvent[]>([]);
  let loading = $state(true);
  let loadError = $state(false);
  let copying = $state(false);
  let copied = $state(false);
  let copyFailed = $state(false);
  let exporting = $state(false);
  let exportStatus = $state("");
  const reference = $derived(`[diagnostic:${target.runId}:${target.operationId}]`);

  $effect(() => {
    const { operationId, runId } = target;
    let active = true;
    loading = true;
    loadError = false;
    events = [];
    void operationEvents(operationId, runId).then((result) => {
      if (active) events = result.filter((event) => event.runId === runId);
    }).catch(() => {
      if (active) loadError = true;
    }).finally(() => {
      if (active) loading = false;
    });
    return () => { active = false; };
  });

  async function copyReference() {
    copying = true;
    copied = false;
    copyFailed = false;
    try {
      await navigator.clipboard.writeText(reference);
      copied = true;
    } catch {
      copyFailed = true;
    } finally {
      copying = false;
    }
  }
  async function save() {
    exporting = true;
    exportStatus = "";
    try {
      const path = await exportDiagnostics(target.runId, target.operationId);
      if (path) exportStatus = $t("Saved to {0}", path);
    } catch {
      exportStatus = $t("Could not export diagnostics. Try again or copy the diagnostic ID.");
    } finally {
      exporting = false;
    }
  }
</script>

<Modal title={$t("Activity details")} {onClose}>
  <p class="dldr:mt-0 dldr:wrap-anywhere">{target.message}</p>
  {#if loading}
    <p role="status" class="dldr:text-ui-dim">{$t("Loading activity…")}</p>
  {:else if loadError}
    <p role="status" class="dldr:text-ui-dim">{$t("Activity could not be loaded. You can still export diagnostics or copy the diagnostic ID.")}</p>
  {:else if !events.length}
    <p role="status" class="dldr:text-ui-dim">{$t("This operation is no longer in recent activity. Try exporting diagnostics for the saved records.")}</p>
  {:else}
    <ol aria-label={$t("Operation activity")} class="dldr:m-0 dldr:list-none dldr:p-0">
      {#each events as event}
        <li class="dldr:border-0 dldr:border-t dldr:border-solid dldr:border-ui-line dldr:py-3">
          <div class="dldr:flex dldr:flex-wrap dldr:items-baseline dldr:justify-between dldr:gap-2">
            <strong class="dldr:wrap-anywhere">{logTitle(event,$locale)}</strong>
            <span class={event.outcome === "failed" ? "dldr:text-ui-error" : "dldr:text-ui-dim"}>{auditOutcomeLabel(event.outcome, $locale)}</span>
          </div>
          <time class="dldr:text-xs dldr:text-ui-dim">{logTime(event.at,$locale)}</time>
          <LogEventDetails {event} />
        </li>
      {/each}
    </ol>
  {/if}
  <Disclosure title={$t("Diagnostic IDs")} nested>
    <dl class="dldr:m-0 dldr:grid dldr:gap-2 dldr:text-xs">
      <dt class="dldr:text-ui-dim">{$t("Run")}</dt>
      <dd class="dldr:m-0 dldr:select-text dldr:wrap-anywhere dldr:font-mono">{target.runId}</dd>
      <dt class="dldr:text-ui-dim">{$t("Operation")}</dt>
      <dd class="dldr:m-0 dldr:select-text dldr:wrap-anywhere dldr:font-mono">{target.operationId}</dd>
    </dl>
  </Disclosure>
  {#if copyFailed}
    <p role="status" class="dldr:text-sm dldr:text-ui-dim">{$t("Could not copy. Select the diagnostic ID below.")}</p>
    <textarea aria-label={$t("Diagnostic ID")} readonly value={reference} rows="3"
      class="dldr:box-border dldr:w-full dldr:rounded-md dldr:border dldr:border-solid dldr:border-ui-line dldr:bg-ui-input dldr:p-3 dldr:text-xs dldr:text-ui-ink dldr:ui-focus-control"></textarea>
  {/if}
  {#if exportStatus}<p role="status" class="dldr:wrap-anywhere dldr:text-sm dldr:text-ui-dim">{exportStatus}</p>{/if}
  {#snippet actions()}
    <Button disabled={copying} onclick={copyReference}><Icon name={copied ? "check" : "copy"} />{copied ? $t("Copied") : $t("Copy diagnostic ID")}</Button>
    <Button disabled={exporting} onclick={save}><Icon name="downloads" />{exporting ? $t("Exporting…") : $t("Export diagnostics")}</Button>
  {/snippet}
</Modal>
