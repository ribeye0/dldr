<script lang="ts">
  import { t } from "$lib/i18n";
  import PageHeader from "$lib/components/layout/PageHeader.svelte";
  import PageToolbar from "$lib/components/layout/PageToolbar.svelte";
  import Icon from "$lib/components/ui/Icon.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import Modal from "$lib/components/ui/Modal.svelte";
  import type { Account, JobSnapshot } from "$lib/model/types";
  import AccountEditor from "./AccountEditor.svelte";
  import AccountSourceRow from "./AccountSourceRow.svelte";

  type AccountStatusTone =
    | "synced"
    | "syncing"
    | "failed"
    | "warning"
    | "disabled"
    | "idle";

  let {
    embedded = false,
    accounts = [],
    loading = false,
    saving = false,
    jobsLoading = false,
    editingAccountId = null,
    label = $bindable(""),
    loginName = $bindable(""),
    password = $bindable(""),
    syncAllDisabled = false,
    getActiveSyncJob,
    getStatusLabel,
    getStatusTone,
    onReload,
    onSyncAll,
    onToggleEnabled,
    onEdit,
    onSync,
    onCancelSync,
    onRemove,
    onReset,
    onSave,
  }: {
    embedded?: boolean;
    accounts?: Account[];
    loading?: boolean;
    saving?: boolean;
    jobsLoading?: boolean;
    editingAccountId?: string | null;
    label?: string;
    loginName?: string;
    password?: string;
    syncAllDisabled?: boolean;
    getActiveSyncJob: (accountId: string) => JobSnapshot | null;
    getStatusLabel: (account: Account) => string;
    getStatusTone: (account: Account) => AccountStatusTone;
    onReload: () => void;
    onSyncAll: () => void;
    onToggleEnabled: (account: Account, enabled: boolean) => void;
    onEdit: (account: Account) => void;
    onSync: (account: Account) => void;
    onCancelSync: (account: Account) => void;
    onRemove: (account: Account) => void;
    onReset: () => void;
    onSave: (event: SubmitEvent) => void | boolean | Promise<void | boolean>;
  } = $props();

  let editorOpen = $state(false);
  function edit(account: Account) {
    onEdit(account);
    editorOpen = true;
  }
  function add() {
    onReset();
    editorOpen = true;
  }
  async function save(event: SubmitEvent) {
    const result = await onSave(event);
    if (result !== false) editorOpen = false;
  }
  const selectedAccount = $derived(
    accounts.find((a) => a.id === editingAccountId) ?? null,
  );
</script>

<section class="accounts-panel" aria-label={$t("Accounts")}>
  <PageHeader level={embedded ? 2 : 1} title={$t("Accounts")}>
    <Button
      disabled={loading || jobsLoading || syncAllDisabled}
      onclick={onSyncAll}><Icon name="refresh" />{$t("Sync")}</Button
    >{#if accounts.length === 0}<Button variant="primary" disabled={saving} onclick={add}
      ><Icon name="plus" />{$t("Add account")}</Button
    >{/if}
    {#snippet toolbar()}
      <PageToolbar
        label={$t("Accounts tools")}
        {onReload}
        reloadDisabled={loading || saving}
      >
        <p class="dldr:m-0 dldr:text-sm dldr:text-ui-dim">
          {$t("Connect your DLsite account to find the works you own. Nothing is downloaded during setup.")}
        </p>
      </PageToolbar>
    {/snippet}
  </PageHeader>
  {#if loading}<p
      role="status"
      class="dldr:py-10 dldr:text-center dldr:text-ui-dim"
    >
      {$t("Loading…")}
    </p>{:else if !accounts.length}<p
      class="dldr:py-10 dldr:text-center dldr:text-ui-dim"
    >
      {$t("No accounts")}
    </p>{:else}
    {#each accounts as account (account.id)}<AccountSourceRow
        {account}
        statusLabel={getStatusLabel(account)}
        statusTone={getStatusTone(account)}
        activeSyncJob={getActiveSyncJob(account.id)}
        onSelect={edit}
        {onSync}
        {onCancelSync}
      />{/each}
  {/if}
</section>
{#if editorOpen}
  <Modal
    title={editingAccountId ? $t("Edit account") : $t("Add account")}
    dismissible={!saving}
    onClose={() => (editorOpen = false)}
  >
    <AccountEditor
      editing={Boolean(editingAccountId)}
      {saving}
      bind:label
      bind:loginName
      bind:password
      onSave={save}
    />
    {#if selectedAccount}<div
        class="dldr:mt-5 dldr:flex dldr:items-center dldr:justify-between dldr:gap-3 dldr:border-0 dldr:border-t dldr:border-solid dldr:border-ui-line dldr:pt-4"
      >
        <label class="dldr:flex dldr:items-center dldr:gap-2 dldr:text-sm"
          ><input
            type="checkbox"
            checked={selectedAccount.enabled}
            disabled={saving || Boolean(getActiveSyncJob(selectedAccount.id))}
            onchange={(e) =>
              selectedAccount &&
              onToggleEnabled(selectedAccount, e.currentTarget.checked)}
          />{$t("Enable sync")}</label
        >
        <Button
          tone="error"
          variant="text"
          disabled={saving || Boolean(getActiveSyncJob(selectedAccount.id))}
          onclick={() => {
            if (selectedAccount) {
              onRemove(selectedAccount);
              editorOpen = false;
            }
          }}>{$t("Remove account")}</Button
        >
      </div>{/if}
  </Modal>
{/if}
