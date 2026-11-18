<script lang="ts">
  import { t } from "$lib/i18n";
  import type { AuditEvent } from "$lib/model/types";
  import { logFields, logText } from "$lib/utils/logs";
  import Button from "$lib/components/ui/Button.svelte";
  import Icon from "$lib/components/ui/Icon.svelte";
  let { event }: { event: AuditEvent } = $props();
  let copied = $state(false);
  let copyError = $state(false);
  async function copy() {
    try { await navigator.clipboard.writeText(logText(event)); copied = true; copyError = false; }
    catch { copyError = true; }
  }
</script>

{#if event.errorMessage}<p class="log-cause dldr:my-2 dldr:wrap-anywhere dldr:text-ui-error">{$t(event.errorMessage)}</p>{/if}
{#if event.hint}<p class="dldr:my-2 dldr:wrap-anywhere dldr:text-sm"><strong>{$t("Suggested action")}</strong> · {$t(event.hint)}</p>{/if}
{#if logFields(event).length}
  <dl class="log-fields dldr:my-3 dldr:grid dldr:grid-cols-[minmax(6rem,auto)_minmax(0,1fr)] dldr:gap-x-4 dldr:gap-y-2 dldr:text-sm">
    {#each logFields(event) as [key,value]}
      <dt class="dldr:wrap-anywhere dldr:text-ui-dim">{key}</dt>
      <dd class="dldr:m-0 dldr:select-text dldr:wrap-anywhere dldr:font-mono">{value}</dd>
    {/each}
  </dl>
{/if}
<div class="dldr:flex dldr:flex-wrap dldr:gap-2 dldr:py-2"><Button onclick={copy}><Icon name={copied ? "check" : "copy"} />{copied ? $t("Copied") : $t("Copy log entry")}</Button></div>
{#if copyError}<textarea aria-label={$t("Log entry")} readonly value={logText(event)} rows="8" class="dldr:box-border dldr:w-full dldr:bg-ui-input dldr:p-3 dldr:text-sm dldr:text-ui-ink"></textarea>{/if}
<details class="dldr:mt-2">
  <summary class="dldr:ui-focus-row dldr:cursor-pointer dldr:py-2 dldr:text-sm dldr:text-ui-dim">{$t("Raw diagnostic record")}</summary>
  <pre class="dldr:select-text dldr:whitespace-pre-wrap dldr:wrap-anywhere dldr:text-xs">{JSON.stringify(event,null,2)}</pre>
</details>
