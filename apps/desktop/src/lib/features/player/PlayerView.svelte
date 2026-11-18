<script lang="ts">
  import { tick, untrack } from "svelte";
  import { t } from "$lib/i18n";
  import type { PlayerController } from "$lib/controllers/player-controller.svelte";
  import { mediaTime } from "./subtitles";
  import ScriptView from "./ScriptView.svelte";
  import Icon from "$lib/components/ui/Icon.svelte";
  import { formatBytes } from "$lib/utils/format";
  let { player, onExpand }: { player: PlayerController; onExpand?: () => void } = $props();
  let group = $state(""); let tab = $state<"tracks" | "script">("tracks");
  let settingsOpen = $state(false); let fontSize = $state(16);
  let panel = $state<HTMLElement>(); let toggle = $state<HTMLButtonElement>();
  let drag: { id: number; startY: number; handle: HTMLButtonElement } | null = null;
  let dragging = $state(false); let dragOffset = $state(0);
  const s = $derived(player.state);
  const media = $derived(player.media);
  const track = $derived(media?.tracks.find(t => t.id === s.trackId));
  const groups = $derived([...new Set(media?.tracks.map(t => t.group) ?? [])]);
  const groupTracks = $derived(media?.tracks.filter(t => t.group === track?.group) ?? []);
  const previous = $derived(Boolean(track && groupTracks[0]?.id !== track.id));
  const next = $derived(Boolean(track && groupTracks.at(-1)?.id !== track.id));
  const blocked = $derived(player.busy || player.loading);
  const seekBlocked = $derived(player.loading || (player.busy && !player.seeking));
  let seekPreview = $state<number | null>(null);
  const position = $derived(seekPreview ?? player.seekTarget ?? s.position);
  const seekTrack = $derived(`${s.workId ?? ""}\0${s.trackId ?? ""}`);
  $effect(() => { const key = seekTrack; seekPreview = null; });
  function commitSeek() {
    if (seekPreview === null) return;
    const target = seekPreview;
    seekPreview = null;
    void player.run({action: "seek", position: target});
  }
  const notice = $derived(player.error || s.error || "");
  const duration = $derived(s.duration || (s.trackId ? s.durations[s.trackId] : 0) || 0);
  let previousWork = "";
  $effect(() => {
    const work = media?.workId ?? "";
    if (work !== previousWork) {
      previousWork = work; group = media?.tracks[0]?.group ?? "";
      tab = s.trackId && s.subtitleId ? "script" : "tracks";
      settingsOpen = false;
    }
    if (track) group = track.group;
  });
  $effect(() => {
    if (player.expanded) {
      untrack(() => onExpand?.());
      void tick().then(() => untrack(() => panel?.focus({preventScroll:true})));
    }
    return resetDrag;
  });
  function expand() { player.expanded = true; }
  function collapse() { resetDrag(); player.expanded = false; void tick().then(() => toggle?.focus({preventScroll:true})); }
  function resetDrag() {
    const previousDrag = drag;
    drag = null; dragging = false; dragOffset = 0;
    if (previousDrag?.handle.hasPointerCapture(previousDrag.id)) previousDrag.handle.releasePointerCapture(previousDrag.id);
  }
  function startDrag(event: PointerEvent & { currentTarget: HTMLButtonElement }) {
    if (!event.isPrimary || event.button !== 0 || drag) return;
    event.currentTarget.setPointerCapture(event.pointerId);
    drag = { id: event.pointerId, startY: event.clientY, handle: event.currentTarget };
    dragging = true;
  }
  function moveDrag(event: PointerEvent) {
    if (drag?.id === event.pointerId) dragOffset = Math.min(120, Math.max(0, event.clientY - drag.startY));
  }
  function endDrag(event: PointerEvent) {
    if (drag?.id !== event.pointerId) return;
    const shouldCollapse = event.clientY - drag.startY >= 64;
    resetDrag();
    if (shouldCollapse) collapse();
  }
  function cancelDrag(event: PointerEvent) { if (drag?.id === event.pointerId) resetDrag(); }
  // Pointer gestures finish in endDrag; detail=0 clicks allow keyboard/assistive activation.
  function activateHandle(event: MouseEvent) { if (event.detail === 0) collapse(); }
  function playPause() {
    if (s.trackId) void player.run({action:"pause",paused:!s.paused});
    else { const first = media?.tracks.find(t => t.group === group); if (first) void playTrack(first.id); }
  }
  async function playTrack(id: string) {
    await player.run(id === s.trackId && !s.paused ? {action:"pause",paused:true} : {action:"play",trackId:id});
    if (player.state.trackId === id && player.state.subtitleId) tab = "script";
  }
</script>

{#if media || player.expanded}
  <section class="player-dock" class:expanded={player.expanded} class:dragging style:transform={`translateY(${dragOffset}px)`} aria-label={$t("Audio player")}>
    {#if player.expanded}
      <div class="player-panel" id="audio-player-panel" role="dialog" aria-modal="false" aria-label={$t("Audio player")}
        bind:this={panel} tabindex="-1" onkeydown={(e) => { if(e.key === "Escape") { e.stopPropagation(); collapse(); } }}>
        <header class="panel-header"><button class="drag-handle" aria-label={$t("Drag down to collapse")} title={$t("Drag down to collapse")}
          onpointerdown={startDrag} onpointermove={moveDrag} onpointerup={endDrag} onpointercancel={cancelDrag} onlostpointercapture={cancelDrag}
          onclick={activateHandle}><span class="handle" aria-hidden="true"></span></button><div><h2>{$t("Now playing")}</h2><div class="header-actions">
          <button class:pressed={settingsOpen} class="icon-button" aria-label={$t("Player settings")} aria-expanded={settingsOpen} aria-controls="audio-player-settings" onclick={() => settingsOpen = !settingsOpen}><Icon name="settings" /></button>
          <button class="icon-button" aria-label={$t("Collapse player")} onclick={collapse}><Icon name="chevronDown" /></button>
        </div></div></header>
        <div class="panel-body">
          {#if player.loading}<p class="player-message" role="status">{$t("Scanning audio tracks…")}</p>{/if}
          {#if notice}<p class="player-error" role="alert">{$t(notice.replace(/\s*\[diagnostic:[^\]]+\]$/, ""))}</p>{/if}
          {#if media}
            <section class="now-playing" aria-label={$t("Playback controls")}>
              <div class="artwork">{#if player.cover}<img src={player.cover} alt="" />{:else}<Icon name="music" />{/if}</div>
              <h3 title={player.title}>{player.title}</h3>
              <p class="track-name">{track?.name ?? $t("Choose a track")}</p>
              <label class="seek"><span class="sr-only">{$t("Playback position")}</span><input type="range" min="0" max={duration || 1} step="0.1" value={Math.min(position,duration || 1)} disabled={seekBlocked || !duration || s.loading}
                onpointerdown={() => { seekPreview = position; }} oninput={(e) => { seekPreview = Number(e.currentTarget.value); }}
                onchange={commitSeek} onpointerup={commitSeek} onkeyup={commitSeek} onblur={commitSeek}
                onpointercancel={() => { seekPreview = null; }} /></label>
              <div class="time"><span>{mediaTime(position)}</span><span>{s.loading ? $t("Loading…") : duration ? mediaTime(duration) : "—"}</span></div>
              <div class="transport">
                <button class="icon-button" disabled={blocked || !previous} aria-label={$t("Previous track")} onclick={() => player.run({action:"next",direction:-1})}><Icon name="previous" /></button>
                <button class="jump-button" disabled={seekBlocked || !s.trackId || s.loading} aria-label={$t("Back 10 seconds")} onclick={() => player.run({action:"seek",position:Math.max(0,position-10)})}>−10</button>
                <button class="primary-control" disabled={blocked || !media.tracks.length} aria-label={s.paused ? $t("Play") : $t("Pause")} onclick={playPause}><Icon name={s.paused ? "play" : "pause"} /></button>
                <button class="jump-button" disabled={seekBlocked || !duration || s.loading} aria-label={$t("Forward 10 seconds")} onclick={() => player.run({action:"seek",position:Math.min(duration,position+10)})}>+10</button>
                <button class="icon-button" disabled={blocked || !next} aria-label={$t("Next track")} onclick={() => player.run({action:"next",direction:1})}><Icon name="next" /></button>
              </div>
            </section>
            {#if settingsOpen}
              <section id="audio-player-settings" class="player-settings" aria-label={$t("Player settings")}>
                <div class="audio-options"><label>{$t("Volume")}<input type="range" min="0" max="100" step="1" value={s.volume} disabled={blocked} onchange={(e) => player.run({action:"volume",volume:Number(e.currentTarget.value)})} /></label>
                <label>{$t("Playback speed")}<select value={s.speed} disabled={blocked} onchange={(e) => player.run({action:"speed",speed:Number(e.currentTarget.value)})}>{#each [0.5,0.75,1,1.25,1.5,1.75,2,2.5,3] as speed}<option value={speed}>{speed}×</option>{/each}</select></label></div>
                <label>{$t("Subtitles")}<select value={s.subtitleId ?? ""} disabled={blocked || !s.trackId} onchange={(e) => player.run({action:"subtitle",subtitleId:e.currentTarget.value || null})}><option value="">{$t("Off")}</option>{#each media.subtitles as subtitle}<option value={subtitle.id}>{subtitle.name}</option>{/each}</select></label>
                <div class="subtitle-options"><label>{$t("Subtitle offset (seconds)")}<input type="number" min="-600" max="600" step="0.1" value={s.subtitleOffset} disabled={blocked} onchange={(e) => player.run({action:"offset",offset:Number(e.currentTarget.value)})} /></label>
                <label>{$t("Subtitle size")}<select bind:value={fontSize}>{#each [14,16,18,20,24,28,32] as size}<option value={size}>{size}px</option>{/each}</select></label></div>
                <p class="player-hint">{$t("Positive subtitle offset displays captions later. Subtitle selection is saved for each track.")}</p>
                <div class="settings-actions"><button disabled={blocked} onclick={() => player.open({workId:media.workId,title:player.title,thumbnailUrl:player.cover})}><Icon name="refresh" />{$t("Rescan files")}</button><button disabled={blocked} onclick={() => player.run({action:"stop"})}><Icon name="stop" />{$t("Stop playback")}</button></div>
              </section>
            {/if}
            <div class="player-tabs" role="group" aria-label={$t("Player view")}>
              <button aria-pressed={tab === "script"} onclick={() => tab = "script"}><Icon name="script" />{$t("Script")}</button>
              <button aria-pressed={tab === "tracks"} onclick={() => tab = "tracks"}><Icon name="list" />{$t("Tracks")}<span>{media.tracks.length}</span></button>
            </div>
            {#if tab === "tracks"}
              <section class="track-section" aria-label={$t("Tracks")}>
                {#if groups.length > 1}<label class="group-picker">{$t("Folder / version")}<select bind:value={group}>{#each groups as folder}<option value={folder}>{folder || $t("Main folder")}</option>{/each}</select></label>{/if}
                {#each media.warnings ?? [] as warning}<p class="player-error" role="alert">{$t(warning)}</p>{/each}
                {#if !media.tracks.length}<p class="player-message">{media.hasArchives ? $t("No playable audio was found in these archives.") : $t("No supported audio files were found in this work.")}</p>{/if}
                <ol class="track-list">{#each media.tracks.filter(t => t.group === group) as item (item.id)}
                  <li><button class:current={item.id === s.trackId} aria-current={item.id === s.trackId ? "true" : undefined} disabled={blocked} onclick={() => playTrack(item.id)}>
                    <Icon name={item.id === s.trackId && !s.paused ? "pause" : "play"} /><span><strong>{item.name}</strong><small>{item.format} · {formatBytes(item.size)}{#if s.durations[item.id] || (item.id === s.trackId && s.duration)} · {mediaTime(s.durations[item.id] || s.duration)}{/if}</small></span>
                  </button></li>
                {/each}</ol>
              </section>
            {:else}
              <section class="script-section" aria-label={$t("Script")}>
                {#if player.subtitleError}<p class="player-error" role="alert">{$t(player.subtitleError)}</p>{/if}
                {#if player.subtitleLoading}<p class="player-message" role="status">{$t("Loading…")}</p>
                {:else if !s.subtitleId}<div class="script-empty"><Icon name="script" /><p>{$t("Select a VTT subtitle file to display captions.")}</p><button onclick={() => settingsOpen = true}>{$t("Player settings")}</button></div>
                {:else if !player.cues.length}<p class="player-message">{$t("No subtitle at this position.")}</p>
                {:else}<ScriptView cues={player.cues} {position} offset={s.subtitleOffset} {fontSize} disabled={seekBlocked || !s.trackId} onSeek={(position) => player.run({action:"seek",position})} />{/if}
              </section>
            {/if}
          {/if}
        </div>
      </div>
    {:else}
      <div class="mini-player">
        <button class="mini-identity" onclick={expand} aria-label={$t("Expand player")} aria-expanded="false" aria-controls="audio-player-panel" bind:this={toggle}>
          <span class="mini-cover">{#if player.cover}<img src={player.cover} alt="" />{:else}<Icon name="music" />{/if}</span>
          <span class="mini-titles"><strong>{player.title || $t("Audio player")}</strong><small>{track?.name ?? $t("Choose a track")}</small></span>
        </button>
        <button class="primary-control" disabled={blocked || !media?.tracks.length} aria-label={s.paused ? $t("Play") : $t("Pause")} onclick={playPause}><Icon name={s.paused ? "play" : "pause"} /></button>
        <button class="icon-button" aria-label={$t("Expand player")} aria-expanded="false" aria-controls="audio-player-panel" onclick={expand}><Icon name="chevronUp" /></button>
      </div>
      <div class="mini-progress" role="progressbar" aria-label={$t("Playback position")} aria-valuemin="0" aria-valuemax={duration || 1} aria-valuenow={Math.min(position,duration || 1)}><span style:width={`${duration ? Math.min(100,position/duration*100) : 0}%`}></span></div>
      {#if notice}<p class="mini-error" role="status">{$t(notice.replace(/\s*\[diagnostic:[^\]]+\]$/, ""))}</p>{/if}
    {/if}
  </section>
{/if}

<style>
  .player-dock { position:fixed; z-index:35; bottom:16px; right:20px; width:min(400px,calc(100vw - 236px)); box-sizing:border-box; border:1px solid var(--dldr-color-ui-line); border-radius:20px; background:var(--dldr-color-ui-surface); color:var(--dldr-color-ui-ink); box-shadow:0 10px 42px var(--dldr-color-ui-shadow-strong); overflow:hidden; }
  .player-dock.expanded { transition:transform .18s ease-out; }
  .player-dock.dragging { transition:none; }
  button,select,input { font:inherit; }
  button { color:inherit; border:1px solid var(--dldr-color-ui-line-subtle); border-radius:10px; background:var(--dldr-color-ui-surface); padding:8px 10px; cursor:pointer; }
  button:hover:not(:disabled) { background:var(--dldr-color-ui-status-bg); }
  button:focus-visible,input:focus-visible,select:focus-visible { outline:3px solid var(--dldr-color-ui-accent); outline-offset:-3px; }
  button:disabled { opacity:.4; cursor:default; }
  button.icon-button { display:grid; place-items:center; padding:8px; width:34px; height:34px; border:0; background:transparent; flex:none; }
  button.primary-control { display:grid; place-items:center; width:48px; height:48px; padding:0; border:0; border-radius:50%; background:var(--dldr-color-ui-accent); color:var(--dldr-color-ui-on-accent); flex:none; }
  button.primary-control:hover:not(:disabled) { background:var(--dldr-color-ui-accent-hover); }
  .mini-player { display:flex; align-items:center; gap:8px; padding:12px; }
  .mini-identity { display:flex; align-items:center; gap:10px; flex:1; min-width:0; border:0; padding:0; text-align:left; background:transparent; }
  .mini-cover { display:grid; place-items:center; width:48px; height:48px; border-radius:12px; overflow:hidden; flex:none; background:var(--dldr-color-ui-status-bg); }
  .mini-cover img { width:100%; height:100%; object-fit:cover; }
  .mini-titles { min-width:0; }
  .mini-titles strong,.mini-titles small { display:block; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
  .mini-titles strong { font-size:14px; font-weight:700; }
  .mini-titles small { font-size:12px; color:var(--dldr-color-ui-dim); margin-top:4px; }
  .mini-player .primary-control { width:38px; height:38px; }
  .mini-progress { height:3px; background:var(--dldr-color-ui-status-bg); }
  .mini-progress span { display:block; height:100%; background:var(--dldr-color-brand-sky); transition:width .2s linear; }
  .mini-error { margin:0; padding:8px 12px; font-size:12px; color:var(--dldr-color-ui-error); overflow-wrap:anywhere; }
  .player-panel { display:flex; flex-direction:column; max-height:calc(100dvh - 32px); outline:none; }
  .player-panel .icon-button { width:44px; height:44px; }
  .player-panel :global(svg) { width:22px; height:22px; flex:none; }
  .header-actions :global(svg) { width:24px; height:24px; }
  .seek input { height:28px; }
  .panel-header { flex:none; padding:0 16px 4px; }
  button.drag-handle { display:grid; place-items:center; width:100%; height:32px; padding:0; border:0; background:transparent; touch-action:none; user-select:none; cursor:grab; }
  .dragging .drag-handle { cursor:grabbing; }
  .handle { display:block; width:44px; height:5px; border-radius:4px; background:var(--dldr-color-ui-line); opacity:.65; }
  .panel-header > div,.header-actions { display:flex; align-items:center; justify-content:space-between; }
  .panel-header h2 { margin:0; font-size:16px; font-weight:700; letter-spacing:.04em; color:var(--dldr-color-ui-dim); }
  .header-actions { gap:4px; }
  .pressed { color:var(--dldr-color-ui-accent); }
  .panel-body { min-height:0; overflow-y:auto; overscroll-behavior:contain; padding:4px 18px 18px; scrollbar-width:thin; }
  .now-playing { text-align:center; }
  .artwork { width:112px; height:112px; margin:0 auto 12px; display:grid; place-items:center; border-radius:18px; background:var(--dldr-color-ui-status-bg); box-shadow:0 6px 18px var(--dldr-color-ui-shadow); overflow:hidden; }
  .artwork img { width:100%; height:100%; object-fit:cover; }
  .artwork :global(svg) { width:36px; height:36px; }
  .now-playing h3 { margin:0; font-size:16px; line-height:1.5; font-weight:750; overflow-wrap:anywhere; display:-webkit-box; -webkit-box-orient:vertical; -webkit-line-clamp:2; line-clamp:2; overflow:hidden; }
  .track-name { margin:4px 0 12px; font-size:14px; line-height:1.5; color:var(--dldr-color-ui-dim); overflow-wrap:anywhere; }
  input[type=range] { box-sizing:border-box; width:100%; accent-color:var(--dldr-color-ui-accent); cursor:pointer; margin:0; }
  .time { display:flex; justify-content:space-between; margin:5px 0 8px; font-size:14px; font-variant-numeric:tabular-nums; color:var(--dldr-color-ui-dim); }
  .transport { display:flex; align-items:center; justify-content:center; gap:16px; margin:0 0 14px; }
  .transport .primary-control :global(svg) { width:23px; height:23px; }
  .jump-button { border:0; background:transparent; font-size:14px; font-weight:650; padding:7px 0; width:30px; }
  .player-tabs { display:grid; grid-template-columns:1fr 1fr; gap:4px; padding:4px; margin:0 0 12px; border-radius:12px; background:var(--dldr-color-ui-surface-muted); }
  .player-tabs button { display:flex; align-items:center; justify-content:center; gap:7px; border:0; background:transparent; font-size:15px; font-weight:650; padding:9px; }
  .player-tabs button[aria-pressed=true] { background:var(--dldr-color-ui-surface); color:var(--dldr-color-ui-accent); box-shadow:0 1px 4px var(--dldr-color-ui-shadow); }
  .player-tabs span { font-size:13px; font-weight:500; color:var(--dldr-color-ui-dim); }
  label { display:flex; flex-direction:column; gap:6px; min-width:0; font-size:14px; font-weight:600; }
  select,input[type=number] { box-sizing:border-box; min-width:0; width:100%; border:1px solid var(--dldr-color-ui-line); border-radius:7px; background:var(--dldr-color-ui-surface); color:var(--dldr-color-ui-ink); padding:8px; font-size:15px; }
  .player-settings { display:grid; gap:12px; padding:14px; margin:0 0 14px; border:1px solid var(--dldr-color-ui-line-subtle); border-radius:12px; background:var(--dldr-color-ui-surface-muted); }
  .audio-options,.subtitle-options { display:grid; grid-template-columns:minmax(0,1fr) 100px; gap:12px; align-items:center; }
  .settings-actions { display:flex; flex-wrap:wrap; gap:8px; }
  .settings-actions button { display:flex; gap:5px; align-items:center; font-size:14px; }
  .player-hint { margin:0; font-size:13px; line-height:1.6; color:var(--dldr-color-ui-dim); }
  .group-picker { margin-bottom:10px; }
  .track-list { list-style:none; margin:0; padding:0; height:clamp(180px,32dvh,320px); overflow:auto; scrollbar-width:thin; overscroll-behavior:contain; }
  .track-list li + li { margin-top:6px; }
  .track-list button { width:100%; display:flex; gap:10px; align-items:center; text-align:left; border-color:transparent; }
  .track-list button.current { border-color:var(--dldr-color-ui-line-subtle); background:var(--dldr-color-ui-status-bg); }
  .track-list button > span { min-width:0; }
  .track-list strong { display:block; font-size:15px; font-weight:650; overflow-wrap:anywhere; }
  .track-list small { display:block; font-size:13px; color:var(--dldr-color-ui-dim); margin-top:4px; }
  .script-empty,.player-message { padding:30px 12px; font-size:14px; text-align:center; color:var(--dldr-color-ui-dim); line-height:1.7; }
  .script-empty { min-height:160px; display:flex; flex-direction:column; align-items:center; justify-content:center; }
  .script-empty :global(svg) { width:28px; height:28px; }
  .script-empty button { font-size:14px; }
  .player-error { padding:10px; border:1px solid currentColor; border-radius:8px; color:var(--dldr-color-ui-error); font-size:14px; overflow-wrap:anywhere; }
  .sr-only { position:absolute; width:1px; height:1px; padding:0; margin:-1px; overflow:hidden; clip:rect(0,0,0,0); border:0; }
  @media(max-width:760px) { .player-dock { width:min(380px,calc(100vw - 32px)); right:16px; } }
  @media(max-height:720px) { .artwork { width:72px; height:72px; margin-bottom:8px; } }
  @media(max-width:620px) { .player-dock { bottom:12px; right:12px; width:calc(100vw - 24px); max-width:400px; } .player-panel { max-height:calc(100dvh - 24px); } .panel-body { padding-inline:14px; } .transport { gap:12px; } }
  @media(prefers-reduced-motion:reduce) { .mini-progress span,.player-dock.expanded { transition:none; } }
</style>
