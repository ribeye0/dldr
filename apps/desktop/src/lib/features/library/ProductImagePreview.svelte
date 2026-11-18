<script lang="ts">
  import { t } from "$lib/i18n";
  import type { ProductImagePreview } from "$lib/model/types";
  import Modal from "$lib/components/ui/Modal.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import Icon from "$lib/components/ui/Icon.svelte";
  let {
    preview,
    onClose,
    onSave,
  }: {
    preview: ProductImagePreview;
    onClose?: () => void;
    onSave: (workId: string) => Promise<boolean>;
  } = $props();
  let saving = $state(false);
  let message = $state("");
  let failed = $state(false);
  async function save() {
    if (saving) return;
    saving = true;
    message = "";
    failed = false;
    try {
      if (await onSave(preview.workId)) message = $t("Image saved.");
    } catch (error) {
      failed = true;
      message = String(error);
    } finally {
      saving = false;
    }
  }
</script>

<Modal
  title={preview.title}
  wide
  dismissible={!saving}
  onClose={() => onClose?.()}
>
  <img
    src={preview.url}
    alt={$t("Cover of {0}", preview.title)}
    class="dldr:block dldr:mx-auto dldr:max-w-full dldr:max-h-[65dvh] dldr:object-contain"
  />
  {#if message}<p
      role={failed ? "alert" : "status"}
      class={failed ? "dldr:text-ui-error" : "dldr:text-ui-dim"}
    >
      {message}
    </p>{/if}
  {#snippet actions()}<Button disabled={saving} onclick={save}
      ><Icon name="downloads" />{saving ? $t("Saving…") : $t("Save image")}</Button
    >{/snippet}
</Modal>
