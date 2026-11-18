<script lang="ts">
  import type { Snippet } from "svelte";
  import type { HTMLButtonAttributes } from "svelte/elements";
  let {
    children,
    variant = "secondary",
    tone = "normal",
    class: className = "",
    type = "button",
    ...attributes
  }: HTMLButtonAttributes & {
    children?: Snippet;
    variant?:
      | "primary"
      | "secondary"
      | "text"
      | "sidebar"
      | "navigation"
      | "icon"
      | "field-icon";
    tone?: "normal" | "muted" | "error";
  } = $props();
  const sidebar =
    "dldr:w-full dldr:min-h-10 dldr:px-2.5 dldr:py-2 dldr:justify-start dldr:border-0 dldr:bg-transparent dldr:focus-visible:bg-ui-focus-bg dldr:aria-[current=page]:bg-ui-selected dldr:aria-[current=page]:text-ui-selected-text dldr:enabled:hover:bg-ui-hover";
  const variants = {
    primary:
      "dldr:min-h-9 dldr:px-3 dldr:py-2 dldr:justify-center dldr:border-0 dldr:bg-ui-accent dldr:text-ui-on-accent dldr:[--dldr-shadow-ui-focus-control:var(--dldr-shadow-ui-focus-on-accent)] dldr:font-semibold dldr:enabled:hover:bg-ui-accent-hover",
    secondary:
      "dldr:min-h-9 dldr:px-3 dldr:py-2 dldr:justify-center dldr:border dldr:border-solid dldr:border-ui-line dldr:bg-ui-elevated dldr:enabled:hover:bg-ui-hover",
    text: "dldr:min-h-8 dldr:px-2.5 dldr:py-1.5 dldr:justify-center dldr:border-0 dldr:bg-transparent dldr:enabled:hover:bg-ui-hover",
    icon: "dldr:size-8 dldr:p-1.5 dldr:justify-center dldr:shrink-0 dldr:border-0 dldr:bg-transparent dldr:enabled:hover:bg-ui-hover",
    "field-icon":
      "dldr:size-8 dldr:p-1.5 dldr:justify-center dldr:shrink-0 dldr:border-0 dldr:bg-transparent dldr:enabled:hover:text-ui-accent",
    sidebar,
    navigation: `${sidebar} dldr:max-[620px]:flex-1 dldr:max-[620px]:flex-col dldr:max-[620px]:gap-1 dldr:max-[620px]:px-0.5 dldr:max-[620px]:text-[9px]`,
  };
</script>

<button
  {type}
  {...attributes}
  data-variant={variant}
  class={`ui-button dldr:box-border dldr:inline-flex dldr:min-w-0 dldr:items-center dldr:gap-2 dldr:rounded-md dldr:text-left dldr:font-[inherit] dldr:text-sm dldr:leading-normal dldr:cursor-pointer dldr:ui-focus-control dldr:disabled:opacity-40 dldr:disabled:cursor-default ${variants[variant]} ${variant === "primary" ? "" : tone === "error" ? "dldr:text-ui-error" : tone === "muted" || variant === "sidebar" || variant === "navigation" ? "dldr:text-ui-dim" : variant === "text" ? "dldr:text-ui-accent" : "dldr:text-ui-ink"} ${className}`}
>
  {@render children?.()}
</button>
