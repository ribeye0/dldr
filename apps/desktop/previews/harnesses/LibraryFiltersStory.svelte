<script lang="ts">
  import LibraryFilters from "$lib/features/library/LibraryFilters.svelte";
  import type { ProductFilterFacets } from "$lib/model/types";

  let { active = false }: { active?: boolean } = $props();



  const facets: ProductFilterFacets = {
    makers: [
      { key: "name:North Window Studio", name: "North Window Studio", count: 18 },
      { key: "name:An Especially Long Circle Name For Truncation", name: "An Especially Long Circle Name For Truncation", count: 7 },
      { key: "name:Night Signal", name: "Night Signal", count: 4 },
    ],

  };

  let sort = $state("latestPurchaseDesc");
  let selectedSources = $state<string[]>([]);
  let selectedAges = $state<string[]>([]);
  let selectedTypes = $state<string[]>([]);
  let selectedMakers = $state<string[]>([]);
  let lastAction = $state("No filter change yet");

  $effect(() => {
    selectedSources = active ? ["owned"] : [];
    selectedAges = active ? ["all"] : [];
    selectedTypes = active ? ["audio"] : [];
    selectedMakers = active ? ["name:North Window Studio"] : [];
  });

  function toggle(values: string[], value: string) {
    return values.includes(value) ? values.filter((item) => item !== value) : [...values, value];
  }

</script>

<main class="story-surface">
  <LibraryFilters
    {facets}
    {sort}
    selectedSources={selectedSources}
    selectedAges={selectedAges}
    selectedTypes={selectedTypes}
    selectedMakers={selectedMakers}
    onSetSort={(value) => { sort = value; lastAction = `Sort: ${value}`; }}
    onClearSources={() => { selectedSources = []; lastAction = "Any source"; }}
    onToggleSource={(value) => { selectedSources = toggle(selectedSources, value); lastAction = `Source: ${value}`; }}
    onClearAges={() => { selectedAges = []; lastAction = "Any age"; }}
    onToggleAge={(value) => { selectedAges = toggle(selectedAges, value); lastAction = `Age: ${value}`; }}
    onClearTypes={() => { selectedTypes = []; lastAction = "Any type"; }}
    onToggleType={(value) => { selectedTypes = toggle(selectedTypes, value); lastAction = `Type: ${value}`; }}
    onClearMakers={() => { selectedMakers = []; lastAction = "Any maker"; }}
    onToggleMaker={(value) => { selectedMakers = toggle(selectedMakers, value); lastAction = `Maker: ${value}`; }}
  />
  <p aria-live="polite">{lastAction}</p>
</main>

<style>
  .story-surface {
    width: min(1120px, calc(100vw - 48px));
    margin: 0 auto;
    padding: 24px 0;
  }

  p {
    margin: 12px 0 0;
    color: var(--text-subtle);
    font-size: 12px;
    text-align: right;
  }

  @media (max-width: 720px) {
    .story-surface { width: calc(100vw - 24px); padding: 12px 0; }
  }
</style>
