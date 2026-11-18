<script lang="ts">
  import type { Snippet } from "svelte";
  let {
    title,
    level = 1,
    children,
    tabs,
    toolbar,
  }: {
    title: string;
    level?: 1 | 2;
    children?: Snippet;
    tabs?: Snippet;
    toolbar?: Snippet;
  } = $props();
</script>

<div class="page-header dldr:mb-6 dldr:flex dldr:min-w-0 dldr:flex-col dldr:gap-4">
  <header
    class="dldr:flex dldr:min-h-10 dldr:flex-wrap dldr:items-center dldr:justify-between dldr:gap-3"
  >
    <svelte:element this={level === 1 ? "h1" : "h2"}
      tabindex="-1"
      class={`dldr:outline-none dldr:m-0 dldr:font-bold dldr:tracking-tight ${level === 1 ? "dldr:text-[28px]" : "dldr:text-[22px]"}`}
    >
      {title}
    </svelte:element>
    {#if children}<div class="dldr:flex dldr:flex-wrap dldr:items-center dldr:gap-2">
        {@render children()}
      </div>{/if}
  </header>
  {@render tabs?.()}
  {@render toolbar?.()}
</div>
