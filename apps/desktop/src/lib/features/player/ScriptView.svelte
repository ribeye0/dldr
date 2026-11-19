<script lang="ts">
  import { tick } from "svelte";
  import { t } from "$lib/i18n";
  import type { Cue } from "$lib/api/player";
  import { activeCues, mediaTime } from "./subtitles";
  let { cues, position, offset, fontSize = 16, autoFollow = true, disabled = false, onSeek }: {
    cues: Cue[]; position: number; offset: number; fontSize?: number;
    autoFollow?: boolean; disabled?: boolean; onSeek: (position: number) => void;
  } = $props();
  let viewport = $state<HTMLDivElement>();
  let height = $state(0);
  const active = $derived(activeCues(cues, position, offset));
  // A stable key prevents every clock tick from interrupting manual scrolling.
  const activeKey = $derived(active.map(cue => cues.indexOf(cue)).join(":"));
  $effect(() => {
    if (!viewport) return;
    const observer = new ResizeObserver(entries => { height = entries[0]?.contentRect.height ?? 0; });
    observer.observe(viewport);
    return () => observer.disconnect();
  });
  $effect(() => {
    if (!autoFollow) {
      // Stop an in-flight smooth scroll as soon as free reading is enabled.
      viewport?.scrollTo({top: viewport.scrollTop, behavior:"instant"});
      return;
    }
    const key = activeKey;
    const size = fontSize;
    const rows = cues;
    const measured = height;
    let cancelled = false;
    if (key && measured > 0 && size && rows.length) {
      void tick().then(() => {
        if (cancelled || !viewport) return;
        const current = viewport.querySelectorAll<HTMLElement>('[aria-current="true"]');
        if (!current.length) return;
        const first = current[0].getBoundingClientRect();
        const last = current[current.length-1].getBoundingClientRect();
        const bounds = viewport.getBoundingClientRect();
        const top = viewport.scrollTop + (first.top + last.bottom)/2 - (bounds.top + viewport.clientTop + viewport.clientHeight/2);
        viewport.scrollTo({top, behavior:matchMedia("(prefers-reduced-motion: reduce)").matches ? "instant" : "smooth"});
      });
    }
    return () => { cancelled = true; };
  });
</script>

<div class="script-viewport" bind:this={viewport} style:--script-height={`${height}px`} style:font-size={`${fontSize}px`} aria-label={$t("Subtitle text")} role="region" tabindex="-1">
  <ol class="script-lines">
    {#each cues as cue, i (i)}
      <li><button class:active={active.includes(cue)} aria-current={active.includes(cue) ? "true" : undefined}
        disabled={disabled} title={mediaTime(cue.start)} onclick={() => onSeek(Math.max(0,cue.start+offset))}>
        <span class="cue-time">{mediaTime(cue.start)}</span><span class="cue-text">{cue.text}</span>
      </button></li>
    {/each}
  </ol>
</div>

<style>
  .script-viewport { box-sizing:border-box; height:clamp(180px,32dvh,320px); overflow-y:auto; overflow-x:hidden; overscroll-behavior:contain; scrollbar-width:thin; position:relative; border-radius:14px; background:var(--dldr-color-ui-surface-muted); scroll-behavior:smooth; }
  .script-lines { list-style:none; margin:0; padding:calc(var(--script-height) / 2) 12px; display:flex; flex-direction:column; gap:4px; }
  .script-lines li { min-width:0; }
  button { display:flex; width:100%; flex-direction:column; gap:3px; box-sizing:border-box; padding:8px 12px; border:1px solid transparent; border-radius:12px; font:inherit; line-height:1.7; color:var(--dldr-color-ui-dim); background:transparent; text-align:center; cursor:pointer; transition:color .18s,background .18s; }
  .cue-text { white-space:pre-wrap; overflow-wrap:anywhere; }
  .cue-time { font-size:14px; font-weight:600; font-variant-numeric:tabular-nums; opacity:.8; }
  button.active { color:var(--dldr-color-ui-selected-text); background:var(--dldr-color-ui-selected); border-color:var(--dldr-color-ui-selected-border); font-weight:750; }
  button.active .cue-time { color:var(--dldr-color-ui-accent); opacity:1; }
  button:hover:not(:disabled) { background:var(--dldr-color-ui-status-bg); }
  button:focus-visible,.script-viewport:focus-visible { outline:3px solid var(--dldr-color-ui-accent); outline-offset:-3px; }
  button:disabled { cursor:default; }
  @media(prefers-reduced-motion:reduce) { .script-viewport { scroll-behavior:auto; } button { transition:none; } }
</style>
