<script lang="ts">
  import { tick, untrack } from "svelte";
  import type { Snippet } from "svelte";
  import type { View } from "$lib/model/types";
  import Sidebar from "./Sidebar.svelte";
  import BackToTop from "./BackToTop.svelte";
  import type { ListPosition } from "$lib/utils/filter-history.svelte";
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
  export function capturePosition(workId?: string): ListPosition {
    const focused = document.activeElement;
    return { scrollTop: scroller?.scrollTop ?? 0,
      focusLabel: focused?.getAttribute("aria-label") ?? undefined,
      workId: workId ?? focused?.closest("[data-work-id]")?.getAttribute("data-work-id") ?? undefined };
  }
  export async function restorePosition(position: ListPosition, focus = true) {
    await tick();
    if (!scroller) return;
    if (focus) {
      const card = position.workId ? scroller.querySelector(`[data-work-id="${CSS.escape(position.workId)}"]`) : null;
      const target = position.focusLabel ? (card ?? scroller).querySelector<HTMLElement>(`[aria-label="${CSS.escape(position.focusLabel)}"]`) : null;
      (target ?? card?.querySelector<HTMLElement>(".card-title") ?? scroller.querySelector<HTMLElement>(".list-header, .playlist-pagination") ?? scroller).focus({ preventScroll: true });
    }
    scroller.scrollTop = position.scrollTop;
    scrollTop = scroller.scrollTop;
    positions[activeView] = scrollTop;
  }
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
