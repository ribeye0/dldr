<script lang="ts">
  import { t } from "$lib/i18n";
  import Button from "$lib/components/ui/Button.svelte";
  import type { Account, JobSnapshot } from "$lib/model/types";
  import {
    accountCredentialLabel,
    accountEnabledLabel,
    accountLastSyncLabel,
    accountLoginLabel,
  } from "$lib/utils/accounts";

  type AccountStatusTone =
    "synced" | "syncing" | "failed" | "warning" | "disabled" | "idle";

  let {
    account,
    selected = false,
    statusLabel,
    statusTone = "idle",
    activeSyncJob = null,
    onToggleEnabled,
    onSelect,
    onSync,
    onCancelSync,
    onRemove,
  }: {
    account: Account;
    selected?: boolean;
    statusLabel: string;
    statusTone?: AccountStatusTone;
    activeSyncJob?: JobSnapshot | null;
    onToggleEnabled: (account: Account, enabled: boolean) => void;
    onSelect: (account: Account) => void;
    onSync: (account: Account) => void;
    onCancelSync: (account: Account) => void;
    onRemove: (account: Account) => void;
  } = $props();
</script>

<article
  class="account-row dldr:flex dldr:flex-wrap dldr:items-center dldr:gap-3 dldr:border-0 dldr:border-b dldr:border-solid dldr:border-ui-line dldr:px-4 dldr:py-4"
>
  <span
    class="dldr:grid dldr:size-10 dldr:shrink-0 dldr:place-items-center dldr:rounded-lg dldr:bg-ui-selected dldr:text-ui-accent dldr:text-lg"
    >{account.label.slice(0, 1)}</span
  >
  <div
    class="dldr:flex dldr:min-w-0 dldr:flex-1 dldr:flex-col dldr:gap-1 dldr:wrap-anywhere"
  >
    <strong>{account.label}</strong><span class="dldr:text-xs dldr:text-ui-dim"
      >{$t(accountLoginLabel(account))}</span
    >
    <span
      class={`dldr:text-xs ${statusTone === "failed" ? "dldr:text-ui-error" : "dldr:text-ui-dim"}`}
      >{statusLabel} · {account.enabled ? $t("Enabled") : $t("Disabled")}</span
    >
    <span class="dldr:text-xs dldr:text-ui-dim"
      >{$t("Credential:")} {$t(accountCredentialLabel(account))} · {$t(accountLastSyncLabel(account))}</span
    >
  </div>
  <div class="dldr:flex dldr:flex-wrap dldr:gap-2">
    {#if activeSyncJob}<Button
        disabled={!activeSyncJob.cancellable ||
          activeSyncJob.status === "cancelling"}
        onclick={() => onCancelSync(account)}
        >{activeSyncJob.status === "cancelling"
          ? $t("Cancelling…")
          : $t("Cancel sync")}</Button
      >{:else}<Button
        disabled={!account.enabled}
        onclick={() => onSync(account)}>{$t("Sync")}</Button
      >{/if}
    <Button variant="text" onclick={() => onSelect(account)}>{$t("Edit")}</Button>
  </div>
</article>
