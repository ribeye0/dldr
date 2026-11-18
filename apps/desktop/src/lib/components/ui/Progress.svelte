<script lang="ts">
  let {
    value,
    label,
    compact = false,
  }: { value: number | null; label: string; compact?: boolean } = $props();
</script>

<progress
  max="100"
  value={value ?? undefined}
  aria-label={label}
  class={`dldr:block dldr:w-full dldr:appearance-none dldr:border-0 dldr:rounded dldr:overflow-hidden dldr:bg-ui-progress-track dldr:text-ui-accent ${compact ? "dldr:h-0.5" : "dldr:h-2"}`}
></progress>

<style>
  progress::-webkit-progress-bar {
    background: var(--dldr-color-ui-progress-track);
  }
  progress::-webkit-progress-value {
    background: var(--dldr-color-ui-accent);
  }
  progress::-moz-progress-bar {
    background: var(--dldr-color-ui-accent);
  }
</style>
