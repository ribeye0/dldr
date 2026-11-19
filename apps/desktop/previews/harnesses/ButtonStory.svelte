<script lang="ts">
  import UiButton from "$lib/components/ui/ActionButton.svelte";

  let {
    label = "Continue",
    variant = "primary",
    disabled = false,
  }: {
    label?: string;
    variant?: "primary" | "secondary" | "danger";
    disabled?: boolean;
  } = $props();
</script>

<UiButton {variant} {disabled}>{label}</UiButton>
