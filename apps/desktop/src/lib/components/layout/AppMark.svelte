<script lang="ts">
  import icon from "$lib/assets/brand/dldr-icon.png";
  let { size = "small" }: { size?: "small" | "large" } = $props();
</script>

<span aria-hidden="true"
  class={`app-mark dldr:box-border dldr:grid dldr:shrink-0 dldr:place-items-center dldr:rounded-lg dldr:bg-brand-navy dldr:p-1 ${size === "large" ? "dldr:size-12" : "dldr:size-8 dldr:max-[620px]:size-6"}`}>
  <img src={icon} alt="" class="dldr:block dldr:size-full dldr:object-contain" />
</span>
