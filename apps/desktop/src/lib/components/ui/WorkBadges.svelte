<script lang="ts">
  import { t } from "$lib/i18n";
  import { ageLabel, ageTone, productTypeFromCode, productBadgeTone } from "$lib/utils/products";
  let {
    workType,
    ageCategory,
  }: { workType: string | null; ageCategory: string | null } = $props();
  const type = $derived(productTypeFromCode(workType));
  const tone = $derived(productBadgeTone(workType));
  const kinds: Record<string, string> = {
    audio:
      "dldr:bg-kind-audio-bg dldr:border-kind-audio-border dldr:text-kind-audio-text",
    comic:
      "dldr:bg-kind-comic-bg dldr:border-kind-comic-border dldr:text-kind-comic-text",
    illustration:
      "dldr:bg-kind-illustration-bg dldr:border-kind-illustration-border dldr:text-kind-illustration-text",
    video:
      "dldr:bg-kind-video-bg dldr:border-kind-video-border dldr:text-kind-video-text",
    game: "dldr:bg-kind-game-bg dldr:border-kind-game-border dldr:text-kind-game-text",
    other:
      "dldr:bg-kind-other-bg dldr:border-kind-other-border dldr:text-kind-other-text",
  };
  const ages: Record<string, string> = {
    all: "dldr:bg-age-all-bg dldr:border-age-all-border dldr:text-age-all-text",
    r15: "dldr:bg-age-r15-bg dldr:border-age-r15-border dldr:text-age-r15-text",
    r18: "dldr:bg-age-r18-bg dldr:border-age-r18-border dldr:text-age-r18-text",
    unknown: kinds.other,
  };
  const badge = "dldr:px-1.5 dldr:py-0.5 dldr:rounded dldr:border dldr:border-solid";
</script>

<span
  class="work-badges dldr:inline-flex dldr:flex-wrap dldr:gap-1 dldr:text-[10px] dldr:font-semibold dldr:leading-normal dldr:tracking-normal"
>
  <span
    class={`badge kind ${badge} ${kinds[tone] ?? kinds.other}`}
    data-tone={tone}
    title={$t("Product type: {0}", $t(type.label))}>{$t(type.label)}</span
  >
  <span
    class={`badge age ${badge} ${ages[ageTone(ageCategory)]}`}
    data-age={ageTone(ageCategory)}
    title={$t("Age rating")}>{$t(ageLabel(ageCategory)) || $t("Age unknown")}</span
  >
</span>
