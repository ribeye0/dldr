<script lang="ts">
  import { onMount } from "svelte";
  import { t, locale, locales, localeNames } from "$lib/i18n";
  import * as commands from "$lib/api/tauri";
  import * as native from "$lib/api/native";
  import type { Account, JobSnapshot } from "$lib/model/types";
  import { firstRunStep } from "$lib/utils/accounts";
  import { errorMessage } from "$lib/utils/format";
  import { isActiveJob } from "$lib/utils/jobs";
  import Modal from "$lib/components/ui/Modal.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import Field from "$lib/components/ui/Field.svelte";
  import TextInput from "$lib/components/ui/TextInput.svelte";
  import Progress from "$lib/components/ui/Progress.svelte";

  let { accounts, jobs, initialLibraryRoot, initialDownloadRoot, onRefresh, onComplete }: {
    accounts: Account[];
    jobs: JobSnapshot[];
    initialLibraryRoot: string;
    initialDownloadRoot: string | null;
    onRefresh: () => Promise<void>;
    onComplete: () => Promise<void>;
  } = $props();
  let step = $state<"account" | "sync" | "folders">("account");
  let accountId = $state<string | null>(null);
  let label = $state("DLsite");
  let loginName = $state("");
  let password = $state("");
  let folder = $state("");
  let defaultFolder = $state("");
  let loadingFolder = $state(true);
  let saving = $state(false);
  let error = $state("");
  let syncJobId = $state<string | null>(null);
  let mustSync = $state(false);
  const selected = $derived(accounts.find(account => account.id === accountId));
  const savedAccounts = $derived(accounts.filter(account => account.enabled && account.hasCredential));
  const syncJob = $derived(syncJobId ? jobs.find(job => job.id === syncJobId) : jobs.find(job =>
    job.kind === "accountSync" && job.metadata.accountId === accountId && isActiveJob(job)));
  const syncing = $derived(Boolean(syncJob && isActiveJob(syncJob)));
  const synced = $derived(Boolean(selected?.enabled && selected.hasCredential && selected.lastSyncAt &&
    (syncJobId ? syncJob?.status === "succeeded" : !mustSync)));
  const stepIndex = $derived(["account", "sync", "folders"].indexOf(step));

  function chooseAccount(id: string) {
    const account = accounts.find(item => item.id === id);
    accountId = account?.id ?? null;
    label = account?.label ?? "DLsite";
    loginName = account?.loginName ?? "";
    password = "";
    syncJobId = null;
    error = "";
  }
  onMount(() => {
    const ready = savedAccounts.find(account => account.lastSyncAt) ?? savedAccounts[0] ?? accounts[0];
    if (ready) chooseAccount(ready.id);
    step = firstRunStep(accounts, initialLibraryRoot) ?? "folders";
    folder = initialLibraryRoot;
    void commands.getSetupFolders().then(defaults => {
      defaultFolder = defaults.libraryRoot;
      if (!folder) folder = defaultFolder;
    }).catch(cause => { error = errorMessage(cause); }).finally(() => { loadingFolder = false; });
    let refreshing = false;
    const timer = setInterval(() => {
      // Events normally update this view; polling also recovers missed events.
      if (step !== "sync" || !syncing || refreshing) return;
      refreshing = true;
      void onRefresh().catch(cause => { error = errorMessage(cause); }).finally(() => { refreshing = false; });
    }, 2000);
    return () => clearInterval(timer);
  });

  async function saveAccount(event: SubmitEvent) {
    event.preventDefault();
    if (saving || !label.trim() || !loginName.trim() || (!password && !selected?.hasCredential)) return;
    saving = true; error = "";
    try {
      const account = await commands.saveAccount({ id: accountId, label: label.trim(), loginName: loginName.trim(), password: password || null });
      accountId = account.id;
      password = "";
      if (!account.enabled) await commands.setAccountEnabled(account.id, true);
      await onRefresh();
      syncJobId = null;
      mustSync = true;
      step = "sync";
    } catch (cause) { error = errorMessage(cause); }
    finally { saving = false; }
  }
  async function startSync() {
    if (!accountId || saving || syncing) return;
    saving = true; error = "";
    try {
      const result = await commands.startAccountSync(accountId, null);
      syncJobId = result.jobId;
      await onRefresh();
    } catch (cause) { error = errorMessage(cause); }
    finally { saving = false; }
  }
  async function cancelSync() {
    if (!syncJob || saving) return;
    saving = true; error = "";
    try { await commands.cancelJob(syncJob.id); await onRefresh(); }
    catch (cause) { error = errorMessage(cause); }
    finally { saving = false; }
  }
  async function browse() {
    error = "";
    try {
      const chosen = await native.chooseDirectory({ title: $t("Choose library folder"), defaultPath: folder || defaultFolder || undefined, canCreateDirectories: true });
      if (typeof chosen === "string") folder = chosen;
    } catch (cause) { error = errorMessage(cause); }
  }
  async function finish(event: SubmitEvent) {
    event.preventDefault();
    if (saving || !folder.trim() || loadingFolder) return;
    saving = true; error = "";
    try {
      await commands.configureDownloadFolders(folder.trim(), initialDownloadRoot, $locale);
      await onComplete();
    } catch (cause) { error = $t("Could not finish setup. Check the folder is writable, then try again.") + " " + $t(errorMessage(cause)); }
    finally { saving = false; }
  }
  function back() { error = ""; step = step === "folders" ? "sync" : "account"; }
</script>

<Modal title={$t("Welcome to DLDR")} dismissible={false} onClose={() => {}}>
  <div class="first-run">
    <label class="setup-language">{$t("Interface language")}
      <select aria-label={$t("Interface language")} bind:value={$locale} disabled={saving}>
        {#each locales as code}<option value={code}>{localeNames[code]}</option>{/each}
      </select>
    </label>
    <ol class="setup-steps" aria-label={$t("First-time setup")}>
      {#each ["Set up account", "Sync purchases", "Download folder"] as title, index}
        <li aria-current={index === stepIndex ? "step" : undefined} class:done={index < stepIndex}>
          <span>{index < stepIndex ? "✓" : index + 1}</span>{$t(title)}
        </li>
      {/each}
    </ol>
    {#if step === "account"}
      <h3>{$t("Set up account")}</h3>
      <p>{$t("Connect your DLsite account to find the works you own. Nothing is downloaded during setup.")}</p>
      <form onsubmit={saveAccount}>
        <Field id="setup-label" label={$t("Account name")}><TextInput id="setup-label" bind:value={label} disabled={saving} /></Field>
        <Field id="setup-login" label={$t("Email or username")}><TextInput id="setup-login" autocomplete="username" bind:value={loginName} disabled={saving} /></Field>
        <Field id="setup-password" label={$t("Password")}><TextInput id="setup-password" type="password" autocomplete="current-password" bind:value={password} placeholder={selected?.hasCredential ? $t("Leave blank to keep saved password") : ""} disabled={saving} /></Field>
        <footer><Button variant="primary" type="submit" disabled={saving || !label.trim() || !loginName.trim() || (!password && !selected?.hasCredential)}>{saving ? $t("Saving…") : $t("Save and continue")}</Button></footer>
      </form>
    {:else if step === "sync"}
      <h3>{$t("Sync purchases")}</h3>
      <p>{$t("Sign in and sync your purchased works. Enter a verification code if DLsite asks for one.")}</p>
      <p class="setup-account">{selected?.label} · {selected?.loginName}</p>
      <div role="status" aria-live="polite">
        {#if syncing}
          <p>{$t("Signing in and syncing your purchased works…")}</p>
          <Progress value={syncJob?.progress?.total ? Math.min(100, Math.max(0, (syncJob.progress.current ?? 0) * 100 / syncJob.progress.total)) : null} label={$t("Sync purchases")} />
          {#if syncJob?.progress?.total}<p>{syncJob.progress.current ?? 0} / {syncJob.progress.total}</p>{/if}
        {:else if synced}<p class="setup-success">✓ {$t("Your purchases are synced.")}</p>
        {:else if syncJob?.status === "cancelled"}<p>{$t("Sync cancelled. You can retry whenever you are ready.")}</p>{/if}
      </div>
      {#if syncJob?.status === "failed"}<p role="alert" class="setup-error">{$t("Sign-in or sync failed. Check your account details and try again.")} {syncJob.error?.message}</p>{/if}
      <footer>
        <Button disabled={saving || syncing} onclick={back}>{$t("Back")}</Button>
        {#if syncing}<Button disabled={saving || !syncJob?.cancellable} onclick={cancelSync}>{$t("Cancel sync")}</Button>
        {:else if synced}<Button variant="primary" disabled={saving} onclick={() => { error = ""; step = "folders"; }}>{$t("Continue")}</Button>
        {:else}<Button variant="primary" disabled={saving || !selected?.hasCredential} onclick={startSync}>{saving ? $t("Starting…") : $t("Sign in and sync")}</Button>{/if}
      </footer>
    {:else}
      <h3>{$t("Download folder")}</h3>
      <p>{$t("Choose where completed works are saved. The default is a dldr folder inside your system Downloads folder.")}</p>
      <form onsubmit={finish}>
        <Field id="setup-folder" label={$t("Library folder")}>
          <TextInput id="setup-folder" bind:value={folder} disabled={saving || loadingFolder} />
          <div class="folder-actions"><Button disabled={saving || loadingFolder} onclick={browse}>{$t("Browse")}</Button><Button variant="text" disabled={saving || loadingFolder || !defaultFolder} onclick={() => { folder = defaultFolder; error = ""; }}>{$t("Use default folder")}</Button></div>
        </Field>
        <p>{initialDownloadRoot ? $t("Existing staging folder: {0}", initialDownloadRoot) : $t("Temporary downloads are kept separately in the .staging subfolder. You can change this later in Settings.")}</p>
        <p>{$t("DLDR will create the folders and check write access before finishing.")}</p>
        <footer><Button disabled={saving} onclick={back}>{$t("Back")}</Button><Button variant="primary" type="submit" disabled={saving || loadingFolder || !folder.trim()}>{saving ? $t("Preparing folders…") : $t("Finish setup")}</Button></footer>
      </form>
    {/if}
    {#if error}<p role="alert" class="setup-error">{error}</p>{/if}
  </div>
</Modal>

<style>
  .first-run { font-size: 16px; line-height: 1.6; }
  h3 { margin: 18px 0 6px; font-size: 20px; }
  p { margin: 8px 0 16px; color: var(--dldr-color-ui-dim); overflow-wrap: anywhere; }
  form { display: flex; flex-direction: column; gap: 16px; }
  select { max-width: 100%; padding: 8px; border: 1px solid var(--dldr-color-ui-line-strong); border-radius: 6px; background: var(--dldr-color-ui-surface); color: var(--dldr-color-ui-ink); font: inherit; }
  .setup-language, footer, .folder-actions { display: flex; align-items: center; flex-wrap: wrap; gap: 10px; }
  .setup-language { justify-content: space-between; font-size: 14px; }
  .setup-steps { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); list-style: none; padding: 18px 0; margin: 12px 0; border-bottom: 1px solid var(--dldr-color-ui-line); gap: 8px; }
  .setup-steps li { display: flex; flex-direction: column; align-items: center; text-align: center; gap: 6px; color: var(--dldr-color-ui-dim); font-size: 13px; }
  .setup-steps span { display: grid; place-items: center; width: 30px; height: 30px; border: 1px solid var(--dldr-color-ui-line-strong); border-radius: 50%; font-weight: 700; }
  .setup-steps [aria-current], .setup-steps .done { color: var(--dldr-color-ui-accent); font-weight: 700; }
  .setup-steps [aria-current] span { color: var(--dldr-color-ui-on-accent); background: var(--dldr-color-ui-accent); border-color: var(--dldr-color-ui-accent); }
  footer { justify-content: flex-end; margin-top: 12px; }
  .setup-account { font-weight: 600; color: var(--dldr-color-ui-ink); }
  .setup-error { color: var(--dldr-color-ui-error); }
  .setup-success { color: var(--dldr-color-ui-accent); font-weight: 600; }
</style>
