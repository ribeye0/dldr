<script lang="ts">
  import Modal from "$lib/components/ui/Modal.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import { t } from "$lib/i18n";
  import { formatBytes } from "$lib/utils/format";
  import type { UpdateController } from "$lib/controllers/update-controller.svelte";
  let { update, onOpenReleases }: { update: UpdateController; onOpenReleases: () => void } = $props();
  const percent = $derived(update.total ? Math.min(100, Math.floor(update.received / update.total * 100)) : null);
  const directions = { upgrade: "Upgrade", downgrade: "Downgrade", reinstall: "Reinstall" };
</script>

{#if update.open}
  <Modal title={$t("Update DLDR")} dismissible={update.phase !== "installing"} onClose={() => update.close()}>
    <div class="update-content">
      <div aria-live="polite" role="status">
        {#if update.phase === "checking"}<p>{$t("Checking for updates…")}</p>
        {:else if update.phase === "current"}<p>{$t("DLDR is up to date.")}</p>
        {:else if update.phase === "unsupported"}<p>{$t("Automatic updates are available in the Linux AppImage and Windows releases. This development or native build must be updated manually.")}</p>
        {:else if update.phase === "installed"}<p>{$t("Update installed. DLDR is restarting…")}</p>
        {:else if update.phase === "local"}
          <p>{$t("Choose a DLDR package for your current installation: AppImage, Windows portable ZIP, or Windows setup EXE. You can upgrade, downgrade or reinstall.")}</p>
        {:else if update.phase === "choosing"}<p>{$t("Select a local DLDR package")}</p>
        {:else if update.phase === "preparing"}<p>{$t("Inspecting package and checking its signature…")}</p>
        {:else if update.phase === "cancelling"}<p>{$t("Cancelling…")}</p>
        {:else if update.release}
          <p class="version">{update.release.currentVersion} <span aria-hidden="true">→</span> <strong>{update.release.version}</strong></p>
          {#if update.phase === "available"}
            <p>{$t("Download in the background and keep using DLDR. Install and restart when you are ready.")}</p>
            {#if update.kind === "windowsPortable"}<p>{$t("Your Windows copy will remain portable.")}</p>{/if}
          {:else if update.phase === "ready" && update.ready}
            <p><strong>{$t(directions[update.ready.direction])}</strong> · {$t("Ready to install")}</p>
            <p class="file">{update.ready.fileName} · {formatBytes(update.ready.size)}</p>
            <p>{$t(update.ready.verified ? "Official DLDR signature verified." : "This package is not verified by DLDR. Only install it if you trust its source.")}</p>
            {#if !update.ready.verified}
              <label class="trust"><input type="checkbox" bind:checked={update.trustLocal} />{$t("I trust this unsigned package and want to install it.")}</label>
            {/if}
            {#if update.ready.direction === "downgrade"}
              <p class="warning">{$t("Older versions may not support your current library or settings. Export a library backup in General before downgrading.")}</p>
            {/if}
            <p>{$t("Your account, library and downloaded works will be kept. Installation restarts DLDR.")}</p>
          {:else if update.phase !== "error"}
            <p>{update.phase === "verifying" ? $t("Verifying update…") : update.phase === "installing" ? $t("Installing update…") : $t("Downloading update…")}</p>
          {/if}
        {/if}
        {#if update.error}<p class="error" role="alert">{$t(update.error)}</p>{/if}
      </div>
      {#if update.phase === "downloading"}
        <progress max="100" value={percent ?? undefined} aria-label={$t("Update download progress")}></progress>
        <p class="transfer">{formatBytes(update.received)}{update.total ? ` / ${formatBytes(update.total)} · ${percent}%` : ""}</p>
      {/if}
      {#if update.canCancel || update.phase === "cancelling"}
        <p>{$t("You can close this window and continue using DLDR. Nothing is installed automatically.")}</p>
      {/if}
      {#if update.phase === "local" || (update.phase === "error" && update.localPath)}
        <p class="file">{update.localPath}</p>
        <label class="version-input">{$t("Package version (optional)")}
          <input bind:value={update.localVersion} placeholder="0.1.5" autocomplete="off" />
        </label>
        <p>{$t("Only needed for a renamed or custom build without version metadata.")}</p>
      {/if}
      {#if update.release?.notes && ["available", "ready"].includes(update.phase)}
        <details><summary>{$t("Release notes")}</summary><p class="notes">{update.release.notes}</p></details>
      {/if}
    </div>
    {#snippet actions()}
      {#if update.canCancel}
        <Button onclick={() => update.cancel()}>{$t("Cancel download")}</Button>
      {:else if update.phase === "available"}
        <Button variant="primary" onclick={() => update.download()}>{$t("Download update")}</Button>
      {:else if update.phase === "ready"}
        <Button variant="primary" disabled={!update.ready?.verified && !update.trustLocal} onclick={() => update.install()}>{$t("Install and restart")}</Button>
      {:else if update.phase === "local" || (update.phase === "error" && update.localPath)}
        <Button onclick={() => update.chooseLocal()}>{$t("Choose another file")}</Button>
        <Button variant="primary" onclick={() => update.prepareLocal()}>{$t("Inspect package")}</Button>
      {:else if update.phase === "error"}
        <Button onclick={onOpenReleases}>{$t("GitHub Releases")} ↗</Button>
        <Button variant="primary" onclick={() => update.check()}>{$t("Retry")}</Button>
      {/if}
      {#if update.phase !== "installing"}
        {#if update.phase === "unsupported"}<Button onclick={onOpenReleases}>{$t("GitHub Releases")} ↗</Button>{/if}
        <Button onclick={() => update.close()}>{$t(update.busy ? "Continue using DLDR" : "Close")}</Button>
      {/if}
    {/snippet}
  </Modal>
{/if}

<style>
  .update-content { font-size:16px; line-height:1.7; }
  p { margin:0 0 12px; }
  .version { font-size:20px; display:flex; align-items:center; gap:12px; }
  .error { color:var(--dldr-color-ui-error); }
  .file { overflow-wrap:anywhere; font-variant-numeric:tabular-nums; }
  .trust { display:flex; align-items:flex-start; gap:10px; margin:16px 0; font-weight:600; }
  .trust input { margin-top:7px; accent-color:var(--dldr-color-ui-accent); }
  .warning { border-left:3px solid var(--dldr-color-ui-accent); padding-left:12px; }
  .version-input { display:flex; flex-direction:column; gap:6px; margin-bottom:8px; }
  .version-input input { color:var(--dldr-color-ui-ink); background:var(--dldr-color-ui-surface); border:1px solid var(--dldr-color-ui-line); border-radius:6px; padding:8px 12px; font:inherit; }
  progress { display:block; width:100%; height:10px; accent-color:var(--dldr-color-ui-accent); }
  .transfer { margin-top:8px; font-variant-numeric:tabular-nums; color:var(--dldr-color-ui-dim); }
  summary { cursor:pointer; font-weight:600; }
  .notes { margin-top:12px; white-space:pre-wrap; overflow-wrap:anywhere; max-height:240px; overflow:auto; }
</style>
