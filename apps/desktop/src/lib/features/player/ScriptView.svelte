<script lang="ts">
  import { tick } from "svelte";
  import { t } from "$lib/i18n";
  import type { Cue } from "$lib/api/player";
  import { activeCues, mediaTime } from "./subtitles";
  let { cues, position, offset, fontSize = 16, autoFollow = true, disabled = false, onSeek,
    reading = false, lineHeight = 1.8, showTimes = true, query = "", focusIndex = -1, focusRequest = 0, onBrowse }: {
    cues: Cue[]; position: number; offset: number; fontSize?: number;
    autoFollow?: boolean; disabled?: boolean; onSeek: (position: number) => void;
    reading?: boolean; lineHeight?: number; showTimes?: boolean; query?: string;
    focusIndex?: number; focusRequest?: number; onBrowse?: () => void;
  } = $props();
  let viewport = $state<HTMLDivElement>();
  let height = $state(0);
  // Keep one gesture for the entire transcript, including movement outside it.
  // Native click alone can still fire after dragging within the same element.
  let gesture: {id:number; target:HTMLButtonElement; x:number; y:number; released:boolean} | null = null;
  function beginClick(event: PointerEvent & {currentTarget:HTMLButtonElement}) {
    gesture = !disabled && event.isPrimary && event.button === 0
      ? {id:event.pointerId,target:event.currentTarget,x:event.clientX,y:event.clientY,released:false}
      : null;
  }
  function moveClick(event: PointerEvent) {
    if (gesture?.id === event.pointerId && Math.hypot(event.clientX-gesture.x,event.clientY-gesture.y) > 6) gesture = null;
  }
  function releaseClick(event: PointerEvent) {
    moveClick(event);
    if (gesture?.id !== event.pointerId) return;
    const hit = document.elementFromPoint(event.clientX,event.clientY);
    if (event.button !== 0 || !hit || !gesture.target.contains(hit) || window.getSelection()?.isCollapsed === false) gesture = null;
    else gesture.released = true;
  }
  function leaveClick(event: PointerEvent) { if (gesture?.id === event.pointerId && !gesture.released) gesture = null; }
  function activateCue(event: MouseEvent & {currentTarget:HTMLButtonElement}, cue: Cue) {
    // detail=0 is native keyboard/assistive activation, without a pointer gesture.
    const clicked = event.detail === 0 || Boolean(gesture?.released && gesture.target === event.currentTarget);
    gesture = null;
    if (!disabled && clicked && window.getSelection()?.isCollapsed !== false) onSeek(Math.max(0,cue.start+offset));
  }
  const active = $derived(activeCues(cues, position, offset));
  const activeKey = $derived(active.map(cue => cues.indexOf(cue)).join(":"));
  const followKey = $derived(activeKey || String(Math.max(0,cues.findLastIndex(cue => cue.start+offset <= position))));
  const needle = $derived(query.trim().toLocaleLowerCase());
  function parts(text: string) {
    if (!needle) return [{text, match:false}];
    // Slice the original string; subtitle contents are always rendered as text.
    const pieces: {text:string; match:boolean}[] = [];
    const lower = text.toLocaleLowerCase();
    let start = 0, index = lower.indexOf(needle);
    while (index >= 0) {
      pieces.push({text:text.slice(start,index),match:false}, {text:text.slice(index,index+needle.length),match:true});
      start = index+needle.length; index = lower.indexOf(needle,start);
    }
    pieces.push({text:text.slice(start),match:false});
    return pieces;
  }
  function centerRows(rows: HTMLElement[]) {
    if (!viewport || !rows.length) return;
    const first = rows[0].getBoundingClientRect(), last = rows.at(-1)!.getBoundingClientRect();
    const bounds = viewport.getBoundingClientRect();
    const top = viewport.scrollTop + (first.top+last.bottom)/2 - (bounds.top+viewport.clientTop+viewport.clientHeight/2);
    viewport.scrollTo({top, behavior:matchMedia("(prefers-reduced-motion: reduce)").matches ? "instant" : "smooth"});
  }
  function browse() { gesture = null; pauseFollow(); }
  function pauseFollow() {
    // Cancel an in-flight follow animation before the user's scroll is applied.
    viewport?.scrollTo({top:viewport.scrollTop,behavior:"instant"});
    onBrowse?.();
  }
  $effect(() => {
    if (!viewport) return;
    const observer = new ResizeObserver(entries => { height = entries[0]?.contentRect.height ?? 0; });
    observer.observe(viewport);
    return () => observer.disconnect();
  });
  $effect(() => {
    if (!autoFollow) {
      viewport?.scrollTo({top:viewport.scrollTop,behavior:"instant"});
      return;
    }
    const key = followKey, size = fontSize, rows = cues, measured = height, spacing = lineHeight, timestamps = showTimes;
    let cancelled = false;
    if (key && measured > 0 && size && rows.length && spacing && typeof timestamps === "boolean") {
      void tick().then(() => {
        if (!cancelled && viewport) {
          const current = [...viewport.querySelectorAll<HTMLElement>('li[aria-current="true"]')];
          const nearest = viewport.querySelector<HTMLElement>(`li[data-cue="${key}"]`);
          centerRows(current.length ? current : nearest ? [nearest] : []);
        }
      });
    }
    return () => { cancelled = true; };
  });
  $effect(() => {
    const index = focusIndex;
    focusRequest;
    let cancelled = false;
    if (index >= 0) void tick().then(() => {
      const row = viewport?.querySelector<HTMLElement>(`li[data-cue="${index}"]`);
      if (!cancelled && row) centerRows([row]);
    });
    return () => { cancelled = true; };
  });
</script>

<svelte:window onpointermove={moveClick} onpointerup={releaseClick} onpointercancel={() => gesture = null} onblur={() => gesture = null} />

<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions (This scrollable region needs keyboard focus and native scroll keys.) -->
<div class="script-viewport" class:reading bind:this={viewport} style:--script-height={`${height}px`} style:font-size={`${fontSize}px`} style:--script-line-height={lineHeight}
  aria-label={$t("Subtitle text")} role="region" tabindex="0"
  onwheel={browse} ontouchstart={pauseFollow} onscroll={() => gesture = null}
  onpointerdown={(event) => { if (event.target === viewport) browse(); }}
  onkeydown={(event) => { if (["ArrowUp","ArrowDown","PageUp","PageDown","Home","End"," "].includes(event.key)) browse(); }}>
  <ol class="script-lines">
    {#each cues as cue, i (i)}
      <li data-cue={i} class:active={active.includes(cue)} class:found={i === focusIndex} aria-current={active.includes(cue) ? "true" : undefined}>
        {#if showTimes}<button class="cue-time" disabled={disabled} aria-label={$t("Play from {0}",mediaTime(cue.start))}
          onpointerdown={beginClick} onpointerleave={leaveClick} onclick={(event) => activateCue(event,cue)}>{mediaTime(cue.start)}</button>{/if}
        <button type="button" class="cue-text" disabled={disabled} title={$t("Play from {0}",mediaTime(cue.start))} onpointerdown={beginClick} onpointerleave={leaveClick} onclick={(event) => activateCue(event,cue)}>{#each parts(cue.text) as part}{#if part.match}<mark>{part.text}</mark>{:else}{part.text}{/if}{/each}</button>
      </li>
    {/each}
  </ol>
</div>

<style>
  .script-viewport { box-sizing:border-box; height:clamp(180px,32dvh,320px); overflow-y:auto; overflow-x:hidden; overscroll-behavior:contain; scrollbar-width:thin; position:relative; border-radius:14px; background:var(--dldr-color-ui-surface-muted); }
  .script-lines { list-style:none; margin:0; padding:calc(var(--script-height) / 2) 12px; display:flex; flex-direction:column; gap:4px; }
  li { min-width:0; display:flex; flex-direction:column; align-items:center; gap:3px; padding:8px 12px; border:1px solid transparent; border-radius:12px; line-height:var(--script-line-height); color:var(--dldr-color-ui-dim); text-align:center; transition:color .18s,background .18s; }
  .cue-text { display:block; width:100%; padding:0; border:0; border-radius:4px; background:transparent; color:inherit; font:inherit; text-align:inherit; white-space:pre-wrap; overflow-wrap:anywhere; -webkit-user-select:text; user-select:text; cursor:pointer; }
  .cue-time { padding:2px 5px; border:0; border-radius:4px; background:transparent; color:inherit; font:inherit; font-size:14px; font-weight:600; font-variant-numeric:tabular-nums; opacity:.8; cursor:pointer; }
  .script-viewport:not(.reading) li { gap:1px; }
  .script-viewport:not(.reading) .cue-time { line-height:1.2; padding-block:0; }
  .cue-time:hover:not(:disabled) { background:var(--dldr-color-ui-status-bg); text-decoration:underline; }
  li.active { color:var(--dldr-color-ui-selected-text); background:var(--dldr-color-ui-selected); border-color:var(--dldr-color-ui-selected-border); font-weight:750; }
  li.active .cue-time { color:var(--dldr-color-ui-accent); opacity:1; }
  li.found { outline:2px solid var(--dldr-color-ui-accent); outline-offset:-2px; }
  mark { background:var(--dldr-color-ui-selected); color:var(--dldr-color-ui-ink); text-decoration:underline; text-underline-offset:3px; }
  button:focus-visible,.script-viewport:focus-visible { outline:3px solid var(--dldr-color-ui-accent); outline-offset:-3px; }
  button:disabled { cursor:default; }
  .reading { height:100%; min-height:0; border-radius:0; background:transparent; }
  .reading .script-lines { width:min(100%,860px); box-sizing:border-box; margin-inline:auto; padding:calc(var(--script-height) * .4) 32px; gap:8px; }
  .reading li { position:relative; text-align:left; align-items:stretch; padding:10px 16px; border:0; border-radius:8px; color:var(--dldr-color-ui-ink); font-weight:400; }
  .reading li.active { font-weight:600; box-shadow:inset 3px 0 var(--dldr-color-ui-accent); }
  .reading .cue-time { align-self:flex-start; font-size:13px; line-height:1.4; color:var(--dldr-color-ui-dim); padding:3px 0; font-weight:500; }
  @media(max-width:620px) { .reading .script-lines { padding-inline:12px; } .reading li { padding-inline:12px; } }
  @media(prefers-reduced-motion:reduce) { li { transition:none; } }
</style>
