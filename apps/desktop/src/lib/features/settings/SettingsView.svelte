<script lang="ts">
  import type { Snippet } from "svelte";
  import DownloadRateControl from "$lib/features/downloads/DownloadRateControl.svelte";
  import TransferConcurrencyControl from "$lib/features/downloads/TransferConcurrencyControl.svelte";
  import { t, locales, localeNames, type UiLocale } from "$lib/i18n";
  import PageHeader from "$lib/components/layout/PageHeader.svelte";
  import FormActions from "$lib/components/ui/FormActions.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import UiButton from "$lib/components/ui/ActionButton.svelte";
  import Choices from "$lib/components/ui/ChoiceGroup.svelte";
  import Mark from "$lib/components/layout/AppMark.svelte";
  import Disclosure from "$lib/components/ui/Disclosure.svelte";
  import Field from "$lib/components/ui/Field.svelte";
  import TextInput from "$lib/components/ui/TextInput.svelte";
  import LibraryMaintenance from "./LibraryMaintenance.svelte";
  import ThemeControl from "./ThemeControl.svelte";
  import type { AppInfo } from "$lib/model/types";
  import { appInfoValue } from "$lib/utils/format";
  import { DEFAULT_LANGUAGE_PRIORITY } from "$lib/model/constants";
  import { buildLanguagePriority } from "$lib/utils/name-language-priority";
  const titleLanguageLabels: Record<string, string> = {
    ja_JP: "Japanese", zh_TW: "Traditional Chinese", zh_CN: "Simplified Chinese", en_US: "English",
  };

  let {
    tab = $bindable("general"),
    accountSettings,
    logs,
    onTabChange,
    onLocaleChange,
    uiLocale = $bindable<UiLocale>("zh-TW"),
    libraryRoot = $bindable(""),
    downloadRoot = $bindable(""),
    autoExtract = $bindable(true),
    languagePriority = $bindable([...DEFAULT_LANGUAGE_PRIORITY]),
    loading = false,
    saving = false,
    appInfo = null,
    appInfoLoading = false,
    updatePhase = "idle",
    updateProgressMessage = "",
    onReload,
    onMoveLanguage,
    onChooseDirectory,
    onUseDefaultDownloadRoot,
    onSave,
    onOpenGitHub,
    onOpenDlsite,
    onCheckForUpdates,
  }: {
    tab?: string;
    accountSettings?: Snippet;
    logs?: Snippet;
    onTabChange?: (tab: string) => void;
    onLocaleChange?: (locale: UiLocale) => void;
    uiLocale?: UiLocale;
    libraryRoot?: string;
    downloadRoot?: string;
    autoExtract?: boolean;
    languagePriority?: string[];
    loading?: boolean;
    saving?: boolean;
    appInfo?: AppInfo | null;
    appInfoLoading?: boolean;
    updatePhase?: "idle" | "checking" | "downloading" | "installing";
    updateProgressMessage?: string;
    onReload: () => void;
    onMoveLanguage?: (code: string, direction: -1 | 1) => void;
    onChooseDirectory: (kind: "library" | "download") => void;
    onUseDefaultDownloadRoot: () => void;
    onSave: (event: SubmitEvent) => void;
    onOpenGitHub: () => void;
    onOpenDlsite: () => void;
    onCheckForUpdates: () => void;
  } = $props();

  const busy = $derived(loading || saving);
  const visiblePriority = $derived(buildLanguagePriority(languagePriority, uiLocale));
</script>

<div class="settings-layout dldr:min-w-0">
  <PageHeader title={$t("Settings")}>
    {#snippet tabs()}
      <Choices
        variant="tabs"
        label={$t("Settings section")}
        value={tab}
        onchange={(v) => { tab = v; onTabChange?.(v); }}
        options={[
          { value: "general", label: $t("General") },
          { value: "downloads", label: $t("Download settings") },
          { value: "accounts", label: $t("Accounts") },
          { value: "logs", label: "Logs" },
          { value: "about", label: $t("About & updates") },
        ]}
      />
    {/snippet}
  </PageHeader>
  {#if tab === "downloads"}
    <DownloadRateControl />
    <TransferConcurrencyControl />
    <form
      onsubmit={onSave}
      class="dldr:flex dldr:min-w-0 dldr:flex-col dldr:gap-6"
    >
      <Field id="archive-handling" label={$t("After downloading")}>
        <select id="archive-handling" disabled={busy} value={autoExtract ? "extract" : "keep"}
          onchange={(event) => { autoExtract = event.currentTarget.value === "extract"; }}
          class="dldr:w-full dldr:max-w-xl dldr:rounded-md dldr:border dldr:border-solid dldr:border-ui-line dldr:bg-ui-surface dldr:p-3 dldr:text-ui-ink dldr:text-base">
          <option value="extract">{$t("Extract automatically and delete archives (default)")}</option>
          <option value="keep">{$t("Keep archives")}</option>
        </select>
        <p class="dldr:text-sm dldr:text-ui-dim">{$t("Play audio and matching VTT subtitles directly from archives. Automatic extraction deletes recognized archives only after successful extraction. This setting applies to new downloads; queued and paused downloads keep their original choice.")}</p>
      </Field>
      <Field id="library-root" label={$t("Library folder")}
        ><div class="dldr:flex dldr:flex-wrap dldr:gap-2">
          <div class="dldr:min-w-0 dldr:flex-1">
            <TextInput
              id="library-root"
              disabled={busy}
              bind:value={libraryRoot}
            />
          </div>
          <UiButton
            variant="secondary"
            responsiveWidth="auto"
            disabled={busy}
            onclick={() => onChooseDirectory("library")}>{$t("Browse")}</UiButton
          >
        </div></Field
      >
      <Field id="download-root" label={$t("Download staging folder")}
        ><div class="dldr:flex dldr:flex-wrap dldr:gap-2">
          <div class="dldr:min-w-0 dldr:flex-1">
            <TextInput
              id="download-root"
              disabled={busy}
              bind:value={downloadRoot}
            />
          </div>
          <UiButton
            variant="secondary"
            responsiveWidth="auto"
            disabled={busy}
            onclick={() => onChooseDirectory("download")}>{$t("Browse")}</UiButton
          >
        </div>
        <div>
          <Button
            variant="text"
            disabled={busy}
            onclick={onUseDefaultDownloadRoot}
            >{$t("Use system Downloads folder")}</Button
          >
        </div></Field
      >
      <FormActions>
        <Button disabled={busy} onclick={onReload}>{$t("Revert changes")}</Button>
        <Button variant="primary" type="submit" disabled={busy}
          >{saving ? $t("Saving…") : $t("Save changes")}</Button
        >
      </FormActions>
    </form>{:else if tab === "general"}
    <ThemeControl />
    <form
      onsubmit={onSave}
      aria-label={$t("Name language settings")}
      class="dldr:flex dldr:min-w-0 dldr:flex-col dldr:gap-5"
    >
      <Field id="ui-locale" label={$t("Interface language")}>
        <select id="ui-locale" value={uiLocale} disabled={busy}
          onchange={(event) => { const next = event.currentTarget.value as UiLocale; if (onLocaleChange) onLocaleChange(next); else uiLocale = next; }}
          class="dldr:w-full dldr:max-w-sm dldr:rounded-md dldr:border dldr:border-solid dldr:border-ui-line dldr:bg-ui-surface dldr:p-3 dldr:text-ui-ink dldr:text-base">
          {#each locales as code}<option value={code}>{localeNames[code]}</option>{/each}
        </select>
        <p class="dldr:max-w-2xl dldr:text-sm dldr:text-ui-dim">{$t("Interface language and genres change together. Default title order follows the interface language; a custom order is kept. Save changes to remember this choice. Creator names are independent.")}</p>
      </Field>
      <div class="dldr:max-w-2xl dldr:text-sm dldr:text-ui-dim">
        <p class="dldr:mt-0">
          {$t("Choose the language order for work titles. The first available nonempty name is shown. Circle and voice actor names prefer Japanese, Traditional Chinese, then Simplified Chinese; a single supplied name is kept as-is.")}
        </p>
        <p class="dldr:mb-0">
          {$t("Search covers all supplied names. Other languages are grouped under Other and used only when the four preferred languages are unavailable.")}
        </p>
      </div>
      <ol
        aria-label={$t("Name language priority")}
        role="list"
        class="language-priority dldr:m-0 dldr:flex dldr:max-w-3xl dldr:list-none dldr:flex-col dldr:gap-2 dldr:p-0"
      >
        {#each visiblePriority as code, index (code)}
          {@const name = $t(titleLanguageLabels[code])}
          <li class="language-priority-row">
            <span class="language-rank" aria-hidden="true">{index + 1}</span>
            <div class="dldr:min-w-0 dldr:wrap-anywhere dldr:text-sm">
              <span>{name}</span>
              {#if name !== code}<code
                  class="dldr:mt-1 dldr:block dldr:text-xs dldr:text-ui-dim"
                  >{code}</code
                >{/if}
            </div>
            <div class="dldr:flex dldr:shrink-0 dldr:gap-1">
              <Button
                disabled={busy || index === 0}
                aria-label={$t("Move {0} up", name)}
                onclick={() => onMoveLanguage?.(code, -1)}>↑</Button
              >
              <Button
                disabled={busy || index === visiblePriority.length - 1}
                aria-label={$t("Move {0} down", name)}
                onclick={() => onMoveLanguage?.(code, 1)}>↓</Button
              >
            </div>
          </li>
        {/each}
        <li class="language-priority-row">
          <span class="language-rank" aria-hidden="true">5</span>
          <div class="dldr:min-w-0 dldr:wrap-anywhere dldr:text-sm">
            <span>{$t("Other")}</span>
            <span class="dldr:mt-1 dldr:block dldr:text-xs dldr:text-ui-dim">{$t("Fallback when preferred languages are unavailable")}</span>
          </div>
        </li>
      </ol>
      <FormActions>
        <Button disabled={busy} onclick={onReload}>{$t("Revert changes")}</Button>
        <Button variant="primary" type="submit" disabled={busy}
          >{saving ? $t("Saving…") : $t("Save changes")}</Button
        >
      </FormActions>
    </form><LibraryMaintenance />
  {:else if tab === "accounts"}{@render accountSettings?.()}
  {:else if tab === "logs"}{@render logs?.()}
  {:else}<section aria-label={$t("About")} class="dldr:min-w-0 dldr:py-2">
      <div class="dldr:flex dldr:items-center dldr:gap-3">
        <Mark size="large" />
        <div class="dldr:min-w-0">
          <h2 class="dldr:m-0 dldr:text-lg dldr:font-semibold">
            {$t(appInfoValue(appInfo?.name, appInfoLoading))}
          </h2>
          <p class="dldr:mt-1 dldr:mb-0 dldr:text-sm dldr:text-ui-dim">DLsite Downloader</p>
          <p class="dldr:mt-1 dldr:mb-0 dldr:text-sm dldr:text-ui-dim">
            {$t("Version")} {$t(appInfoValue(appInfo?.version, appInfoLoading))}
          </p>
        </div>
      </div>
      <div class="dldr:my-5">
        <UiButton variant="secondary" onclick={onCheckForUpdates}>{$t("Download updates")} ↗</UiButton>
        <p class="dldr:text-sm dldr:text-ui-dim">{$t("DLDR updates are downloaded manually from GitHub Releases. Close DLDR before replacing the application; your library and downloaded works are kept.")}</p>
      </div>
      <div
        class="dldr:mb-5 dldr:flex dldr:flex-wrap dldr:items-center dldr:gap-3 dldr:border-0 dldr:border-t dldr:border-solid dldr:border-ui-line dldr:pt-4"
      >
        <UiButton variant="secondary" onclick={onOpenGitHub}>GitHub ↗</UiButton
        ><UiButton variant="secondary" onclick={onOpenDlsite}>DLsite ↗</UiButton
        ><span class="dldr:text-xs dldr:text-ui-dim">MIT License</span>
      </div>
      <Disclosure title={$t("Application details")}
        ><dl
          class="dldr:grid dldr:grid-cols-[auto_minmax(0,1fr)] dldr:gap-3 dldr:text-sm"
        >
          <dt>{$t("Identifier")}</dt>
          <dd class="dldr:m-0 dldr:wrap-anywhere">
            {$t(appInfoValue(appInfo?.identifier, appInfoLoading))}
          </dd>
          <dt>Tauri</dt>
          <dd class="dldr:m-0">
            {$t(appInfoValue(appInfo?.tauriVersion, appInfoLoading))}
          </dd>
        </dl></Disclosure
      >
    </section>{/if}
</div>

<style>
  .settings-layout {
    --dldr-text-xs: 14px;
    --dldr-text-sm: 16px;
    --dldr-text-base: 16px;
    --dldr-text-xs--line-height: 1.5;
    --dldr-text-sm--line-height: 1.6;
    --dldr-text-base--line-height: 1.6;
    --dldr-settings-body-size: 16px;
    --dldr-settings-detail-size: 14px;
    font-size: var(--dldr-settings-body-size);
    line-height: 1.6;
  }
  .settings-layout > :global(.page-header .tabs > button) {
    font-weight: 700;
  }
  .language-priority-row {
    display: grid;
    grid-template-columns: 36px minmax(0, 1fr) auto;
    align-items: center;
    gap: 12px;
    min-width: 0;
    padding: 12px;
    border: 1px solid var(--dldr-color-ui-line);
    border-radius: 6px;
    background: var(--dldr-color-ui-surface);
  }
  .language-rank {
    display: grid;
    place-items: center;
    min-height: 36px;
    border-radius: 6px;
    background: var(--dldr-color-ui-selected);
    color: var(--dldr-color-ui-selected-text);
    font-size: 18px;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    line-height: 1;
  }
</style>
