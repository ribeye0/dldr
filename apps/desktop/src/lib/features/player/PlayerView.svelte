<script lang="ts">
  import { tick, untrack } from "svelte";
  import { t } from "$lib/i18n";
  import type { PlayerController } from "$lib/controllers/player-controller.svelte";
  import { mediaTime } from "./subtitles";
  import ScriptView from "./ScriptView.svelte";
  import ScriptReader from "./ScriptReader.svelte";
  import Icon from "$lib/components/ui/Icon.svelte";
  import { formatBytes } from "$lib/utils/format";
  let { player, onExpand }: { player: PlayerController; onExpand?: () => void } = $props();
  const speeds = [0.5,0.75,1,1.25,1.5,1.75,2,2.5,3];
  let lastVolume = 70;
  let group = $state(""); let tab = $state<"tracks" | "script">("tracks");
  let settingsOpen = $state(false); let fontSize = $state(16);
  let readerOpen = $state(false);
  let readerButton = $state<HTMLButtonElement>();
  let readerOrigin: HTMLButtonElement | null = null;
  function closeReader() { readerOpen = false; void tick().then(() => (readerOrigin?.isConnected ? readerOrigin : readerButton)?.focus({preventScroll:true})); }
  let followPlayback = $state(true);
  function setFollowPlayback(enabled: boolean) { followPlayback = enabled; }
  let panel = $state<HTMLElement>(); let toggle = $state<HTMLButtonElement>();
  let drag: { id: number; startY: number; handle: HTMLButtonElement } | null = null;
  let dragging = $state(false); let dragOffset = $state(0);
  const s = $derived(player.state);
  const media = $derived(player.media);
  $effect(() => { if (s.volume > 0) lastVolume = s.volume; });
  function toggleMute() { void player.run({action:"volume",volume:s.volume ? 0 : lastVolume}); }
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
  $effect(() => {
    seekTrack;
    seekPreview = null;
    followPlayback = true;
  });
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
    if (!player.expanded) readerOpen = false;
    if (player.expanded) {
      untrack(() => onExpand?.());
      void tick().then(() => untrack(() => { if (!readerOpen) panel?.focus({preventScroll:true}); }));
    }
    return resetDrag;
  });
  function expand() { player.expanded = true; }
  function handlePanelKeydown(event: KeyboardEvent) {
    if (event.key !== "Escape") return;
    event.stopPropagation();
    if (settingsOpen) {
      settingsOpen = false;
      void tick().then(() => panel?.querySelector<HTMLButtonElement>('[aria-controls="audio-player-settings"]')?.focus({preventScroll:true}));
    } else collapse();
  }
  function openReader(event: MouseEvent) {
    readerOrigin = event.currentTarget as HTMLButtonElement;
    settingsOpen = false;
    followPlayback = true;
    readerOpen = true;
  }
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
    if (id === s.trackId && !s.paused) await player.run({action:"pause",paused:true});
    else {
      followPlayback = true;
      await player.run({action:"play",trackId:id});
    }
    if (player.state.trackId === id && player.state.subtitleId) tab = "script";
  }
</script>

{#snippet playbackControls()}
  <label class="seek"><span class="sr-only">{$t("Playback position")}</span><input type="range" min="0" max={duration || 1} step="0.1" style:--range-progress={`${duration ? Math.min(100,position/duration*100) : 0}%`} value={Math.min(position,duration || 1)} disabled={seekBlocked || !duration || s.loading}
    onpointerdown={() => { seekPreview = position; }} oninput={(e) => { seekPreview = Number(e.currentTarget.value); }}
    onchange={commitSeek} onpointerup={commitSeek} onkeyup={commitSeek} onblur={commitSeek}
    onpointercancel={() => { seekPreview = null; }} /></label>
  <div class="time"><span>{mediaTime(position)}</span><span>{s.loading ? $t("Loading…") : duration ? mediaTime(duration) : "—"}</span></div>
  <div class="transport">
    <button class="icon-button" disabled={blocked || !previous} aria-label={$t("Previous track")} title={$t("Previous track")} onclick={() => player.run({action:"next",direction:-1})}><Icon name="previous" /></button>
    <button class="jump-button" disabled={seekBlocked || !s.trackId || s.loading} aria-label={$t("Back 10 seconds")} title={$t("Back 10 seconds")} onclick={() => player.run({action:"seek",position:Math.max(0,position-10)})}>−10</button>
    <button class="primary-control" disabled={blocked || !media?.tracks.length} aria-label={s.paused ? $t("Play") : $t("Pause")} title={s.paused ? $t("Play") : $t("Pause")} onclick={playPause}><Icon name={s.paused ? "play" : "pause"} /></button>
    <button class="jump-button" disabled={seekBlocked || !duration || s.loading} aria-label={$t("Forward 10 seconds")} title={$t("Forward 10 seconds")} onclick={() => player.run({action:"seek",position:Math.min(duration,position+10)})}>+10</button>
    <button class="icon-button" disabled={blocked || !next} aria-label={$t("Next track")} title={$t("Next track")} onclick={() => player.run({action:"next",direction:1})}><Icon name="next" /></button>
  </div>
  {#if !readerOpen}
    <div class="playback-quick">
      <div class="quick-volume">
        <button class="icon-button" disabled={blocked} aria-label={s.volume ? $t("Mute") : $t("Unmute")} title={s.volume ? $t("Mute") : $t("Unmute")} onclick={toggleMute}><Icon name={s.volume ? "volume" : "muted"} /></button>
        <label><span class="sr-only">{$t("Volume")}</span><input type="range" min="0" max="100" step="1" value={s.volume} style:--range-progress={`${s.volume}%`} disabled={blocked} onchange={(event) => player.run({action:"volume",volume:Number(event.currentTarget.value)})} /></label>
        <span class="volume-value" aria-hidden="true">{s.volume}%</span>
      </div>
      <label class="quick-speed"><span class="sr-only">{$t("Playback speed")}</span><select aria-label={$t("Playback speed")} title={$t("Playback speed")} value={s.speed} disabled={blocked} onchange={(event) => player.run({action:"speed",speed:Number(event.currentTarget.value)})}>{#each speeds as speed}<option value={speed}>{speed}×</option>{/each}</select></label>
    </div>
  {/if}
{/snippet}

{#snippet playbackSettings()}
  <section id="audio-player-settings" class="player-settings" aria-label={$t("Player settings")}>
    {#if readerOpen}<div class="audio-options"><label>{$t("Volume")}<input type="range" min="0" max="100" step="1" value={s.volume} style:--range-progress={`${s.volume}%`} disabled={blocked} onchange={(e) => player.run({action:"volume",volume:Number(e.currentTarget.value)})} /></label>
    <label>{$t("Playback speed")}<select value={s.speed} disabled={blocked} onchange={(e) => player.run({action:"speed",speed:Number(e.currentTarget.value)})}>{#each speeds as speed}<option value={speed}>{speed}×</option>{/each}</select></label></div>{/if}
    <label>{$t("Subtitles")}<select value={s.subtitleId ?? ""} disabled={blocked || !s.trackId} onchange={(e) => player.run({action:"subtitle",subtitleId:e.currentTarget.value || null})}><option value="">{$t("Off")}</option>{#each media?.subtitles ?? [] as subtitle}<option value={subtitle.id}>{subtitle.name}</option>{/each}</select></label>
    <div class="subtitle-options"><label>{$t("Subtitle offset (seconds)")}<input type="number" min="-600" max="600" step="0.1" value={s.subtitleOffset} disabled={blocked} onchange={(e) => player.run({action:"offset",offset:Number(e.currentTarget.value)})} /></label>
    {#if !readerOpen}<label>{$t("Subtitle size")}<select bind:value={fontSize}>{#each [14,16,18,20,24,28,32] as size}<option value={size}>{size}px</option>{/each}</select></label>{/if}</div>
    <p class="player-hint">{$t("Positive subtitle offset displays captions later. Subtitle selection is saved for each track.")}</p>
    <div class="settings-actions"><button disabled={blocked} onclick={() => { if (media) void player.open({workId:media.workId,title:player.title,thumbnailUrl:player.cover}); }}><Icon name="refresh" />{$t("Rescan files")}</button><button disabled={blocked} onclick={() => player.run({action:"stop"})}><Icon name="stop" />{$t("Stop playback")}</button></div>
  </section>
{/snippet}

{#if media || player.expanded}
  <section class="player-dock" class:expanded={player.expanded} class:dragging style:transform={`translateY(${dragOffset}px)`} aria-label={$t("Audio player")}>
    {#if player.expanded}
      <div class="player-panel" id="audio-player-panel" role="dialog" aria-modal="false" aria-label={$t("Audio player")}
        bind:this={panel} tabindex="-1" onkeydown={handlePanelKeydown}>
        <header class="panel-header"><button class="drag-handle" aria-label={$t("Drag down to collapse")} title={$t("Drag down to collapse")}
          onpointerdown={startDrag} onpointermove={moveDrag} onpointerup={endDrag} onpointercancel={cancelDrag} onlostpointercapture={cancelDrag}
          onclick={activateHandle}><span class="handle" aria-hidden="true"></span></button><div><h2>{$t("Now playing")}</h2><div class="header-actions">
          <button class="icon-button" aria-label={$t("Reading mode")} title={$t("Reading mode")} disabled={!media} onclick={openReader}><Icon name="expand" /></button>
          <button title={$t("Player settings")} class:pressed={settingsOpen} class="icon-button" aria-label={$t("Player settings")} aria-expanded={settingsOpen} aria-controls="audio-player-settings" onclick={() => settingsOpen = !settingsOpen}><Icon name="settings" /></button>
          <button class="icon-button" aria-label={$t("Collapse player")} title={$t("Collapse player")} onclick={collapse}><Icon name="chevronDown" /></button>
        </div></div></header>
        <div class="panel-body">
          {#if player.loading}<p class="player-message" role="status">{$t("Scanning audio tracks…")}</p>{/if}
          {#if notice}<p class="player-error" role="alert">{$t(notice.replace(/\s*\[diagnostic:[^\]]+\]$/, ""))}</p>{/if}
          {#if media}
            <section class="now-playing" aria-label={$t("Playback controls")}>
              <div class="playing-identity">
                <div class="artwork">{#if player.cover}<img src={player.cover} alt="" />{:else}<Icon name="music" />{/if}</div>
                <div class="playing-titles"><h3 title={player.title}>{player.title}</h3><p class="track-name" title={track?.name}>{track?.name ?? $t("Choose a track")}</p><span class="track-status"><span class:playing={Boolean(s.trackId && !s.paused)} class="status-dot"></span>{s.loading ? $t("Loading…") : !s.trackId ? $t("Choose a track") : s.paused ? $t("Paused") : $t("Now playing")}{#if track}<span class="track-format">{track.format}</span>{/if}</span></div>
              </div>
              {@render playbackControls()}
            </section>
            <div class="player-tabs" role="group" aria-label={$t("Player view")}>
              <button aria-pressed={tab === "script" && !settingsOpen} onclick={() => { tab = "script"; settingsOpen = false; }}><Icon name="script" />{$t("Script")}</button>
              <button aria-pressed={tab === "tracks" && !settingsOpen} onclick={() => { tab = "tracks"; settingsOpen = false; }}><Icon name="list" />{$t("Tracks")}<span>{media.tracks.length}</span></button>
            </div>
            <div class="panel-content">
            {#if settingsOpen}
              {@render playbackSettings()}
            {:else if tab === "tracks"}
              <section class="track-section" aria-label={$t("Tracks")}>
                {#if groups.length > 1}<label class="group-picker">{$t("Folder / version")}<select bind:value={group}>{#each groups as folder}<option value={folder}>{folder || $t("Main folder")}</option>{/each}</select></label>{/if}
                {#each media.warnings ?? [] as warning}<p class="player-error" role="alert">{$t(warning)}</p>{/each}
                {#if !media?.tracks.length}<p class="player-message">{media.hasArchives ? $t("No playable audio was found in these archives.") : $t("No supported audio files were found in this work.")}</p>{/if}
                <ol class="track-list">{#each media.tracks.filter(t => t.group === group) as item, index (item.id)}
                  <li><button class:current={item.id === s.trackId} aria-current={item.id === s.trackId ? "true" : undefined} disabled={blocked} onclick={() => playTrack(item.id)}>
                    <span class="track-number">{String(index+1).padStart(2,"0")}</span><span class="track-info"><strong title={item.name}>{item.name}</strong><small>{item.format} · {formatBytes(item.size)}{#if s.durations[item.id] || (item.id === s.trackId && s.duration)} · {mediaTime(s.durations[item.id] || s.duration)}{/if}</small></span><span class="track-action"><Icon name={item.id === s.trackId && !s.paused ? "pause" : "play"} /></span>
                  </button></li>
                {/each}</ol>
              </section>
            {:else}
              <section class="script-section" aria-label={$t("Script")}>
                <div class="script-toolbar">
                  <label class="follow-toggle"><span>{$t("Auto-follow playback")}</span><input class="follow-switch" type="checkbox" role="switch" checked={followPlayback} onchange={(event) => setFollowPlayback(event.currentTarget.checked)} aria-describedby="subtitle-follow-hint" /></label>
                  <button class="reader-launch" bind:this={readerButton} onclick={openReader}><Icon name="expand" />{$t("Reading mode")}</button>
                  <p id="subtitle-follow-hint" class="sr-only">{$t("Turn off to browse the script freely.")}</p>
                </div>
                {#if player.subtitleError}<p class="player-error" role="alert">{$t(player.subtitleError)}</p>{/if}
                {#if player.subtitleLoading}<p class="player-message" role="status">{$t("Loading…")}</p>
                {:else if !s.subtitleId}<div class="script-empty"><Icon name="script" /><p>{$t("Select a VTT subtitle file to display captions.")}</p><button onclick={() => settingsOpen = true}>{$t("Player settings")}</button></div>
                {:else if !player.cues.length}<p class="player-message">{$t("No subtitle at this position.")}</p>
                {:else}
                  <ScriptView cues={player.cues} {position} offset={s.subtitleOffset} {fontSize} autoFollow={followPlayback} onBrowse={() => setFollowPlayback(false)} disabled={seekBlocked || !s.trackId} onSeek={(position) => player.run({action:"seek",position})} />{/if}
              </section>
            {/if}
            </div>
          {/if}
        </div>
      </div>
    {:else}
      <div class="mini-player">
        <button class="mini-identity" onclick={expand} aria-label={$t("Expand player")} aria-expanded="false" aria-controls="audio-player-panel" bind:this={toggle}>
          <span class="mini-cover">{#if player.cover}<img src={player.cover} alt="" />{:else}<Icon name="music" />{/if}</span>
          <span class="mini-titles"><strong>{track?.name ?? (player.title || $t("Audio player"))}</strong><small>{track ? player.title : $t("Choose a track")}</small></span>
        </button>
        <button class="primary-control" disabled={blocked || !media?.tracks.length} aria-label={s.paused ? $t("Play") : $t("Pause")} title={s.paused ? $t("Play") : $t("Pause")} onclick={playPause}><Icon name={s.paused ? "play" : "pause"} /></button>
        <button class="icon-button" aria-label={$t("Expand player")} aria-expanded="false" aria-controls="audio-player-panel" onclick={expand}><Icon name="chevronUp" /></button>
      </div>
      <div class="mini-progress" role="progressbar" aria-label={$t("Playback position")} aria-valuemin="0" aria-valuemax={duration || 1} aria-valuenow={Math.min(position,duration || 1)}><span style:width={`${duration ? Math.min(100,position/duration*100) : 0}%`}></span></div>
      {#if notice}<p class="mini-error" role="status">{$t(notice.replace(/\s*\[diagnostic:[^\]]+\]$/, ""))}</p>{/if}
    {/if}
  </section>
{/if}

{#if readerOpen && player.expanded && media}
  <ScriptReader cues={player.cues} {position} offset={s.subtitleOffset}
    title={player.title} tracks={media.tracks} trackId={s.trackId}
    subtitleKey={`${s.workId ?? ""}\0${s.trackId ?? ""}\0${s.subtitleId ?? ""}`}
    loading={player.subtitleLoading} error={player.subtitleError || notice}
    hasSubtitle={Boolean(s.subtitleId)} disabled={seekBlocked || !s.trackId}
    tracksDisabled={blocked} follow={followPlayback} onFollowChange={setFollowPlayback}
    onTrack={(trackId) => player.run({action:"play",trackId})}
    onSeek={(position) => player.run({action:"seek",position})}
    onClose={closeReader} controls={playbackControls} settings={playbackSettings} />
{/if}

<style>
  .player-dock { position:fixed; z-index:35; bottom:16px; right:20px; width:min(400px,calc(100vw - 236px)); box-sizing:border-box; border:1px solid var(--dldr-color-ui-line); border-radius:20px; background:var(--dldr-color-ui-surface); color:var(--dldr-color-ui-ink); box-shadow:0 12px 48px var(--dldr-color-ui-shadow-strong); overflow:hidden; }
  .player-dock.expanded { width:min(440px,calc(100vw - 236px)); transition:transform .18s ease-out; }
  .player-dock.dragging { transition:none; }
  button,select,input { font:inherit; }
  button { color:inherit; border:1px solid var(--dldr-color-ui-line-subtle); border-radius:10px; background:var(--dldr-color-ui-surface); padding:8px 10px; cursor:pointer; transition:background .15s,color .15s; }
  button:hover:not(:disabled) { background:var(--dldr-color-ui-hover); }
  button:focus-visible,input:focus-visible,select:focus-visible { outline:3px solid var(--dldr-color-ui-accent); outline-offset:2px; }
  button:disabled { opacity:.4; cursor:default; }
  button.icon-button { display:grid; place-items:center; padding:8px; width:40px; height:40px; border:0; border-radius:50%; background:transparent; flex:none; }
  button.icon-button:hover:not(:disabled) { background:var(--dldr-color-ui-hover); }
  button.primary-control { display:grid; place-items:center; width:56px; height:56px; padding:0; border:0; border-radius:50%; background:var(--dldr-color-ui-accent); color:var(--dldr-color-ui-on-accent); flex:none; box-shadow:0 3px 12px var(--dldr-color-ui-shadow); }
  button.primary-control:hover:not(:disabled) { background:var(--dldr-color-ui-accent-hover); }
  .mini-player { display:flex; align-items:center; gap:8px; padding:12px; }
  .mini-identity { display:flex; align-items:center; gap:12px; flex:1; min-width:0; border:0; padding:0; text-align:left; background:transparent; }
  .mini-cover { display:grid; place-items:center; width:48px; height:48px; border-radius:12px; overflow:hidden; flex:none; background:var(--dldr-color-ui-surface-muted); }
  .mini-cover img { width:100%; height:100%; object-fit:cover; }
  .mini-titles { min-width:0; }
  .mini-titles strong,.mini-titles small { display:block; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
  .mini-titles strong { font-size:14px; font-weight:700; }
  .mini-titles small { font-size:12px; color:var(--dldr-color-ui-dim); margin-top:4px; }
  .mini-player .primary-control { width:40px; height:40px; }
  .mini-progress { height:4px; background:var(--dldr-color-ui-progress-track); }
  .mini-progress span { display:block; height:100%; background:var(--dldr-color-ui-accent); transition:width .2s linear; }
  .mini-error { margin:0; padding:8px 12px; font-size:12px; color:var(--dldr-color-ui-error); overflow-wrap:anywhere; }
  .player-panel { display:flex; flex-direction:column; height:min(780px,calc(100dvh - 32px)); outline:none; }
  .player-panel .icon-button { width:40px; height:40px; }
  .player-panel :global(svg) { width:22px; height:22px; flex:none; }
  .header-actions :global(svg) { width:24px; height:24px; }
  .panel-header { flex:none; padding:0 18px 6px; }
  button.drag-handle { display:grid; place-items:center; width:100%; height:24px; padding:0; border:0; background:transparent; touch-action:none; user-select:none; cursor:grab; }
  .dragging .drag-handle { cursor:grabbing; }
  .handle { display:block; width:36px; height:4px; border-radius:4px; background:var(--dldr-color-ui-line); opacity:.65; }
  .panel-header > div,.header-actions { display:flex; align-items:center; justify-content:space-between; }
  .panel-header h2 { margin:0; font-size:16px; font-weight:700; color:var(--dldr-color-ui-dim); }
  .header-actions { gap:2px; }
  button.pressed { color:var(--dldr-color-ui-accent); background:var(--dldr-color-ui-selected); }
  .panel-body { min-height:0; flex:1; display:flex; flex-direction:column; padding:0 18px 16px; }
  .now-playing { flex:none; padding-bottom:8px; }
  .playing-identity { display:flex; align-items:center; gap:14px; margin:4px 0 12px; }
  .artwork { width:76px; height:76px; display:grid; place-items:center; border-radius:14px; background:var(--dldr-color-ui-surface-muted); box-shadow:0 3px 12px var(--dldr-color-ui-shadow); overflow:hidden; flex:none; }
  .artwork img { width:100%; height:100%; object-fit:cover; }
  .artwork :global(svg) { width:30px; height:30px; color:var(--dldr-color-ui-dim); }
  .playing-titles { min-width:0; flex:1; }
  .now-playing h3 { margin:0 0 3px; font-size:13px; line-height:1.5; font-weight:500; color:var(--dldr-color-ui-dim); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
  .track-name { margin:0; font-size:16px; line-height:1.5; font-weight:700; overflow-wrap:anywhere; display:-webkit-box; -webkit-box-orient:vertical; -webkit-line-clamp:2; line-clamp:2; overflow:hidden; }
  .track-status { display:flex; align-items:center; gap:6px; margin-top:6px; font-size:12px; color:var(--dldr-color-ui-dim); }
  .status-dot { width:6px; height:6px; border-radius:50%; background:var(--dldr-color-ui-line); }
  .status-dot.playing { background:var(--dldr-color-ui-accent); box-shadow:0 0 0 3px var(--dldr-color-ui-selected); }
  .track-format { border-left:1px solid var(--dldr-color-ui-line-subtle); padding-left:6px; font-size:11px; letter-spacing:.03em; }
  input[type=range] { appearance:none; -webkit-appearance:none; box-sizing:border-box; width:100%; height:28px; padding:0; border:0; background:transparent; color:var(--dldr-color-ui-accent); cursor:pointer; margin:0; border-radius:8px; }
  input[type=range]::-webkit-slider-runnable-track { height:5px; border-radius:99px; background:linear-gradient(to right,var(--dldr-color-ui-accent) 0 var(--range-progress,0%),var(--dldr-color-ui-progress-track) var(--range-progress,0%) 100%); }
  input[type=range]::-webkit-slider-thumb { appearance:none; -webkit-appearance:none; width:15px; height:15px; margin-top:-5px; border:2px solid var(--dldr-color-ui-surface); border-radius:50%; background:var(--dldr-color-ui-accent); box-shadow:0 0 0 1px var(--dldr-color-ui-accent); }
  input[type=range]::-moz-range-track { height:5px; border-radius:99px; background:var(--dldr-color-ui-progress-track); }
  input[type=range]::-moz-range-progress { height:5px; border-radius:99px; background:var(--dldr-color-ui-accent); }
  input[type=range]::-moz-range-thumb { width:12px; height:12px; border:2px solid var(--dldr-color-ui-surface); border-radius:50%; background:var(--dldr-color-ui-accent); }
  input[type=range]:disabled { opacity:.45; cursor:default; }
  .seek { display:block; }
  .time { display:flex; justify-content:space-between; margin:0 0 10px; font-size:13px; font-variant-numeric:tabular-nums; color:var(--dldr-color-ui-dim); }
  .transport { display:flex; align-items:center; justify-content:center; gap:12px; margin:0 0 8px; }
  .transport .primary-control :global(svg) { width:26px; height:26px; }
  .jump-button { display:grid; place-items:center; border:0; border-radius:50%; background:var(--dldr-color-ui-surface-muted); font-size:14px; font-weight:650; padding:0; width:42px; height:42px; }
  .playback-quick { display:flex; align-items:center; justify-content:space-between; gap:12px; }
  .quick-volume { display:flex; align-items:center; gap:4px; min-width:0; }
  .quick-volume .icon-button { width:32px; height:32px; padding:6px; }
  .quick-volume label { width:80px; }
  .quick-volume :global(svg) { width:18px; height:18px; }
  .volume-value { font-size:12px; font-variant-numeric:tabular-nums; min-width:34px; color:var(--dldr-color-ui-dim); }
  .quick-speed select { width:auto; min-width:72px; padding:5px 8px; border-radius:99px; font-size:13px; font-weight:600; background:var(--dldr-color-ui-surface-muted); border-color:transparent; }
  .player-tabs { flex:none; display:grid; grid-template-columns:1fr 1fr; gap:4px; padding:4px; margin:0 0 12px; border-radius:12px; background:var(--dldr-color-ui-surface-muted); }
  .player-tabs button { display:flex; align-items:center; justify-content:center; gap:7px; border:0; background:transparent; font-size:14px; font-weight:650; padding:9px; }
  .player-tabs button[aria-pressed=true] { background:var(--dldr-color-ui-surface); color:var(--dldr-color-ui-accent); box-shadow:0 1px 5px var(--dldr-color-ui-shadow); }
  .player-tabs span { display:grid; place-items:center; min-width:20px; height:20px; border-radius:7px; padding:0 5px; font-size:12px; font-weight:600; color:var(--dldr-color-ui-dim); background:var(--dldr-color-ui-surface-muted); }
  .panel-content { flex:1; min-height:0; overflow:auto; overscroll-behavior:contain; scrollbar-width:thin; }
  .script-section { height:100%; min-height:0; display:flex; flex-direction:column; }
  .script-section :global(.script-viewport) { height:auto; flex:1; min-height:120px; }
  label { display:flex; flex-direction:column; gap:6px; min-width:0; font-size:14px; font-weight:600; }
  select,input[type=number] { box-sizing:border-box; min-width:0; width:100%; border:1px solid var(--dldr-color-ui-line); border-radius:8px; background:var(--dldr-color-ui-surface); color:var(--dldr-color-ui-ink); padding:9px; font-size:15px; }
  .player-settings { display:grid; gap:16px; padding:16px; margin:0; border:1px solid var(--dldr-color-ui-line-subtle); border-radius:12px; background:var(--dldr-color-ui-surface-muted); }
  .audio-options,.subtitle-options { display:grid; grid-template-columns:minmax(0,1fr) 100px; gap:12px; align-items:center; }
  .settings-actions { display:flex; flex-wrap:wrap; gap:8px; padding-top:2px; }
  .settings-actions button { display:flex; gap:5px; align-items:center; font-size:13px; }
  .player-hint { margin:0; font-size:13px; line-height:1.6; color:var(--dldr-color-ui-dim); }
  .reader-launch { display:flex; align-items:center; justify-content:center; gap:6px; font-size:13px; font-weight:600; padding:7px 10px; white-space:nowrap; background:var(--dldr-color-ui-surface-muted); }
  .script-toolbar { flex:none; display:flex; justify-content:space-between; align-items:center; gap:12px; margin-bottom:10px; }
  label.follow-toggle { flex-direction:row; align-items:center; gap:8px; font-size:13px; cursor:pointer; }
  input.follow-switch { appearance:none; flex:none; width:34px; height:20px; margin:0; padding:2px; border:1px solid var(--dldr-color-ui-line); border-radius:999px; background:var(--dldr-color-ui-line-subtle); cursor:pointer; }
  input.follow-switch::before { content:""; display:block; width:14px; height:14px; border-radius:50%; background:var(--dldr-color-ui-surface); box-shadow:0 1px 3px var(--dldr-color-ui-shadow); transition:transform .15s; }
  input.follow-switch:checked { background:var(--dldr-color-ui-accent); border-color:var(--dldr-color-ui-accent); }
  input.follow-switch:checked::before { transform:translateX(14px); background:var(--dldr-color-ui-on-accent); }
  .track-section { min-height:100%; }
  .group-picker { margin-bottom:12px; }
  .track-list { list-style:none; margin:0; padding:0; }
  .track-list li + li { margin-top:6px; }
  .track-list button { width:100%; display:grid; grid-template-columns:24px minmax(0,1fr) 28px; gap:10px; align-items:center; text-align:left; padding:12px 10px; border-color:transparent; }
  .track-list button.current { border-color:var(--dldr-color-ui-line-subtle); background:var(--dldr-color-ui-selected); }
  .track-number { font-size:12px; font-variant-numeric:tabular-nums; text-align:center; color:var(--dldr-color-ui-dim); }
  .track-info { min-width:0; }
  .track-action { display:grid; place-items:center; color:var(--dldr-color-ui-accent); }
  .track-list strong { display:-webkit-box; -webkit-box-orient:vertical; -webkit-line-clamp:2; line-clamp:2; overflow:hidden; font-size:14px; font-weight:650; overflow-wrap:anywhere; line-height:1.6; }
  .track-list small { display:block; font-size:12px; color:var(--dldr-color-ui-dim); margin-top:4px; }
  .script-empty,.player-message { padding:24px 12px; font-size:14px; text-align:center; color:var(--dldr-color-ui-dim); line-height:1.7; }
  .script-empty { flex:1; display:flex; flex-direction:column; align-items:center; justify-content:center; }
  .script-empty :global(svg) { width:28px; height:28px; }
  .script-empty button { font-size:14px; }
  .player-error { margin:0 0 10px; padding:10px; border:1px solid var(--dldr-color-ui-error-border); border-radius:8px; color:var(--dldr-color-ui-error); background:var(--dldr-color-ui-error-bg); font-size:14px; overflow-wrap:anywhere; }
  .sr-only { position:absolute; width:1px; height:1px; padding:0; margin:-1px; overflow:hidden; clip:rect(0,0,0,0); border:0; }
  @media(max-width:760px) { .player-dock,.player-dock.expanded { width:min(440px,calc(100vw - 32px)); right:16px; } }
  @media(max-width:620px) { .player-dock,.player-dock.expanded { bottom:12px; right:12px; width:calc(100vw - 24px); max-width:440px; } .player-panel { height:min(780px,calc(100dvh - 24px)); } .panel-body { padding-inline:14px; } .panel-header { padding-inline:14px; } .script-toolbar { gap:8px; } }
  @media(max-height:650px) { .artwork { width:54px; height:54px; border-radius:10px; } .playing-identity { margin:0 0 4px; gap:10px; } .track-name { -webkit-line-clamp:1; line-clamp:1; font-size:14px; } .track-status { margin-top:2px; } .transport .primary-control { width:48px; height:48px; } .transport { margin-bottom:4px; } .time { margin-bottom:4px; } .panel-header { padding-bottom:2px; } button.drag-handle { height:18px; } .player-tabs { margin-bottom:8px; } }
  @media(prefers-reduced-motion:reduce) { button,.mini-progress span,.player-dock.expanded,input.follow-switch::before { transition:none; } }
</style>
