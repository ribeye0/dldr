<script lang="ts">
  import { t } from "$lib/i18n";
  import { listProducts, type ProductListRequest } from "$lib/api/tauri";
  import type { Product, JobSnapshot } from "$lib/model/types";
  import type { PlaylistController } from "$lib/controllers/playlist-controller.svelte";
  import { errorMessage } from "$lib/utils/format";
  import { parseProductSearch } from "$lib/utils/product-search";
  import { FilterHistory, refineSearch, type ListPosition, type MetadataSearchKind } from "$lib/utils/filter-history.svelte";
  import FilterBackBar from "$lib/components/ui/FilterBackBar.svelte";
  import ProductCard from "$lib/features/library/ProductCard.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import Icon from "$lib/components/ui/Icon.svelte";
  import Modal from "$lib/components/ui/Modal.svelte";
  import Pagination from "$lib/components/ui/Pagination.svelte";
  import { PRODUCT_PAGE_SIZE } from "$lib/model/constants";
  let { controller, onOpenDetails, onCopyWorkId, onOpenPlayer, onOpenFolder, onOpenDlsite, onOpenOnline, onDownload, getActiveJob = () => null, onTogglePlaylist, onChoosePlaylist, onError, detailLoadingWorkId, onCapturePosition = () => ({ scrollTop: 0 }), onRestorePosition = () => {} }: {
    controller:PlaylistController; onOpenDetails:(p:Product)=>void; onCopyWorkId:(id:string)=>void;
    onOpenPlayer:(p:Product)=>void; onOpenFolder:(p:Product)=>void; onOpenDlsite:(id:string)=>void; onOpenOnline?:(p:Product)=>void; onDownload?:(p:Product)=>void | Promise<void>;
    getActiveJob?:(p:Product)=>JobSnapshot|null;
    onTogglePlaylist:(p:Product)=>void; onChoosePlaylist:(p:Product,point:{x:number;y:number;trigger:HTMLButtonElement})=>void;
    onError:(message:string)=>void; detailLoadingWorkId:string|null;
    onCapturePosition?: () => ListPosition;
    onRestorePosition?: (position: ListPosition, focus?: boolean) => void;
  } = $props();
  let selectedId = $state("0"); let search = $state(""); let appliedSearch = $state("");
  let page = $state(0); let products = $state<Product[]>([]); let total = $state(0);
  let loading = $state(false); let loadError = $state("");
  let results = $state<HTMLDivElement>();
  const pageCount = $derived(Math.max(1, Math.ceil(total / PRODUCT_PAGE_SIZE)));
  let editor = $state<"create"|"rename"|null>(null); let name = $state(""); let saveError = $state("");
  const selected = $derived(controller.lists.find(list => list.id === selectedId));
  const syntaxError = $derived(parseProductSearch(search).error);
  let displayedQuery: string | null = null;
  const history = new FilterHistory<{ selectedId: string; search: string; appliedSearch: string; page: number }>();
  let pendingPosition: { position: ListPosition; focus: boolean } | null = null;
  $effect(() => {
    const revision = controller.revision;
    const id = selectedId; const query = appliedSearch; const offset = page*PRODUCT_PAGE_SIZE;
    if (!revision) return;
    const queryKey = JSON.stringify([id, query, offset]);
    const background = queryKey === displayedQuery;
    let cancelled = false;
    loading = !background; loadError = "";
    const request: ProductListRequest = {playlistId:id,search:query || null,accountIds:[],typeGroups:[],ageCategories:[],sourceGroups:[],makerKeys:[],sort:"titleAsc",limit:PRODUCT_PAGE_SIZE,offset};
    void listProducts(request).then(result => {
      if (cancelled) return;
      const last = Math.max(0,Math.ceil(result.totalCount/PRODUCT_PAGE_SIZE)-1);
      if (page > last) { page = last; return; }
      products = result.products; total = result.totalCount;
      displayedQuery = queryKey;
    }).catch(cause => { if (!cancelled) {
      if (background) onError(errorMessage(cause));
      else {products=[];total=0;loadError=errorMessage(cause);}
    } })
      .finally(() => { if (!cancelled) {
        loading = false;
        if (pendingPosition) { const pending = pendingPosition; pendingPosition = null; onRestorePosition(pending.position, pending.focus); }
      } });
    return () => {cancelled=true;};
  });
  function submitSearch() { if (!syntaxError) {history.clear();pendingPosition=null;page=0;appliedSearch=search;} }
  function goToPage(next: number) {
    if (!loading && Number.isInteger(next) && next >= 1 && next <= pageCount) page = next - 1;
  }
  export function filterBy(kind: MetadataSearchKind, value: string, position = onCapturePosition()) {
    if (loading) return false;
    const next = refineSearch(appliedSearch, kind, value);
    if (next === null) return false;
    history.push({ selectedId, search, appliedSearch, page }, position);
    pendingPosition = { position: { scrollTop: 0 }, focus: true };
    page = 0; search = next; appliedSearch = next;
    return true;
  }
  function back() {
    if (loading) return;
    const entry = history.pop();
    if (!entry) return;
    pendingPosition = { position: entry.position, focus: true };
    ({ selectedId, search, appliedSearch, page } = entry.state);
  }
  function changePlaylist(id: string) { history.clear(); pendingPosition = null; selectedId = id; page = 0; }
  function edit(kind:"create"|"rename") {name=kind==="rename"?selected?.name || $t("Playlist {0}",0):"";saveError="";editor=kind;}
  async function save(event: SubmitEvent) {
    event.preventDefault();saveError="";
    try {
      const id = await controller.edit(editor === "create" ? {action:"create",name} : {action:"rename",playlistId:selectedId,name});
      if (id) { history.clear();pendingPosition=null;selectedId=id;page=0;search="";appliedSearch="";editor=null; }
    } catch(cause) {saveError=errorMessage(cause);}
  }
  async function remove(work:Product) {
    try {await controller.edit({action:"remove",playlistId:selectedId,workId:work.workId});}
    catch(cause) {onError(errorMessage(cause));}
  }
</script>

<section class="playlists-view">
  <FilterBackBar visible={history.entries.length > 0} disabled={loading} onBack={back} />
  <header><h1>{$t("Playlists")}</h1><Button onclick={() => edit("create")} disabled={controller.busy}><Icon name="plus" />{$t("New playlist")}</Button></header>
  <div class="playlist-controls">
    <label>{$t("Playlist")}<select value={selectedId} onchange={(e) => changePlaylist(e.currentTarget.value)} disabled={!controller.lists.length}>
      {#each controller.lists as list (list.id)}<option value={list.id}>{list.name || $t("Playlist {0}",0)} ({list.workIds.length})</option>{/each}
    </select></label>
    <Button onclick={() => edit("rename")} disabled={controller.busy || !selected}>{$t("Rename playlist")}</Button>
    <Button onclick={() => controller.load().catch(cause=>onError(errorMessage(cause)))} disabled={controller.busy}>{$t("Reload")}</Button>
  </div>
  <p class="hint">{$t("Click a work's playlist icon to add or remove it from your default playlist, even before downloading. Right-click to choose another playlist.")}</p>
  <form class="playlist-search" onsubmit={(e)=>{e.preventDefault();submitSearch();}}>
    <input aria-label={$t("Search this playlist")} placeholder={$t("Search this playlist")} bind:value={search} />
    <Button type="submit" variant="primary" disabled={Boolean(syntaxError)}>{$t("Search")}</Button>
  </form>
  {#if syntaxError}<p role="alert">{$t(syntaxError)}</p>{/if}
  <div class="playlist-pagination" bind:this={results} tabindex="-1"><span>{$t(total===1?"{0} work":"{0} works",total)}</span>
    <Pagination page={page+1} {pageCount} {loading} label={$t("Playlist pages")} onChange={goToPage} />
  </div>
  {#if loadError}<p role="alert">{$t(loadError)}</p>
  {:else if loading}<p role="status">{$t("Loading…")}</p>
  {:else if !products.length}<p class="empty">{$t("No works in this playlist match your search.")}</p>
  {:else}<div class="playlist-grid work-grid">
    {#each products as product (product.workId)}<div class="playlist-item">
      <ProductCard {product} {onOpenDetails} {onCopyWorkId} {onOpenPlayer} {onOpenFolder} {onOpenDlsite} {onOpenOnline} {onDownload} activeJob={getActiveJob(product)} {onTogglePlaylist} {onChoosePlaylist}
        playlistBusy={controller.busy} inDefaultPlaylist={controller.lists.find(list=>list.id==="0")?.workIds.includes(product.workId)}
        detailLoading={detailLoadingWorkId===product.workId} onTagSearch={(v)=>filterBy("tag",v)} onPrecisionSearch={filterBy} />
      <Button variant="text" disabled={controller.busy} onclick={()=>remove(product)}><Icon name="close" />{$t("Remove from playlist")}</Button>
    </div>{/each}
  </div>{/if}
  <Pagination page={page+1} {pageCount} {loading} bottom scrollTarget={results} label={$t("Playlist pages")} onChange={goToPage} />
</section>
{#if editor}<Modal title={$t(editor==="create"?"New playlist":"Rename playlist")} dismissible={!controller.busy} onClose={()=>editor=null}>
  <form class="name-form" onsubmit={save}>
    <label>{$t("Playlist name")}<input bind:value={name} required maxlength="80" disabled={controller.busy} /></label>
    {#if saveError}<p role="alert">{$t(saveError)}</p>{/if}
    <Button type="submit" variant="primary" disabled={controller.busy || !name.trim()}>{$t("Save changes")}</Button>
  </form>
</Modal>{/if}

<style>
  header,.playlist-pagination,.playlist-controls { display:flex; flex-wrap:wrap; align-items:center; gap:12px; }
  header,.playlist-pagination { justify-content:space-between; }
  h1 { margin:0; font-size:28px; }
  header { margin-bottom:20px; }
  label { display:flex; flex-direction:column; gap:6px; font-size:14px; font-weight:600; min-width:0; }
  .playlist-controls { align-items:flex-end; }
  .playlist-controls label { width:min(360px,100%); }
  input,select { box-sizing:border-box; width:100%; min-width:0; padding:9px 12px; border:1px solid var(--dldr-color-ui-line); border-radius:7px; background:var(--dldr-color-ui-surface); color:var(--dldr-color-ui-ink); font:inherit; }
  input:focus-visible,select:focus-visible { outline:2px solid var(--dldr-color-ui-focus); }
  .hint,.playlist-pagination { font-size:13px; color:var(--dldr-color-ui-dim); }
  .hint { line-height:1.7; }
  .playlist-search { display:flex; gap:8px; margin:16px 0; }
  .playlist-search input { flex:1; border-color:var(--dldr-color-ui-focus); }
  .playlist-pagination { margin-bottom:20px; outline: none; }
  .playlist-item { display:flex; flex-direction:column; min-width:0; gap:4px; }
  .playlist-item :global(.product-card) { flex:1; }
  .name-form { display:grid; gap:16px; padding:24px; }
  .empty { padding:40px 0; color:var(--dldr-color-ui-dim); }
  [role=alert] { color:var(--dldr-color-ui-error); overflow-wrap:anywhere; }
</style>
