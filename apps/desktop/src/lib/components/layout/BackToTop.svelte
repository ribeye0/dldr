<script lang="ts">
  import { t } from "$lib/i18n";
  import Icon from "../ui/Icon.svelte";
  let {
    target,
    class: className = "",
  }: { target?: () => HTMLElement | null; class?: string } = $props();
  function top(event: MouseEvent) {
    const scroller =
      target?.() ?? (event.currentTarget as HTMLElement).closest("dialog");
    if (!scroller) return;
    scroller
      .querySelector<HTMLElement>("h1, h2")
      ?.focus({ preventScroll: true });
    scroller.scrollTo({
      top: 0,
      behavior: matchMedia("(prefers-reduced-motion: reduce)").matches
        ? "instant"
        : "smooth",
    });
  }
</script>

<button
  type="button"
  aria-label={$t("Back to top")}
  title={$t("Back to top")}
  onclick={top}
  class={`dldr:grid dldr:size-10 dldr:shrink-0 dldr:place-items-center dldr:rounded-full dldr:border dldr:border-solid dldr:border-ui-selected-border dldr:bg-ui-selected-strong dldr:p-0 dldr:text-ui-ink dldr:cursor-pointer dldr:ui-focus-control ${className}`}
  ><Icon name="up" /></button
>
