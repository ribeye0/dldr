<script lang="ts">
  import { onMount } from "svelte";
  import { get } from "svelte/store";
  import { locale } from "$lib/i18n";
  onMount(() => {
    const previous = get(locale);
    locale.set("en");
    return () => locale.set(previous);
  });
  import SettingsView from "$lib/features/settings/SettingsView.svelte";
  import { DEFAULT_LANGUAGE_PRIORITY } from "$lib/model/constants";
  import {
    buildLanguagePriority,
    defaultLanguagePriority,
  } from "$lib/utils/name-language-priority";

  let {
    viewState = "ready",
    languagePreset = "default",
  }: {
    viewState?:
      | "ready"
      | "loading"
      | "saving"
      | "checking"
      | "downloading"
      | "installing";
    languagePreset?: "default" | "japanese" | "unknown" | "subset";
  } = $props();

  let libraryRoot = $state("/Users/example/Library/DLDR Collection");
  let downloadRoot = $state("/Users/example/Downloads/dldr/.staging");
  let lastAction = $state("No action yet");
  let savedPriority = $state([...DEFAULT_LANGUAGE_PRIORITY]);
  let languagePriority = $state([...DEFAULT_LANGUAGE_PRIORITY]);
  let titleLanguageFollowsUi = $state(true);
  let savedFollowsUi = $state(true);
  $effect(() => {
    const saved =
      languagePreset === "subset"
        ? ["ja_JP"]
        : languagePreset === "japanese"
          ? [
              "ja_JP",
              ...DEFAULT_LANGUAGE_PRIORITY.filter((code) => code !== "ja_JP"),
            ]
          : languagePreset === "unknown"
            ? ["ko_KR", "ja_JP", "pt_BR", "xx_XX", "zh_TW"]
            : defaultLanguagePriority("en");
    const draft = buildLanguagePriority(saved);
    savedPriority = saved;
    languagePriority = draft;
    titleLanguageFollowsUi = savedFollowsUi = languagePreset === "default";
  });
</script>

<main class="story-surface">
  <SettingsView
    bind:uiLocale={$locale}
    bind:libraryRoot
    bind:downloadRoot
    bind:languagePriority
    bind:titleLanguageFollowsUi
    loading={viewState === "loading"}
    saving={viewState === "saving"}
    appInfo={{
      name: "DLDR",
      version: "0.1.0-rc.1",
      identifier: "io.github.ribeye0.dldr",
      tauriVersion: "2.8.5",
    }}
    appInfoLoading={viewState === "loading"}
    updateBusy={["checking", "downloading", "installing"].includes(viewState)}
    onReload={() => {
      languagePriority = buildLanguagePriority(savedPriority);
      titleLanguageFollowsUi = savedFollowsUi;
      lastAction = "Reverted changes";
    }}
    onChooseDirectory={(kind) => (lastAction = `Browse ${kind}`)}
    onUseDefaultDownloadRoot={() => {
      downloadRoot = "/Users/example/Downloads";
      lastAction = "Use default";
    }}
    onSave={(event) => {
      event.preventDefault();
      savedFollowsUi = titleLanguageFollowsUi;
      savedPriority = titleLanguageFollowsUi ? defaultLanguagePriority($locale) : [...languagePriority];
      lastAction = JSON.stringify({ libraryRoot, downloadRoot, titleLanguageFollowsUi, languagePriority: savedPriority });
      languagePriority = [...savedPriority];
    }}
    onOpenGitHub={() => (lastAction = "Open GitHub")}
    onOpenDlsite={() => (lastAction = "Open DLsite")}
    onCheckForUpdates={() => (lastAction = "Check for updates")}
  />
  <p aria-live="polite">{lastAction}</p>
</main>

<style>
  .story-surface {
    display: grid;
    gap: 12px;
    width: min(1120px, calc(100vw - 48px));
    margin: 24px auto;
  }
  p {
    margin: 0;
    color: var(--text-subtle);
    font-size: 12px;
    text-align: right;
  }
  @media (max-width: 720px) {
    .story-surface {
      width: calc(100vw - 24px);
      margin: 12px auto;
    }
  }
</style>
