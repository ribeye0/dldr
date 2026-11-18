<script lang="ts">
  import type { Snippet } from "svelte";
  let { children }: { children: Snippet } = $props();
</script>

<div
  class="form-actions dldr:flex dldr:flex-wrap dldr:items-center dldr:justify-end dldr:gap-2 dldr:border-0 dldr:border-t dldr:border-solid dldr:border-ui-line dldr:pt-4"
>
  {@render children()}
</div>
