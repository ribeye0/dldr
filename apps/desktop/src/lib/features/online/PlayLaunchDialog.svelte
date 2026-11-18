<script lang="ts">
  import { t } from "$lib/i18n";
  import Modal from "$lib/components/ui/Modal.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import type { Account } from "$lib/model/types";
  let { accounts, workId, selectedAccount = $bindable(""), busy = false, error = "", onOpen, onClose }: {
    accounts: Account[]; workId: string | null; selectedAccount?: string; busy?: boolean; error?: string;
    onOpen: () => void; onClose: () => void;
  } = $props();
</script>
<Modal title={$t("DLsite Play")} {onClose} dismissible={!busy}>
  <p class="intro">{$t("Listen online before downloading your purchased works.")}</p>
  {#if workId}<p class="work-id">{workId}</p>{/if}
  <label for="play-account">{$t("DLDR download account")}</label>
  <select id="play-account" bind:value={selectedAccount} disabled={busy}>
    <option value="">{$t("Choose an account")}</option>
    {#each accounts as account (account.id)}<option value={account.id}>{account.label}</option>{/each}
  </select>
  <p class="hint">{$t("Sign into the same account on the official website.")}</p>
  <p class="hint">{$t("Use your DLsite / viviON ID and password to sign in.")}</p>
  <p class="hint">{$t("The official browser login is saved separately from DLDR. Opening a work does not download it.")}</p>
  {#if error}<p role="alert" class="error">{error}</p>{/if}
  {#snippet actions()}
    <Button onclick={onClose} disabled={busy}>{$t("Cancel")}</Button>
    <Button variant="primary" onclick={onOpen} disabled={busy || !selectedAccount}>{$t(busy ? "Opening…" : "Open DLsite Play")}</Button>
  {/snippet}
</Modal>
<style>
  p { margin:0 0 14px; line-height:1.6; }
  label { display:block; font-weight:700; margin-bottom:7px; }
  select { box-sizing:border-box; width:100%; min-height:42px; padding:8px 10px; border:1px solid var(--dldr-color-ui-line-strong); border-radius:7px; background:var(--dldr-color-ui-surface); color:var(--dldr-color-ui-ink); font:inherit; }
  .hint { margin-top:12px; color:var(--dldr-color-ui-dim); font-size:14px; }
  .work-id { font-family:monospace; color:var(--dldr-color-ui-accent); }
  .error { color:var(--dldr-color-ui-error); }
</style>
