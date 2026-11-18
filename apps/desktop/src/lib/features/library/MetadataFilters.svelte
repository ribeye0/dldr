<script lang="ts">
  import { genreName } from "$lib/i18n/genres";
  import { t } from "$lib/i18n";
  import type { ProductFilterFacets, ProductNameFacet } from "$lib/model/types";
  import type { MetadataFilters } from "$lib/api/tauri";
  let { facets, value, onCycle }: { facets: ProductFilterFacets; value: MetadataFilters; onCycle?: (kind: "genre"|"voice", name: string) => void } = $props();
  function options(kind: "genre"|"voice"): ProductNameFacet[] {
    const include = kind === "genre" ? value.genreNames : value.voiceNames;
    const exclude = kind === "genre" ? value.excludedGenreNames : value.excludedVoiceNames;
    const values = kind === "genre" ? facets.genres ?? [] : facets.voices ?? [];
    return [...new Map([...values,...[...include,...exclude].filter(name => !values.some(v => v.name === name)).map((name): ProductNameFacet => ({ name, count: 0 }))].map(v => [v.genreId ?? v.name,v])).values()];
  }
</script>
<section aria-label={$t("分類與聲優篩選")} class="metadata-filters">
  <p>{$t("點一下包含，再點排除，第三次清除。分類需全部符合；聲優符合任一位即可。聲優依已快取資料列出，重新載入可更新清單。")}</p>
  {#each ["genre","voice"] as kind}
    {@const type = kind as "genre"|"voice"}
    {@const include = type === "genre" ? value.genreNames : value.voiceNames}
    {@const exclude = type === "genre" ? value.excludedGenreNames : value.excludedVoiceNames}
    <div class="group"><strong>{type === "genre" ? $t("分類") : $t("聲優")}</strong><div class="chips">
      {#each options(type) as item (item.genreId ?? item.name)}
        {@const displayName = type === "genre" ? $genreName(item.name, item.genreId) : item.name}
        <button type="button" class:voice-filter={type === "voice"} class:included={include.includes(item.name)} class:excluded={exclude.includes(item.name)}
          aria-label={`${type === "genre" ? $t("分類") : $t("聲優")} ${displayName}：${include.includes(item.name) ? $t("包含") : exclude.includes(item.name) ? $t("排除") : $t("不限")}`}
          onclick={() => onCycle?.(type,item.name)}>{exclude.includes(item.name) ? $t("排除") : ""}{displayName} <small>{item.count}</small></button>
      {/each}
    </div></div>
  {/each}
</section>
<style>
  .metadata-filters { padding: 14px; border-bottom: 1px solid var(--dldr-color-ui-line); }
  p { font-size: 12px; color: var(--dldr-color-ui-dim); margin: 0 0 12px; }
  .group { display: grid; grid-template-columns: 68px minmax(0,1fr); gap: 10px; margin: 10px 0; font-size: 12px; }
  .chips { display: flex; flex-wrap: wrap; gap: 6px; max-height: 200px; overflow: auto; }
  button { max-width: 100%; font: inherit; padding: 6px 10px; border: 1px solid var(--dldr-color-ui-genre-border); border-radius: 6px; background: var(--dldr-color-ui-surface); color: inherit; cursor: pointer; overflow-wrap: anywhere; }
  button.voice-filter { border-radius: 999px; border-color: var(--dldr-color-ui-genre-border); }
  button.included { background: var(--dldr-color-ui-selected); border-color: var(--dldr-color-ui-accent); }
  button.excluded { color: var(--dldr-color-ui-error); border-color: currentColor; }
  small { opacity: .7; }
</style>
