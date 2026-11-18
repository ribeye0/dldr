<script lang="ts">
  import { t } from "$lib/i18n";
  import { registerModal } from "./modal-stack";
  import { onDestroy, untrack } from "svelte";
  import type { Snippet } from "svelte";
  import Button from "./Button.svelte";
  import Icon from "./Icon.svelte";
  let {
    title,
    children,
    actions,
    open = true,
    onClose,
    dismissible = true,
    wide = false,
  }: {
    title: string;
    children: Snippet;
    actions?: Snippet;
    open?: boolean;
    onClose: () => void;
    dismissible?: boolean;
    wide?: boolean;
  } = $props();
  let dialog: HTMLDialogElement;
  let unregister: (() => void) | undefined;
  $effect(() => {
    const visible = open;
    untrack(() => {
      if (!dialog) return;
      if (visible && !dialog.open) {
        dialog.showModal();
        unregister = registerModal(dialog);
      } else if (!visible && dialog.open) {
        unregister?.();
        unregister = undefined;
        dialog.close();
      }
    });
  });
  onDestroy(() => {
    unregister?.();
    if (dialog?.open) dialog.close();
  });
</script>

<dialog
  bind:this={dialog}
  aria-label={title}
  onkeydown={(e) => {
    if (e.key === "Escape") e.stopPropagation();
  }}
  oncancel={(e) => {
    e.preventDefault();
    e.stopPropagation();
    if (dismissible) onClose();
  }}
  onclose={(e) => e.stopPropagation()}
  class={`workspace-modal dldr:box-border dldr:fixed dldr:m-auto dldr:max-w-none dldr:max-h-[90dvh] dldr:overflow-auto dldr:rounded-xl dldr:border dldr:border-solid dldr:border-ui-line-strong dldr:bg-ui-surface dldr:p-0 dldr:text-ui-ink dldr:backdrop:bg-ui-backdrop dldr:backdrop:backdrop-blur-sm ${wide ? "dldr:w-[min(760px,92vw)]" : "dldr:w-[min(480px,92vw)]"}`}
>
  <header
    class="dldr:sticky dldr:top-0 dldr:z-10 dldr:flex dldr:items-center dldr:justify-between dldr:gap-3 dldr:border-0 dldr:border-b dldr:border-solid dldr:border-ui-line dldr:bg-ui-surface dldr:px-6 dldr:py-3.5"
  >
    <h2
      tabindex="-1"
      class="dldr:outline-none dldr:m-0 dldr:min-w-0 dldr:wrap-anywhere dldr:text-base dldr:font-semibold"
    >
      {title}
    </h2>
    <Button
      variant="icon"
      aria-label={$t("Close {0}", title)}
      disabled={!dismissible}
      onclick={onClose}><Icon name="close" /></Button
    >
  </header>
  <div class="dldr:px-6 dldr:py-5">
    {@render children()}
    {#if actions}<footer
        class="dldr:mt-5 dldr:flex dldr:flex-wrap dldr:justify-end dldr:gap-2"
      >
        {@render actions()}
      </footer>{/if}
  </div>
</dialog>
