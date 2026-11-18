<script lang="ts">
  import { t } from "$lib/i18n";
  import {
    AGE_FILTERS,
    SORT_OPTIONS,
    SOURCE_FILTERS,
    TYPE_FILTERS,
  } from "$lib/model/constants";
  import type { ProductFilterFacets } from "$lib/model/types";

  let {
    facets = { makers: [] },
    sort = "latestPurchaseDesc",
    selectedSources = [],
    selectedAges = [],
    selectedTypes = [],
    selectedMakers = [],
    onSetSort,
    onClearSources,
    onToggleSource,
    onClearAges,
    onToggleAge,
    onClearTypes,
    onToggleType,
    onClearMakers,
    onToggleMaker,
  }: {
    facets?: ProductFilterFacets;
    sort?: string;
    selectedSources?: string[];
    selectedAges?: string[];
    selectedTypes?: string[];
    selectedMakers?: string[];
    onSetSort?: (value: string) => void;
    onClearSources?: () => void;
    onToggleSource?: (value: string) => void;
    onClearAges?: () => void;
    onToggleAge?: (value: string) => void;
    onClearTypes?: () => void;
    onToggleType?: (value: string) => void;
    onClearMakers?: () => void;
    onToggleMaker?: (key: string) => void;
  } = $props();

</script>

<div id="library-filter-grid" class="filter-panel">
  <div class="filter-group sort-filter">
    <span>{$t("Sort")}</span>
    <div class="toggle-row">
      {#each SORT_OPTIONS as [value, label] (value)}
        <button
          class:active={sort === value}
          type="button"
          onclick={() => onSetSort?.(value)}
        >
          <span class="filter-chip-label">{$t(label)}</span>
        </button>
      {/each}
    </div>
  </div>

  <div class="filter-group">
    <span>{$t("Source")}</span>
    <div class="toggle-row">
      <button
        class:active={selectedSources.length === 0}
        type="button"
        onclick={onClearSources}
      >
        <span class="filter-chip-label">{$t("Any")}</span>
      </button>
      {#each SOURCE_FILTERS as [value, label] (value)}
        <button
          class:active={selectedSources.includes(value)}
          data-source-filter={value}
          type="button"
          onclick={() => onToggleSource?.(value)}
        >
          <span class="filter-chip-label">{$t(label)}</span>
        </button>
      {/each}
    </div>
  </div>

  <div class="filter-group">
    <span>{$t("Age")}</span>
    <div class="toggle-row">
      <button
        class:active={selectedAges.length === 0}
        type="button"
        onclick={onClearAges}
      >
        <span class="filter-chip-label">{$t("Any")}</span>
      </button>
      {#each AGE_FILTERS as [value, label] (value)}
        <button
          class:active={selectedAges.includes(value)}
          data-age-filter={value}
          type="button"
          onclick={() => onToggleAge?.(value)}
        >
          <span class="filter-chip-label">{$t(label)}</span>
        </button>
      {/each}
    </div>
  </div>

  <div class="filter-group">
    <span>{$t("Type")}</span>
    <div class="toggle-row">
      <button
        class:active={selectedTypes.length === 0}
        type="button"
        onclick={onClearTypes}
      >
        <span class="filter-chip-label">{$t("Any")}</span>
      </button>
      {#each TYPE_FILTERS as [value, label] (value)}
        <button
          class:active={selectedTypes.includes(value)}
          data-type-filter={value}
          type="button"
          onclick={() => onToggleType?.(value)}
        >
          <span class="filter-chip-label">{$t(label)}</span>
        </button>
      {/each}
    </div>
  </div>

  <div class="filter-group maker-filter">
    <span>{$t("Makers")}</span>
    <div class="toggle-row">
      <button
        class:active={selectedMakers.length === 0}
        type="button"
        onclick={onClearMakers}
      >
        <span class="filter-chip-label">{$t("Any")}</span>
      </button>
      {#each facets.makers as maker (maker.key)}
        <button
          class:active={selectedMakers.includes(maker.key)}
          type="button"
          title={`${maker.name} (${maker.count})`}
          onclick={() => onToggleMaker?.(maker.key)}
        >
          <span class="filter-chip-label">{maker.name}</span>
          <small>{maker.count}</small>
        </button>
      {/each}
    </div>
  </div>

</div>

<style>
  .filter-panel {
    display: grid;
    flex: 0 0 auto;
    gap: 10px;
    min-width: 0;
    padding: 14px;
    border-bottom: 1px solid var(--border);
    background: var(--panel-soft);
  }

  .filter-group {
    display: grid;
    grid-template-columns: 78px minmax(0, 1fr);
    gap: 10px;
    align-items: start;
    min-width: 0;
  }

  .filter-group > span {
    padding-top: 6px;
    color: var(--text-subtle);
    font-size: 12px;
    font-weight: 700;
  }

  .toggle-row {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    min-width: 0;
  }

  .maker-filter .toggle-row {
    align-items: flex-start;
  }

  .toggle-row button {
    display: inline-flex;
    align-items: center;
    justify-content: flex-start;
    min-width: 0;
    max-width: 210px;
    height: 30px;
    padding: 0 10px;
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    color: var(--muted);
    background: var(--field);
    font: inherit;
    font-size: 12px;
    font-weight: 650;
    cursor: pointer;
    overflow: hidden;
    text-overflow: ellipsis;
    transition:
      border-color 120ms ease,
      background-color 120ms ease,
      color 120ms ease;
    white-space: nowrap;
  }

  .toggle-row button:focus-visible {
    outline: none;
    box-shadow: var(--focus-ring);
  }

  .toggle-row button.active {
    border-color: var(--accent);
    color: var(--text-strong);
    background: var(--accent-muted);
  }

  .toggle-row button[data-age-filter],
  .toggle-row button[data-source-filter],
  .toggle-row button[data-type-filter] {
    --filter-color: var(--dldr-color-kind-other-text);
    --filter-soft: var(--dldr-color-kind-other-bg);
    border-color: color-mix(
      in srgb,
      var(--filter-color) 22%,
      var(--border-strong)
    );
    color: color-mix(in srgb, var(--filter-color) 28%, var(--text-subtle));
    background: color-mix(in srgb, var(--filter-soft) 24%, var(--field));
  }

  .toggle-row button[data-age-filter]:hover:not(:disabled),
  .toggle-row button[data-source-filter]:hover:not(:disabled),
  .toggle-row button[data-type-filter]:hover:not(:disabled) {
    border-color: color-mix(
      in srgb,
      var(--filter-color) 72%,
      var(--border-strong)
    );
    color: color-mix(in srgb, var(--filter-color) 82%, var(--text-strong));
  }

  .toggle-row button[data-age-filter].active,
  .toggle-row button[data-source-filter].active,
  .toggle-row button[data-type-filter].active {
    border-color: color-mix(in srgb, var(--filter-color) 82%, white);
    color: var(--text-strong);
    background: color-mix(in srgb, var(--filter-color) 28%, var(--field));
  }

  .filter-chip-label {
    display: block;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .toggle-row button small {
    flex: 0 0 auto;
    margin-left: 6px;
    color: var(--text-subtle);
    font-size: 11px;
    font-weight: 700;
  }

  @media (max-width: 720px) {
    .filter-group {
      grid-template-columns: 1fr;
    }

    .filter-group > span {
      padding-top: 0;
    }

    .toggle-row button {
      flex: 1 1 130px;
    }
  }
  /* Adapt the preserved production filters to the draft's semantic palette. */
  .toggle-row button[data-age-filter],
  .toggle-row button[data-type-filter],
  .toggle-row button[data-source-filter] {
    --filter-color: var(--dldr-color-kind-other-text);
    --filter-soft: var(--dldr-color-kind-other-bg);
  }
  .toggle-row button[data-age-filter="all"] {
    --filter-color: var(--dldr-color-age-all-text);
    --filter-soft: var(--dldr-color-age-all-bg);
  }
  .toggle-row button[data-age-filter="r15"] {
    --filter-color: var(--dldr-color-age-r15-text);
    --filter-soft: var(--dldr-color-age-r15-bg);
  }
  .toggle-row button[data-age-filter="r18"] {
    --filter-color: var(--dldr-color-age-r18-text);
    --filter-soft: var(--dldr-color-age-r18-bg);
  }
  .toggle-row button[data-type-filter="audio"] {
    --filter-color: var(--dldr-color-kind-audio-text);
    --filter-soft: var(--dldr-color-kind-audio-bg);
  }
  .toggle-row button[data-type-filter="video"] {
    --filter-color: var(--dldr-color-kind-video-text);
    --filter-soft: var(--dldr-color-kind-video-bg);
  }
  .toggle-row button[data-type-filter="game"] {
    --filter-color: var(--dldr-color-kind-game-text);
    --filter-soft: var(--dldr-color-kind-game-bg);
  }
  .toggle-row button[data-type-filter="image"] {
    --filter-color: var(--dldr-color-kind-image-text);
    --filter-soft: var(--dldr-color-kind-image-bg);
  }
  .toggle-row button[data-type-filter="other"] {
    --filter-color: var(--dldr-color-kind-other-text);
    --filter-soft: var(--dldr-color-kind-other-bg);
  }
  .toggle-row button[data-source-filter="owned"] {
    --filter-color: var(--dldr-color-source-owned-text);
    --filter-soft: var(--dldr-color-source-owned-bg);
  }
  .toggle-row button[data-source-filter="localOnly"] {
    --filter-color: var(--dldr-color-source-local-text);
    --filter-soft: var(--dldr-color-source-local-bg);
  }
  /* Keep the requested kind/rating swatches exact, including selected filters. */
  .toggle-row button[data-type-filter],
  .toggle-row button[data-age-filter] {
    background: var(--filter-soft);
    color: var(--filter-color);
  }
  .toggle-row button[data-type-filter].active,
  .toggle-row button[data-age-filter].active {
    color: var(--filter-color);
    background: var(--filter-soft);
    border-color: var(--border-strong);
    box-shadow: inset 0 0 0 1px var(--border-strong);
  }
  .toggle-row button[data-type-filter]:hover:not(:disabled),
  .toggle-row button[data-age-filter]:hover:not(:disabled) {
    color: var(--filter-color);
  }
  .toggle-row button[data-type-filter]:focus-visible,
  .toggle-row button[data-age-filter]:focus-visible {
    outline: 2px solid var(--border-strong);
    outline-offset: 2px;
  }
  @media (forced-colors: active) {
    .toggle-row button:focus-visible {
      outline: 1px solid Highlight;
      outline-offset: -2px;
    }
  }
</style>
