<script lang="ts">
  import { t } from "$lib/i18n";
  import { notificationPortal } from "./modal-stack";
  import type { Toast } from "$lib/model/types";
  import Button from "./Button.svelte";
  import Icon from "./Icon.svelte";
  import { splitDiagnosticMessage, type DiagnosticTarget } from "$lib/utils/diagnostic-message";

  let {
    toasts,
    onDismiss,
    onViewActivity,
  }: {
    toasts: Toast[];
    onDismiss: (id: string) => void;
    onViewActivity?: (target: DiagnosticTarget) => void;
  } = $props();
</script>

{#if toasts.length > 0}
  <section
    popover="manual"
    use:notificationPortal
    class="toast-stack"
    aria-label={$t("Notifications")}
    aria-live="polite"
  >
    {#each toasts as toast (toast.id)}
      {@const parsed = splitDiagnosticMessage(toast.message)}
      <article
        class="toast"
        class:error={toast.kind === "error"}
        class:success={toast.kind === "success"}
        role={toast.kind === "error" ? "alert" : "status"}
      >
        <div class="toast-marker" aria-hidden="true"></div>
        <div class="dldr:min-w-0 dldr:py-2.5">
          <p>{parsed.message}</p>
          {#if parsed.diagnostic && onViewActivity}
            <div class="dldr:mt-2">
              <Button variant="text" onclick={() => {
                if (!parsed.diagnostic) return;
                onViewActivity?.({ ...parsed.diagnostic, message: parsed.message });
                onDismiss(toast.id);
              }}>{$t("View in Activity")} <Icon name="arrow" /></Button>
            </div>
          {:else if parsed.diagnostic}
            <code class="dldr:mt-2 dldr:block dldr:wrap-anywhere dldr:text-xs dldr:text-ui-dim">{parsed.diagnostic.runId}<br />{parsed.diagnostic.operationId}</code>
          {/if}
        </div>
        <button
          class="toast-close"
          type="button"
          aria-label={$t("Dismiss notification")}
          onclick={() => onDismiss(toast.id)}
        >
          <svg class="toast-close-icon" viewBox="0 0 24 24" aria-hidden="true">
            <path d="M18 6 6 18M6 6l12 12" />
          </svg>
        </button>
      </article>
    {/each}
  </section>
{/if}

<style>
  .toast-stack {
    position: fixed;
    inset: var(--toast-top, 56px) auto auto 50%;
    transform: translateX(-50%);
    margin: 0;
    padding: 8px 12px 18px;
    border: 0;
    background: transparent;
    height: max-content;
    min-height: 0;
    max-height: min(380px, calc(100dvh - var(--toast-top, 56px) - 16px));
    overflow-y: auto;
    z-index: 120;
    display: grid;
    /* Popover rows must follow their text height, including inside a tall dialog. */
    grid-auto-rows: max-content;
    align-content: start;
    align-items: start;
    gap: 8px;
    width: min(464px, calc(100vw - 12px));
    box-sizing: border-box;
    pointer-events: none;
  }

  .toast {
    --toast-color: var(--accent);
    --toast-bg: var(--accent-muted);

    display: grid;
    grid-template-columns: 4px minmax(0, 1fr) auto;
    gap: 10px;
    align-items: center;
    min-height: 48px;
    height: max-content;
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    background: var(--panel-raised);
    box-shadow: 0 18px 42px var(--dldr-color-ui-shadow-strong);
    overflow: hidden;
    pointer-events: auto;
  }

  .toast.success {
    --toast-color: var(--accent);
    --toast-bg: var(--accent-muted);
  }

  .toast.error {
    --toast-color: var(--danger);
    --toast-bg: var(--dldr-color-ui-error-bg);
  }

  .toast-marker {
    align-self: stretch;
    background: var(--toast-color);
  }

  .toast p {
    min-width: 0;
    margin: 0;
    padding: 0;
    color: var(--text);
    font-size: 13px;
    line-height: 1.35;
    overflow-wrap: anywhere;
  }

  .toast-close {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    min-width: 30px;
    height: 30px;
    margin-right: 8px;
    padding: 0;
    border: 1px solid transparent;
    border-radius: 6px;
    color: var(--muted);
    background: transparent;
    cursor: pointer;
  }

  .toast-close:hover {
    border-color: var(--border-strong);
    color: var(--text);
    background: var(--toast-bg);
  }

  .toast-close-icon {
    width: 16px;
    height: 16px;
    fill: none;
    stroke: currentColor;
    stroke-linecap: round;
    stroke-linejoin: round;
    stroke-width: 2.2;
  }
</style>
