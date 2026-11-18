<script lang="ts">
  let {
    label,
    options,
    value,
    onchange,
    variant = "filter",
  }: {
    label: string;
    options: { value: string; label: string; count?: number }[];
    value: string;
    onchange: (value: string) => void;
    variant?: "filter" | "tabs";
  } = $props();
</script>

<div
  role="group"
  aria-label={label}
  class={variant === "tabs"
    ? "tabs dldr:flex dldr:flex-wrap dldr:gap-1 dldr:border-0 dldr:border-b dldr:border-solid dldr:border-ui-line"
    : "switches dldr:flex dldr:flex-wrap dldr:gap-1"}
>
  {#each options as option}
    <button
      type="button"
      aria-pressed={value === option.value}
      onclick={() => onchange(option.value)}
      class={`dldr:box-border dldr:inline-flex dldr:items-center dldr:justify-center dldr:gap-2 dldr:min-h-8 dldr:px-3 dldr:py-2 dldr:font-[inherit] dldr:text-sm dldr:leading-normal dldr:cursor-pointer dldr:ui-focus-control dldr:hover:bg-ui-hover ${variant === "tabs" ? `dldr:min-h-10 dldr:border-0 dldr:border-b-2 dldr:border-solid dldr:rounded-t-md dldr:bg-transparent ${value === option.value ? "dldr:border-ui-accent dldr:text-ui-ink" : "dldr:border-transparent dldr:text-ui-dim"}` : `dldr:rounded-md dldr:border-0 ${value === option.value ? "dldr:bg-ui-selected dldr:text-ui-ink" : "dldr:bg-transparent dldr:text-ui-dim"}`} `}
    >
      {option.label}{#if option.count !== undefined}<span
          class="dldr:text-xs dldr:text-ui-dim">{option.count}</span
        >{/if}
    </button>
  {/each}
</div>
