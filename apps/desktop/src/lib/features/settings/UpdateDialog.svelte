<script lang="ts">
  import Modal from "$lib/components/ui/Modal.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import { t } from "$lib/i18n";
  import { formatBytes } from "$lib/utils/format";
  import type { UpdateController } from "$lib/controllers/update-controller.svelte";
  let { update, onOpenReleases }: { update: UpdateController; onOpenReleases: () => void } = $props();
  const percent = $derived(update.total ? Math.min(100, Math.floor(update.received / update.total * 100)) : null);
</script>

{#if update.open}
  <Modal title={$t("Update DLDR")} dismissible={!update.busy} onClose={() => update.close()}>
    <div class="update-content">
      <div aria-live="polite" role="status">
        {#if update.phase === "checking"}<p>{$t("Checking for updates…")}</p>
        {:else if update.phase === "current"}<p>{$t("DLDR is up to date.")}</p>
        {:else if update.phase === "unsupported"}<p>{$t("Automatic updates are available in the Linux AppImage and Windows releases. This development or native build must be updated manually.")}</p>
        {:else if update.phase === "error"}<p class="error">{$t(update.error)}</p>
        {:else if update.phase === "installed"}<p>{$t("Update installed. DLDR is restarting…")}</p>
        {:else if update.release}
          <p class="version">{update.release.currentVersion} <span aria-hidden="true">→</span> <strong>{update.release.version}</strong></p>
          {#if update.phase === "available"}
            <p>{$t("DLDR will download the update and restart. Your account, library and downloaded works will be kept.")}</p>
            {#if update.kind === "windowsPortable"}<p>{$t("Your Windows copy will remain portable.")}</p>{/if}
          {:else}
            <p>{update.phase === "verifying" ? $t("Verifying update…") : update.phase === "installing" ? $t("Installing update…") : $t("Downloading update…")}</p>
          {/if}
        {/if}
      </div>
      {#if update.phase === "downloading"}
        <progress max="100" value={percent ?? undefined} aria-label={$t("Update download progress")}></progress>
        <p class="transfer">{formatBytes(update.received)}{update.total ? ` / ${formatBytes(update.total)} · ${percent}%` : ""}</p>
      {/if}
      {#if update.release?.notes && update.phase === "available"}
        <details><summary>{$t("Release notes")}</summary><p class="notes">{update.release.notes}</p></details>
      {/if}
    </div>
    {#snippet actions()}
      {#if update.phase === "available"}
        <Button onclick={() => update.close()}>{$t("Cancel")}</Button>
        <Button variant="primary" onclick={() => update.install()}>{$t("Update and restart")}</Button>
      {:else if update.phase === "error"}
        <Button onclick={onOpenReleases}>{$t("GitHub Releases")} ↗</Button>
        <Button variant="primary" onclick={() => update.check()}>{$t("Retry")}</Button>
      {:else if !update.busy}
        {#if update.phase === "unsupported"}<Button onclick={onOpenReleases}>{$t("GitHub Releases")} ↗</Button>{/if}
        <Button onclick={() => update.close()}>{$t("Close")}</Button>
      {/if}
    {/snippet}
  </Modal>
{/if}

<style>
  .update-content { font-size:14px; line-height:1.7; }
  p { margin:0 0 12px; }
  .version { font-size:20px; display:flex; align-items:center; gap:12px; }
  .error { color:var(--dldr-color-ui-error); }
  progress { display:block; width:100%; height:10px; accent-color:var(--dldr-color-ui-accent); }
  .transfer { margin-top:8px; font-variant-numeric:tabular-nums; color:var(--dldr-color-ui-dim); }
  summary { cursor:pointer; font-weight:600; }
  .notes { margin-top:12px; white-space:pre-wrap; overflow-wrap:anywhere; max-height:240px; overflow:auto; }
</style>
