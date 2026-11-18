<script lang="ts">
  import type { Snippet } from "svelte";
  import Icon from "$lib/components/ui/Icon.svelte";
  let {
    title,
    children,
    nested = false,
    open = false,
  }: {
    title: string;
    children: Snippet;
    nested?: boolean;
    open?: boolean;
  } = $props();
</script>

<details
  {open}
  class={`dldr:[&[open]>summary>span:first-child]:rotate-90 ${nested ? "dldr:mt-3" : "metadata-section dldr:border-0 dldr:border-b dldr:border-solid dldr:border-ui-line"}`}
>
  <summary
    class="dldr:flex dldr:min-h-11 dldr:cursor-pointer dldr:list-none dldr:items-center dldr:gap-2 dldr:rounded-md dldr:px-3 dldr:py-3 dldr:text-sm dldr:text-ui-ink dldr:ui-focus-row dldr:hover:bg-ui-row-hover dldr:[&::-webkit-details-marker]:hidden"
  >
    <span class="dldr:flex dldr:shrink-0"><Icon name="arrow" /></span><span
      >{title}</span
    >
  </summary>
  <div class="dldr:px-3 dldr:pb-4 dldr:pt-2">{@render children()}</div>
</details>
