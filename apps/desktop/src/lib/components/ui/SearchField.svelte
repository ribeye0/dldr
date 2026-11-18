<script lang="ts">
  import { t } from "$lib/i18n";
  import { tick } from "svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import Icon from "$lib/components/ui/Icon.svelte";
  let {
    value = $bindable(""),
    label,
    placeholder,
    compact = false,
    clearable = false,
    disabled = false,
    onClear,
    invalid = false,
    describedBy,
    suggest,
  }: {
    value: string;
    label: string;
    placeholder: string;
    compact?: boolean;
    clearable?: boolean;
    disabled?: boolean;
    onClear?: () => void;
    invalid?: boolean;
    describedBy?: string;
    suggest?: (value: string, caret: number) => { id: string; label: string; description?: string; value: string; caret: number }[];
  } = $props();
  let input: HTMLInputElement;
  const id = $props.id();
  const listId = `${id}-completions`;
  let focused = $state(false);
  let composing = $state(false);
  let dismissed = $state(false);
  let caret = $state(0);
  let active = $state(-1);
  const suggestions = $derived(suggest?.(value, caret) ?? []);
  const expanded = $derived(focused && !disabled && !composing && !dismissed && suggestions.length > 0);

  function refresh() {
    caret = input.selectionStart ?? input.value.length;
    active = -1;
    dismissed = false;
  }
  async function complete(index: number) {
    const suggestion = suggestions[index];
    if (!suggestion) return;
    value = suggestion.value;
    caret = suggestion.caret;
    active = -1;
    dismissed = false;
    await tick();
    input.focus();
    input.setSelectionRange(caret, caret);
  }
  function keydown(event: KeyboardEvent) {
    if (!suggest || event.isComposing || composing || event.ctrlKey || event.metaKey || event.altKey) return;
    if (event.key === "Escape" && expanded) {
      event.preventDefault(); event.stopPropagation(); dismissed = true; active = -1;
    } else if ((event.key === "ArrowDown" || event.key === "ArrowUp") && suggestions.length) {
      event.preventDefault(); dismissed = false;
      active = active < 0 ? (event.key === "ArrowDown" ? 0 : suggestions.length - 1)
        : (active + (event.key === "ArrowDown" ? 1 : -1) + suggestions.length) % suggestions.length;
      void tick().then(() => document.getElementById(`${listId}-${active}`)?.scrollIntoView({ block: "nearest" }));
    } else if (expanded && ((event.key === "Tab" && !event.shiftKey) || (event.key === "Enter" && active >= 0))) {
      event.preventDefault(); void complete(active < 0 ? 0 : active);
    }
  }
</script>

<div class="search-container">
<div
  class:invalid
  class="search-field dldr:flex dldr:min-w-0 dldr:flex-1 dldr:items-center dldr:gap-1 dldr:rounded-md dldr:border dldr:border-solid dldr:border-ui-focus dldr:bg-ui-input dldr:px-1 dldr:text-ui-dim"
>
  <span class="dldr:flex dldr:size-8 dldr:shrink-0 dldr:items-center dldr:justify-center"
    ><Icon name="search" /></span
  >
  <input
    bind:this={input}
    bind:value
    aria-label={label}
    aria-invalid={invalid || undefined}
    aria-describedby={describedBy}
    role={suggest ? "combobox" : undefined}
    aria-autocomplete={suggest ? "list" : undefined}
    aria-expanded={suggest ? expanded : undefined}
    aria-controls={suggest ? listId : undefined}
    aria-activedescendant={expanded && suggestions[active] ? `${listId}-${active}` : undefined}
    autocomplete="off"
    spellcheck={false}
    {placeholder}
    {disabled}
    onfocus={() => { focused = true; refresh(); }}
    onblur={() => { focused = false; active = -1; }}
    oninput={refresh}
    onclick={refresh}
    onselect={() => {
      if ((input.selectionStart ?? input.value.length) !== caret) refresh();
    }}
    onkeydown={keydown}
    onkeyup={(event) => { if (["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) refresh(); }}
    oncompositionstart={() => composing = true}
    oncompositionend={() => { composing = false; refresh(); }}
    class={`dldr:min-w-0 dldr:w-full dldr:border-0 dldr:bg-transparent dldr:py-0 dldr:pl-0 ${clearable ? "dldr:pr-0" : "dldr:pr-2"} dldr:font-[inherit] dldr:text-ui-ink dldr:outline-none dldr:placeholder:text-ui-placeholder ${compact ? "dldr:h-8 dldr:text-xs" : "dldr:h-9 dldr:text-base"}`}
  />
  {#if clearable}<span
      class="search-clear-slot dldr:flex dldr:size-8 dldr:shrink-0 dldr:items-center dldr:justify-center"
      >{#if value}<Button
          variant="field-icon"
          tone="muted"
          aria-label={$t("Clear search")}
          {disabled}
          onclick={() => {
            value = "";
            caret = 0;
            input.focus();
            onClear?.();
          }}><Icon name="close" /></Button
        >{/if}</span
    >{/if}
</div>
{#if expanded}
  <div class="completion-panel">
    <div id={listId} role="listbox" aria-label={$t("Search suggestions")} class="completion-list">
      {#each suggestions as suggestion, index (suggestion.id)}
        <button type="button" role="option" tabindex="-1" id={`${listId}-${index}`}
          aria-selected={active === index} class:active={active === index}
          onpointerdown={(event) => event.preventDefault()} onclick={() => void complete(index)}>
          <span>{suggestion.label}</span>
          {#if suggestion.description}<small>{suggestion.description}</small>{/if}
        </button>
      {/each}
    </div>
    <p>{$t("↑↓ choose · Tab complete · Enter apply · Esc close")}</p>
  </div>
{/if}
</div>

<style>
  .search-container { position: relative; flex: 1; min-width: 0; }
  .invalid { border-color: var(--dldr-color-ui-error); }
  .completion-panel { position: absolute; top: calc(100% + 6px); left: 0; right: 0; z-index: 30; min-width: 0; border: 1px solid var(--dldr-color-ui-line-strong); border-radius: 8px; background: var(--dldr-color-ui-elevated); color: var(--dldr-color-ui-ink); box-shadow: 0 8px 24px #0002; overflow: hidden; }
  .completion-list { max-height: 280px; overflow: auto; padding: 4px; }
  .completion-list button { display: flex; align-items: baseline; flex-wrap: wrap; gap: 4px 12px; width: 100%; padding: 10px; border: 0; border-radius: 5px; text-align: left; font: inherit; font-size: 14px; color: inherit; background: transparent; cursor: pointer; overflow-wrap: anywhere; }
  .completion-list button:hover, .completion-list button.active { background: var(--dldr-color-ui-selected); color: var(--dldr-color-ui-selected-text); }
  .completion-list button > span { font-weight: 600; }
  .completion-list small { color: var(--dldr-color-ui-dim); font-size: 12px; }
  .completion-panel p { margin: 0; padding: 8px 12px; border-top: 1px solid var(--dldr-color-ui-line-subtle); color: var(--dldr-color-ui-dim); font-size: 11px; line-height: 1.5; }
</style>
