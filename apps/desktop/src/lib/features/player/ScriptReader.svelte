<script lang="ts">
  import { onMount, tick } from "svelte";
  import type { Snippet } from "svelte";
  import type { Cue, MediaTrack } from "$lib/api/player";
  import { t } from "$lib/i18n";
  import { registerModal } from "$lib/components/ui/modal-stack";
  import Icon from "$lib/components/ui/Icon.svelte";
  import ScriptView from "./ScriptView.svelte";

  let { cues, position, offset, title, tracks, trackId, subtitleKey, loading, error,
    hasSubtitle, disabled, tracksDisabled, follow, onFollowChange, onTrack, onSeek, onClose, controls, settings }: {
    cues: Cue[]; position: number; offset: number; title: string; tracks: MediaTrack[];
    trackId: string | null; subtitleKey: string; loading: boolean; error: string;
    hasSubtitle: boolean; disabled: boolean; tracksDisabled: boolean; follow: boolean;
    onFollowChange: (follow: boolean) => void; onTrack: (id: string) => void;
    onSeek: (time: number) => void; onClose: () => void; controls: Snippet; settings: Snippet;
  } = $props();
  const storageKey = "dldr.player.reading";
  const sizes = [16,18,20,22,24,28,32];
  const spacings = [1.5,1.8,2,2.2];
  let dialog = $state<HTMLDialogElement>();
  let optionsButton = $state<HTMLButtonElement>();
  function closeOptions() { optionsOpen = false; void tick().then(() => optionsButton?.focus({preventScroll:true})); }
  let fontSize = $state(22), lineHeight = $state(1.8), showTimes = $state(true);
  let ready = $state(false), optionsOpen = $state(false), fullscreen = $state(false), fullscreenAvailable = $state(false);
  let fullscreenError = $state(false), closing = false, ownsFullscreen = false;
  let query = $state(""), match = $state(0), focusRequest = $state(0);
  const search = $derived(query.trim().toLocaleLowerCase());
  const matches = $derived(search ? cues.flatMap((cue,index) => cue.text.toLocaleLowerCase().includes(search) ? [index] : []) : []);
  const focusIndex = $derived(search ? matches[match] ?? -1 : -1);
  const canFollow = $derived(hasSubtitle && cues.length > 0 && !loading);
  $effect(() => { subtitleKey; trackId; query = ""; match = 0; });
  $effect(() => {
    if (!ready) return;
    try { localStorage.setItem(storageKey,JSON.stringify({fontSize,lineHeight,showTimes})); }
    catch { /* Preferences still work without persistent storage. */ }
  });
  onMount(() => {
    try {
      const saved = JSON.parse(localStorage.getItem(storageKey) ?? "null");
      if (saved && sizes.includes(saved.fontSize)) fontSize = saved.fontSize;
      if (saved && spacings.includes(saved.lineHeight)) lineHeight = saved.lineHeight;
      if (typeof saved?.showTimes === "boolean") showTimes = saved.showTimes;
    } catch { /* Invalid or inaccessible storage uses the reading defaults. */ }
    ready = true;
    dialog?.showModal();
    const unregister = dialog ? registerModal(dialog) : () => {};
    fullscreenAvailable = Boolean(document.fullscreenEnabled && document.documentElement.requestFullscreen);
    function changed() { fullscreen = Boolean(document.fullscreenElement); if (!fullscreen) ownsFullscreen = false; }
    changed();
    document.addEventListener("fullscreenchange",changed);
    return () => {
      document.removeEventListener("fullscreenchange",changed);
      if (ownsFullscreen && document.fullscreenElement) void document.exitFullscreen().catch(() => {});
      unregister();
      dialog?.close();
    };
  });
  async function close() {
    if (closing) return;
    closing = true;
    if (ownsFullscreen && document.fullscreenElement) await document.exitFullscreen().catch(() => {});
    onClose();
  }
  async function toggleFullscreen() {
    fullscreenError = false;
    try {
      if (document.fullscreenElement) await document.exitFullscreen();
      else {
        await document.documentElement.requestFullscreen();
        ownsFullscreen = true;
      }
    } catch { fullscreenError = true; }
  }
  function browse() { if (follow) onFollowChange(false); }
  function changeQuery(value: string) {
    query = value; match = 0; focusRequest++;
    if (value.trim()) browse();
  }
  function nextMatch(direction: number) {
    if (!matches.length) return;
    browse();
    match = (match+direction+matches.length)%matches.length; focusRequest++;
  }
  async function resume() {
    query = "";
    // Let search positioning settle before the follow effect recenters the active cue.
    await tick();
    onFollowChange(true);
  }
</script>

<dialog class="script-reader" bind:this={dialog} aria-label={$t("Script reader")}
  onpointerdown={(event) => { if (optionsOpen && !(event.target as Element).closest(".reader-options,.header-actions")) optionsOpen = false; }}
  onkeydown={(event) => { if (event.key === "Escape") { event.stopPropagation(); if (optionsOpen) { event.preventDefault(); closeOptions(); } } }}
  oncancel={(event) => { event.preventDefault(); event.stopPropagation(); if (optionsOpen) closeOptions(); else void close(); }}>
  <header class="reader-header">
    <div class="identity"><span class="eyebrow">{$t("Script reader")}</span><h2 title={title}>{title}</h2></div>
    <div class="header-actions">
      <button bind:this={optionsButton} title={$t("Reading settings")} class:pressed={optionsOpen} aria-label={$t("Reading settings")} aria-expanded={optionsOpen} aria-controls="script-reader-options" onclick={() => optionsOpen ? closeOptions() : optionsOpen = true}><Icon name="settings" /></button>
      {#if fullscreenAvailable}<button aria-label={fullscreen ? $t("Exit full screen") : $t("Full screen")} title={fullscreen ? $t("Exit full screen") : $t("Full screen")} onclick={toggleFullscreen}><Icon name={fullscreen ? "shrink" : "expand"} /></button>{/if}
      <button aria-label={$t("Back to player")} title={$t("Back to player")} onclick={close}><Icon name="close" /></button>
    </div>
  </header>
  <div class="reader-toolbar">
    <label class="track-picker"><span class="sr-only">{$t("Tracks")}</span><select value={trackId ?? ""} disabled={tracksDisabled} onchange={(event) => onTrack(event.currentTarget.value)}>
      <option value="" disabled>{$t("Choose a track")}</option>
      {#each [...new Set(tracks.map(track => track.group))] as group}<optgroup label={group || $t("Main folder")}>
        {#each tracks.filter(track => track.group === group) as track}<option value={track.id}>{track.name}</option>{/each}
      </optgroup>{/each}
    </select></label>
    <div class="search" role="search" aria-label={$t("Search script")}>
      <Icon name="search" /><input type="search" aria-label={$t("Search script")} placeholder={$t("Search script")} value={query}
        oninput={(event) => changeQuery(event.currentTarget.value)}
        onkeydown={(event) => { if (event.key === "Enter") { event.preventDefault(); nextMatch(event.shiftKey ? -1 : 1); } }} />
      {#if search}<span class="match-count" role="status">{matches.length ? `${match+1}/${matches.length}` : $t("No matches")}</span>
        <button disabled={!matches.length} aria-label={$t("Previous match")} onclick={() => nextMatch(-1)}><Icon name="chevronUp" /></button>
        <button disabled={!matches.length} aria-label={$t("Next match")} onclick={() => nextMatch(1)}><Icon name="chevronDown" /></button>{/if}
    </div>
    <button class="follow" class:pressed={follow} disabled={!canFollow} onclick={() => follow ? onFollowChange(false) : resume()}>
      <Icon name={follow ? "check" : "refresh"} />{follow ? $t("Following playback") : $t("Return to playback")}
    </button>
  </div>
  {#if optionsOpen}
    <section id="script-reader-options" class="reader-options" aria-label={$t("Reading settings")}>
      <div class="options-heading"><h3>{$t("Reading settings")}</h3><button aria-label={$t("Close {0}",$t("Reading settings"))} onclick={closeOptions}><Icon name="close" /></button></div>
      <div class="reading-options">
        <label>{$t("Font size")}<select aria-label={$t("Font size")} bind:value={fontSize}>{#each sizes as size}<option value={size}>{size}px</option>{/each}</select></label>
        <label>{$t("Line spacing")}<select aria-label={$t("Line spacing")} bind:value={lineHeight}>{#each spacings as spacing}<option value={spacing}>{spacing}</option>{/each}</select></label>
        <label class="times-toggle"><input type="checkbox" bind:checked={showTimes} />{$t("Show timestamps")}</label>
      </div>
      {@render settings()}
    </section>
  {/if}
  {#if error}<p class="reader-notice" role="alert">{$t(error.replace(/\s*\[diagnostic:[^\]]+\]$/, ""))}</p>{/if}
  {#if fullscreenError}<p class="reader-notice" role="status">{$t("Full screen is unavailable. You can keep reading in this window.")}</p>{/if}
  <div class="reader-content">
    {#if loading}<p class="empty" role="status">{$t("Loading…")}</p>
    {:else if !hasSubtitle}<div class="empty"><Icon name="script" /><p>{$t("Select a VTT subtitle file to display captions.")}</p><button onclick={() => optionsOpen = true}>{$t("Player settings")}</button></div>
    {:else if !cues.length}<p class="empty">{$t("No subtitle at this position.")}</p>
    {:else}
      {#key subtitleKey}
        <ScriptView {cues} {position} {offset} {fontSize} {lineHeight} {showTimes} {query} {focusIndex} {focusRequest}
          reading autoFollow={follow} {disabled} {onSeek} onBrowse={browse} />
      {/key}
    {/if}
  </div>
  <footer class="reader-footer" aria-label={$t("Playback controls")}>
    <p class="reading-hint">{$t("Scroll to read freely. Click a subtitle or timestamp to seek.")}</p>
    <div class="reader-controls">{@render controls()}</div>
  </footer>
</dialog>

<style>
  .script-reader { position:fixed; inset:0; margin:0; padding:0; width:100%; height:100%; max-width:none; max-height:none; box-sizing:border-box; border:0; color:var(--dldr-color-ui-ink); background:var(--dldr-color-ui-background); overflow:hidden; }
  .script-reader[open] { display:flex; flex-direction:column; }
  .script-reader::backdrop { background:var(--dldr-color-ui-background); }
  button,input,select { font:inherit; color:inherit; }
  button,select { border:1px solid var(--dldr-color-ui-line); background:var(--dldr-color-ui-surface); border-radius:8px; padding:8px 12px; }
  button { display:flex; align-items:center; justify-content:center; gap:6px; cursor:pointer; }
  button:hover:not(:disabled),button.pressed { background:var(--dldr-color-ui-status-bg); }
  button:disabled { opacity:.45; cursor:default; }
  button:focus-visible,input:focus-visible,select:focus-visible { outline:3px solid var(--dldr-color-ui-accent); outline-offset:-3px; }
  :global(.script-reader svg) { width:21px; height:21px; flex:none; }
  .reader-header { display:flex; flex:none; align-items:center; justify-content:space-between; gap:20px; padding:18px 28px 12px; }
  .identity { min-width:0; }
  .eyebrow { font-size:13px; color:var(--dldr-color-ui-dim); font-weight:650; }
  h2 { font-size:17px; margin:3px 0 0; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
  .header-actions { display:flex; gap:6px; }
  .header-actions button { width:40px; height:40px; padding:8px; border:0; background:transparent; }
  .reader-toolbar { display:flex; flex:none; align-items:center; gap:12px; padding:0 28px 14px; border-bottom:1px solid var(--dldr-color-ui-line-subtle); }
  .track-picker { flex:1; min-width:100px; max-width:440px; }
  select { width:100%; min-width:0; text-overflow:ellipsis; }
  .search { display:flex; align-items:center; gap:6px; flex:1; min-width:160px; max-width:440px; background:var(--dldr-color-ui-surface); border:1px solid var(--dldr-color-ui-line); border-radius:8px; padding:2px 8px; }
  .search:focus-within { outline:2px solid var(--dldr-color-ui-accent); outline-offset:1px; }
  .search input { width:100%; min-width:0; border:0; background:transparent; padding:7px 2px; }
  .search button { padding:5px; border:0; }
  .match-count { flex:none; font-size:12px; font-variant-numeric:tabular-nums; color:var(--dldr-color-ui-dim); }
  .follow { min-height:40px; white-space:nowrap; margin-left:auto; font-size:14px; }
  .reader-options { position:absolute; z-index:2; top:76px; right:24px; width:min(400px,calc(100% - 32px)); max-height:calc(100dvh - 100px); overflow:auto; padding:18px; box-sizing:border-box; border:1px solid var(--dldr-color-ui-line); border-radius:16px; background:var(--dldr-color-ui-surface); box-shadow:0 12px 40px var(--dldr-color-ui-shadow-strong); overscroll-behavior:contain; scrollbar-width:thin; }
  .options-heading { display:flex; align-items:center; justify-content:space-between; gap:12px; margin-bottom:16px; }
  .options-heading h3 { margin:0; font-size:16px; }
  .options-heading button { width:32px; height:32px; padding:6px; border:0; background:transparent; }
  .reading-options { display:grid; grid-template-columns:1fr 1fr; gap:14px; align-items:end; margin-bottom:18px; }
  .reading-options label { display:grid; gap:6px; font-size:14px; }
  .reading-options .times-toggle { grid-column:1 / -1; display:flex; align-items:center; align-self:center; gap:8px; }
  input[type=checkbox] { width:18px; height:18px; accent-color:var(--dldr-color-ui-accent); }
  .reader-content { flex:1; min-height:0; overflow:hidden; }
  .reader-footer { flex:none; border-top:1px solid var(--dldr-color-ui-line-subtle); background:var(--dldr-color-ui-surface); padding:8px 28px 12px; }
  .reading-hint { margin:0 0 6px; font-size:12px; text-align:center; color:var(--dldr-color-ui-dim); }
  .reader-controls { max-width:960px; margin:auto; display:grid; grid-template-columns:minmax(0,1fr) auto; gap:0 24px; align-items:center; }
  .reader-controls :global(.icon-button) { width:44px; height:44px; }
  .reader-controls :global(.primary-control) { width:52px; height:52px; }
  .reader-controls :global(.seek) { grid-column:1; grid-row:1; }
  .reader-controls :global(.time) { grid-column:1; grid-row:2; margin:0; }
  .reader-controls :global(.transport) { grid-column:2; grid-row:1 / 3; margin:0; }
  .empty { height:100%; margin:0; display:flex; flex-direction:column; align-items:center; justify-content:center; gap:12px; padding:24px; box-sizing:border-box; text-align:center; color:var(--dldr-color-ui-dim); }
  .reader-notice { margin:0; padding:8px 28px; font-size:14px; color:var(--dldr-color-ui-error); }
  .sr-only { position:absolute; width:1px; height:1px; padding:0; margin:-1px; overflow:hidden; clip:rect(0,0,0,0); border:0; }
  @media(max-width:760px) {
    .reader-header { padding:12px 16px; gap:8px; }
    .reader-toolbar { flex-wrap:wrap; gap:8px; padding:0 16px 10px; }
    .track-picker { max-width:none; flex-basis:calc(100% - 180px); }
    .search { order:3; max-width:none; flex-basis:100%; }
    .reader-footer { padding:8px 16px; }
    .reader-controls { grid-template-columns:1fr; gap:0; }
    .reader-controls :global(.transport) { grid-column:1; grid-row:3; gap:20px; }
    .reader-options { right:16px; top:72px; max-height:calc(100dvh - 88px); }
    .reading-options { gap:12px; }
  }
  @media(max-height:600px) { .reader-header { padding-top:8px; padding-bottom:8px; } .reading-hint,.eyebrow { display:none; } }
</style>
