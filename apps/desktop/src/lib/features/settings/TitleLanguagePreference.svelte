<script lang="ts">
  import { tick } from "svelte";
  import { t, type UiLocale } from "$lib/i18n";
  import Button from "$lib/components/ui/Button.svelte";
  import Icon from "$lib/components/ui/Icon.svelte";
  import {
    buildLanguagePriority, defaultLanguagePriority, moveLanguagePriority, reorderLanguagePriority,
  } from "$lib/utils/name-language-priority";

  let {
    uiLocale,
    languagePriority = $bindable<string[]>([]),
    followsUi = $bindable(true),
    disabled = false,
  }: {
    uiLocale: UiLocale;
    languagePriority?: string[];
    followsUi?: boolean;
    disabled?: boolean;
  } = $props();

  const labels: Record<string, string> = {
    ja_JP: "Japanese", zh_TW: "Traditional Chinese", zh_CN: "Simplified Chinese", en_US: "English",
  };
  const order = $derived(followsUi ? defaultLanguagePriority(uiLocale) : buildLanguagePriority(languagePriority, uiLocale));
  let list = $state<HTMLOListElement>();
  let drag = $state<{ id: number; code: string; startY: number; original: string[] } | null>(null);
  let dragging = $state(false);
  let announcement = $state("");

  function chooseMode(follow: boolean) {
    if (disabled) return;
    cancelDrag();
    if (follow) languagePriority = defaultLanguagePriority(uiLocale);
    else languagePriority = [...order];
    followsUi = follow;
  }
  function announce(code: string) {
    announcement = $t("Moved {0} to position {1}", $t(labels[code]), languagePriority.indexOf(code) + 1);
  }
  async function move(code: string, direction: -1 | 1) {
    if (disabled) return;
    languagePriority = moveLanguagePriority(order, code, direction);
    announce(code);
    await tick();
    const row = list?.querySelector<HTMLElement>(`[data-title-language="${code}"]`);
    const button = row?.querySelector<HTMLButtonElement>(`[data-direction="${direction}"]`);
    // At either end, keep keyboard focus on an enabled control in the moved row.
    (button?.disabled ? row?.querySelector<HTMLButtonElement>('[data-direction]:not(:disabled)') : button)?.focus();
  }
  function startDrag(event: PointerEvent, code: string) {
    if (disabled || !event.isPrimary || event.button !== 0) return;
    event.preventDefault();
    (event.currentTarget as HTMLButtonElement).focus();
    drag = { id: event.pointerId, code, startY: event.clientY, original: [...order] };
  }
  function dragMove(event: PointerEvent) {
    if (!drag || !list || event.pointerId !== drag.id || disabled) return;
    if (!dragging && Math.abs(event.clientY - drag.startY) < 5) return;
    dragging = true;
    const bounds = list.getBoundingClientRect();
    if (event.clientX < bounds.left || event.clientX > bounds.right) return;
    const rows = [...list.querySelectorAll<HTMLElement>('[data-title-language]')];
    const target = rows.find(row => {
      const rect = row.getBoundingClientRect();
      return event.clientY >= rect.top && event.clientY <= rect.bottom;
    })?.dataset.titleLanguage;
    if (target && target !== drag.code) {
      languagePriority = reorderLanguagePriority(order, drag.code, target);
      announce(drag.code);
    }
  }
  function finishDrag(event: PointerEvent) {
    if (drag?.id === event.pointerId) { drag = null; dragging = false; }
  }
  function cancelDrag() {
    if (drag) languagePriority = drag.original;
    drag = null;
    dragging = false;
  }
</script>

<svelte:window onpointermove={dragMove} onpointerup={finishDrag}
  onpointercancel={cancelDrag} onblur={cancelDrag}
  onkeydown={(event) => { if (drag && event.key === "Escape") { event.preventDefault(); cancelDrag(); } }} />

<fieldset class="title-language" {disabled}>
  <legend>{$t("Work title language")}</legend>
  <div class="language-modes">
    <label class:active={followsUi}>
      <input type="radio" name="title-language-mode" checked={followsUi} onchange={() => chooseMode(true)} />
      <span>{$t("Follow interface language")}</span>
    </label>
    <label class:active={!followsUi}>
      <input type="radio" name="title-language-mode" checked={!followsUi} onchange={() => chooseMode(false)} />
      <span>{$t("Custom order")}</span>
    </label>
  </div>

  {#if followsUi}
    <ol class="language-summary" aria-label={$t("Name language priority")}>
      {#each order as code, index (code)}
        <li data-title-language={code}>
          {#if index > 0}<span class="order-arrow" aria-hidden="true">→</span>{/if}
          <span>{$t(labels[code])}</span>
        </li>
      {/each}
    </ol>
    <p class="hint">{$t("If a title is unavailable, use the next language in order.")}</p>
  {:else}
    <div class="custom-heading">
      <p class="hint">{$t("Drag languages to reorder, or use the arrow buttons.")}</p>
      <Button variant="text" disabled={disabled} onclick={() => chooseMode(true)}>{$t("Restore default")}</Button>
    </div>
    <ol class="language-priority" aria-label={$t("Name language priority")} bind:this={list}>
      {#each order as code, index (code)}
        {@const name = $t(labels[code])}
        <li class="language-priority-row" class:dragging={dragging && drag?.code === code} data-title-language={code}>
          <button type="button" class="drag-handle dldr:ui-focus-control" aria-label={$t("Drag {0} to reorder", name)}
            onpointerdown={(event) => startDrag(event, code)}>
            <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
              <circle cx="9" cy="5" r="1.6"/><circle cx="15" cy="5" r="1.6"/>
              <circle cx="9" cy="12" r="1.6"/><circle cx="15" cy="12" r="1.6"/>
              <circle cx="9" cy="19" r="1.6"/><circle cx="15" cy="19" r="1.6"/>
            </svg>
          </button>
          <span class="language-rank" aria-hidden="true">{index + 1}</span>
          <span class="language-name">{name}</span>
          <div class="reorder-buttons">
            <Button variant="icon" disabled={disabled || index === 0} data-direction="-1"
              aria-label={$t("Move {0} up", name)} onclick={() => move(code, -1)}><Icon name="chevronUp" /></Button>
            <Button variant="icon" disabled={disabled || index === order.length - 1} data-direction="1"
              aria-label={$t("Move {0} down", name)} onclick={() => move(code, 1)}><Icon name="chevronDown" /></Button>
          </div>
        </li>
      {/each}
    </ol>
  {/if}

  <p class="hint fallback">{$t("Other languages are used only when none of these titles are available.")}</p>
  <p class="hint">{$t("This affects work titles only. Search still includes all names.")}</p>
  <span class="sr-only" aria-live="polite" aria-atomic="true">{announcement}</span>
</fieldset>

<style>
  .title-language { box-sizing:border-box; min-width:0; width:100%; max-width:48rem; margin:0; padding:0; border:0; }
  legend { margin-bottom:12px; padding:0; font-size:17px; font-weight:700; color:var(--dldr-color-ui-ink); }
  .language-modes { display:flex; flex-wrap:wrap; gap:8px; }
  .language-modes label { display:flex; align-items:center; gap:8px; min-height:42px; padding:8px 14px; box-sizing:border-box; border:1px solid var(--dldr-color-ui-line); border-radius:8px; cursor:pointer; color:var(--dldr-color-ui-ink); }
  .language-modes label.active { background:var(--dldr-color-ui-selected); border-color:var(--dldr-color-ui-selected-border); }
  .language-modes input { margin:0; width:16px; height:16px; accent-color:var(--dldr-color-ui-accent); }
  .language-modes label:has(:focus-visible) { outline:2px solid var(--dldr-color-ui-focus); outline-offset:2px; }
  fieldset:disabled .language-modes label { cursor:default; opacity:.55; }
  .language-summary { display:flex; flex-wrap:wrap; gap:8px 0; list-style:none; padding:0; margin:18px 0 8px; color:var(--dldr-color-ui-ink); font-weight:600; }
  .language-summary li { display:flex; align-items:center; gap:10px; }
  .order-arrow { color:var(--dldr-color-ui-dim); margin-left:10px; font-weight:400; }
  .hint { margin:8px 0 0; font-size:14px; line-height:1.6; color:var(--dldr-color-ui-dim); overflow-wrap:anywhere; }
  .custom-heading { display:flex; align-items:center; justify-content:space-between; flex-wrap:wrap; gap:4px 12px; margin-top:12px; }
  .custom-heading .hint { margin:0; }
  .language-priority { display:flex; flex-direction:column; gap:8px; padding:0; margin:10px 0 0; list-style:none; }
  .language-priority-row { display:grid; grid-template-columns:28px 28px minmax(0,1fr) auto; align-items:center; gap:8px; padding:9px 10px; border:1px solid var(--dldr-color-ui-line); border-radius:8px; background:var(--dldr-color-ui-surface); color:var(--dldr-color-ui-ink); }
  .language-priority-row.dragging { border-color:var(--dldr-color-ui-accent); background:var(--dldr-color-ui-selected); box-shadow:0 2px 8px var(--dldr-color-ui-shadow); }
  .language-rank { display:grid; place-items:center; width:28px; height:28px; border-radius:6px; font-size:14px; font-weight:700; font-variant-numeric:tabular-nums; background:var(--dldr-color-ui-selected); }
  .language-name { min-width:0; overflow-wrap:anywhere; font-size:16px; font-weight:600; }
  .drag-handle { display:grid; place-items:center; padding:0; width:28px; height:36px; border:0; border-radius:5px; background:transparent; color:var(--dldr-color-ui-dim); cursor:grab; touch-action:none; }
  .drag-handle svg { width:20px; height:20px; }
  .dragging .drag-handle { cursor:grabbing; }
  .reorder-buttons { display:flex; gap:2px; }
  .fallback { margin-top:14px; }
  .sr-only { position:absolute; width:1px; height:1px; padding:0; margin:-1px; overflow:hidden; clip-path:inset(50%); white-space:nowrap; }
  @media (max-width:420px) {
    .language-priority-row { grid-template-columns:24px 24px minmax(0,1fr) auto; gap:5px; padding:8px 6px; }
    .language-rank { width:24px; height:24px; }
    .language-modes label { padding:8px 10px; }
  }
</style>
