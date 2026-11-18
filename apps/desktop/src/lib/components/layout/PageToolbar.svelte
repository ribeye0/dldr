<script lang="ts">
  import { t } from "$lib/i18n";
  import type { Snippet } from "svelte";
  import Button from "../ui/Button.svelte";
  import Icon from "../ui/Icon.svelte";

  let {
    label,
    children,
    actions,
    onReload,
    reloadDisabled = false,
  }: {
    label: string;
    children?: Snippet;
    actions?: Snippet;
    onReload?: () => void;
    reloadDisabled?: boolean;
  } = $props();
</script>

<div
  role="group"
  aria-label={label}
  class="page-toolbar dldr:flex dldr:min-w-0 dldr:flex-wrap dldr:items-center dldr:gap-3"
>
  {#if children}
    <div
      class="page-toolbar-controls dldr:flex dldr:min-w-0 dldr:basis-80 dldr:grow dldr:flex-wrap dldr:items-center dldr:gap-2"
    >
      {@render children()}
    </div>
  {/if}
  {#if actions || onReload}
    <div
      class="page-toolbar-actions dldr:ml-auto dldr:flex dldr:min-w-0 dldr:flex-wrap dldr:items-center dldr:justify-end dldr:gap-2"
    >
      {@render actions?.()}
      {#if onReload}
        <Button disabled={reloadDisabled} onclick={onReload}
          ><Icon name="refresh" />{$t("Reload")}</Button
        >
      {/if}
    </div>
  {/if}
</div>
