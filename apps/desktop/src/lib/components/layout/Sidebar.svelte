<script lang="ts">
  import { t } from "$lib/i18n";
  import { onMount } from "svelte";
  import type { Snippet } from "svelte";
  import type { View } from "$lib/model/types";
  import { views, labels } from "$lib/model/navigation";
  import Button from "$lib/components/ui/Button.svelte";
  import sidebarLogo from "$lib/assets/brand/dldr-logo.png";
  import Icon from "$lib/components/ui/Icon.svelte";
  let {
    view,
    version = "",
    onNavigate,
    downloadStatus,
    onOpenDownloadFolder,
    onOpenPlay,
  }: {
    view: View;
    version?: string;
    onNavigate: (view: View) => void;
    downloadStatus?: Snippet;
    onOpenDownloadFolder?: () => void;
    onOpenPlay?: () => void;
  } = $props();
  let keyboardFocus = $state(false);
  onMount(() => {
    // Native dialogs may restore :focus-visible even after a pointer dismissal.
    // Track input before dialog handlers can stop Escape/Enter propagation.
    const keyboard = (event: KeyboardEvent) => {
      if (!event.altKey && !event.ctrlKey && !event.metaKey) keyboardFocus = true;
    };
    const pointer = () => { keyboardFocus = false; };
    window.addEventListener("keydown", keyboard, true);
    window.addEventListener("pointerdown", pointer, true);
    return () => {
      window.removeEventListener("keydown", keyboard, true);
      window.removeEventListener("pointerdown", pointer, true);
    };
  });
</script>

<aside
  class:keyboard-focus={keyboardFocus}
  class="sidebar dldr:flex dldr:min-h-0 dldr:flex-col dldr:gap-3 dldr:overflow-y-auto dldr:overscroll-y-contain dldr:[&>*]:shrink-0 dldr:border-0 dldr:border-r dldr:border-solid dldr:border-ui-line-subtle dldr:bg-ui-sidebar dldr:px-3.5 dldr:pt-6 dldr:pb-4 dldr:text-base dldr:max-[620px]:border-r-0 dldr:max-[620px]:border-b dldr:max-[620px]:pt-3 dldr:max-[620px]:pb-2"
  aria-label={$t("Primary")}
>
  <a
    href="#library"
    aria-label={$t("DLsite Downloader — Library")}
    onclick={(event) => {
      event.preventDefault();
      onNavigate("library");
    }}
    class="brand-link dldr:text-ui-ink dldr:no-underline dldr:ui-focus-text"
  >
    <img class="brand-logo" src={sidebarLogo} alt="DLDR — DLsite Downloader" />
  </a>
  <nav aria-label={$t("Main")} class="dldr:grid dldr:gap-1 dldr:max-[620px]:grid-cols-2">
    {#each views as item}<Button
        data-view={item}
        variant="navigation"
        aria-current={view === item ? "page" : undefined}
        onclick={() => onNavigate(item)}
      >
        <Icon name={item} /><span>{$t(labels[item])}</span>
      </Button>{/each}
    {#if onOpenPlay}<Button data-action="dlsite-play" variant="navigation" onclick={onOpenPlay}>
      <Icon name="play" /><span>DLsite Play</span>
    </Button>{/if}
  </nav>
  <div class="dldr:mt-auto dldr:grid dldr:min-w-0 dldr:grid-cols-1 dldr:gap-3">
  {#if onOpenDownloadFolder}
    <Button variant="secondary" class="folder-button dldr:w-full dldr:border-ui-selected-border" onclick={onOpenDownloadFolder}
      title={$t("Open the library folder containing completed downloads")}>
      <Icon name="folder" /><span>{$t("Open Folder")}</span>
    </Button>
  {/if}
  {@render downloadStatus?.()}
  </div>
  <span
    class="version dldr:mt-1 dldr:flex dldr:justify-between dldr:border-0 dldr:border-t dldr:border-solid dldr:border-ui-line-subtle dldr:pt-3 dldr:text-xs dldr:text-ui-dim dldr:max-[620px]:hidden"
    >DLDR <span>{version}</span></span
  >
</aside>

<style>
  .sidebar {
    font-weight: 700;
    color-scheme: dark;
    --dldr-color-ui-ink: var(--dldr-color-brand-white);
    --dldr-color-ui-dim: color-mix(in srgb, var(--dldr-color-brand-white) 84%, var(--dldr-color-brand-sky));
    --dldr-color-ui-accent: var(--dldr-color-brand-sky);
    --dldr-color-ui-error: color-mix(in srgb, var(--dldr-color-brand-pink) 90%, var(--dldr-color-brand-white));
    --dldr-color-ui-elevated: var(--dldr-color-brand-blue);
    --dldr-color-ui-hover: var(--dldr-color-brand-blue);
    --dldr-color-ui-selected: var(--dldr-color-brand-blue);
    --dldr-color-ui-selected-text: var(--dldr-color-ui-dim);
    --dldr-color-ui-selected-strong: var(--dldr-color-brand-white);
    --dldr-color-ui-selected-border: var(--dldr-color-brand-sky);
    --dldr-color-ui-line-subtle: color-mix(in srgb, var(--dldr-color-brand-sky) 42%, var(--dldr-color-brand-navy));
    --dldr-color-ui-progress-track: color-mix(in srgb, var(--dldr-color-brand-sky) 18%, var(--dldr-color-brand-navy));
    --dldr-color-ui-focus-bg: var(--dldr-color-brand-blue);
    --dldr-shadow-ui-focus-control: inset 0 -2px 0 var(--dldr-color-brand-sky);
  }
  .brand-link { display: block; width: 128px; max-width: 100%; margin-inline: auto; border-radius: 8px; }
  .brand-logo { display: block; width: 100%; height: auto; max-height: 144px; object-fit: contain; }
  .sidebar :global(button) { font-weight: 700; }
  .sidebar :global(nav button) { min-height: 48px; gap: 12px; font-size: 16px; }
  .sidebar :global(nav button > span) { min-width: 0; overflow-wrap: anywhere; }
  .sidebar :global(button svg) { width: 22px; height: 22px; }
  .sidebar :global(.folder-button) { min-height: 44px; font-size: 16px; }
  .sidebar :global(nav button[aria-current="page"]) {
    background: var(--dldr-color-ui-selected);
    color: var(--dldr-color-ui-selected-text);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--dldr-color-brand-white) 45%, var(--dldr-color-brand-sky));
  }
  .sidebar :global(nav button:focus-visible) {
    outline: none;
    box-shadow: none;
  }
  .sidebar.keyboard-focus :global(nav button:focus-visible) {
    outline: 2px solid var(--dldr-color-brand-white);
    outline-offset: -2px;
  }
  .sidebar:not(.keyboard-focus) :global(nav button:focus-visible:not([aria-current="page"]):not(:hover)) {
    background: transparent;
  }
  @media (max-width: 620px) {
    .brand-link { width: 96px; }
    .sidebar :global(nav button) { gap: 6px; font-size: 14px; }
  }
</style>
