<script lang="ts">
  import { tick, untrack } from "svelte";
  import type { Snippet } from "svelte";
  import type { View } from "$lib/model/types";
  import Sidebar from "./Sidebar.svelte";
  import BackToTop from "./BackToTop.svelte";
  let {
    activeView,
    playerActive = false,
    onNavigate,
    children,
    downloadStatus,
    version = "",
    onOpenDownloadFolder,
    onOpenPlay,
  }: {
    activeView: View;
    playerActive?: boolean;
    onNavigate: (view: View) => void;
    children?: Snippet;
    downloadStatus?: Snippet;
    version?: string;
    onOpenDownloadFolder?: () => void;
    onOpenPlay?: () => void;
  } = $props();
  let scroller: HTMLElement;
  let scrollTop = $state(0);
  const positions: Partial<Record<View, number>> = {};
  $effect(() => {
    const view = activeView;
    void tick().then(() =>
      untrack(() => {
        if (scroller) {
          scroller.scrollTop = positions[view] ?? 0;
          scrollTop = scroller.scrollTop;
        }
      }),
    );
  });
</script>

<div
  class="app-shell dldr:grid dldr:h-dvh dldr:grid-cols-[196px_minmax(0,1fr)] dldr:overflow-hidden dldr:bg-ui-background dldr:text-ui-ink dldr:text-base dldr:max-[620px]:grid-cols-1 dldr:max-[620px]:grid-rows-[auto_minmax(0,1fr)]"
>
  <Sidebar
    view={activeView}
    {version}
    {downloadStatus}
    {onNavigate}
    {onOpenDownloadFolder}
    {onOpenPlay}
  />
  <main
    bind:this={scroller}
    class:with-player={playerActive}
    tabindex="-1"
    class="workspace dldr:outline-none dldr:min-h-0 dldr:min-w-0 dldr:overflow-auto dldr:px-6 dldr:py-7 dldr:[scrollbar-gutter:stable] dldr:max-[620px]:px-4 dldr:max-[620px]:py-5"
    onscroll={() => {
      scrollTop = scroller.scrollTop;
      positions[activeView] = scrollTop;
    }}
  >
    {@render children?.()}
  </main>
</div>
{#if scrollTop > 300}<BackToTop
    target={() => scroller}
    class={playerActive ? "dldr:fixed dldr:bottom-28 dldr:right-6 dldr:z-20" : "dldr:fixed dldr:bottom-6 dldr:right-6 dldr:z-20"}
  />{/if}

<style> .workspace.with-player { padding-bottom:112px; } </style>
