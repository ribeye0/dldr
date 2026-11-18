<script lang="ts">
  import { onMount, tick } from "svelte";
  import { t } from "$lib/i18n";
  import type { Product, Playlist } from "$lib/model/types";
  import Icon from "$lib/components/ui/Icon.svelte";
  let { work, point, lists, busy, onToggle, onClose, onManage }: {
    work: Product; point:{x:number;y:number;trigger:HTMLButtonElement}; lists:Playlist[]; busy:boolean;
    onToggle:(id:string)=>void; onClose:()=>void; onManage:()=>void;
  } = $props();
  let menu: HTMLDivElement;
  let left = $state(0); let top = $state(0);
  onMount(() => {
    const trigger = point.trigger;
    const close = onClose;
    const rect = menu.getBoundingClientRect();
    left = Math.max(12, Math.min(point.x, innerWidth - rect.width - 12));
    top = Math.max(12, Math.min(point.y, innerHeight - rect.height - 12));
    menu.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus();
    window.addEventListener("resize",close); window.addEventListener("blur",close);
    return () => {
      window.removeEventListener("resize",close); window.removeEventListener("blur",close);
      void tick().then(() => { if (trigger.isConnected) trigger.focus({preventScroll:true}); });
    };
  });
  function keydown(event: KeyboardEvent) {
    if (event.key === "Escape" || event.key === "Tab") { event.preventDefault(); event.stopPropagation(); onClose(); return; }
    if (!["ArrowDown","ArrowUp","Home","End"].includes(event.key)) return;
    event.preventDefault();
    const items = [...menu.querySelectorAll<HTMLButtonElement>('button:not(:disabled)')];
    const index = items.indexOf(document.activeElement as HTMLButtonElement);
    items[event.key === "Home" ? 0 : event.key === "End" ? items.length-1 : (index + (event.key === "ArrowDown" ? 1 : -1) + items.length) % items.length]?.focus();
  }
</script>

<button class="menu-backdrop" aria-label={$t("Close playlist menu")} onclick={onClose} oncontextmenu={(e) => {e.preventDefault();onClose();}}></button>
<div class="playlist-menu" role="menu" aria-label={$t("Playlist membership")} tabindex="-1" bind:this={menu} style:left={`${left}px`} style:top={`${top}px`} onkeydown={keydown}>
  <p>{$t("Playlist membership")}</p>
  {#each lists as list (list.id)}
    <button role="menuitemcheckbox" aria-checked={list.workIds.includes(work.workId)} disabled={busy} onclick={() => onToggle(list.id)}>
      <span>{list.name || $t("Playlist {0}",0)}</span><Icon name={list.workIds.includes(work.workId) ? "check" : "plus"} />
    </button>
  {/each}
  <button role="menuitem" class="manage" onclick={onManage}><Icon name="playlists" />{$t("Manage playlists")}</button>
</div>

<style>
  .menu-backdrop { position:fixed; inset:0; z-index:60; border:0; background:transparent; }
  .playlist-menu { position:fixed; z-index:61; width:min(280px,calc(100vw - 24px)); max-height:min(420px,calc(100dvh - 24px)); box-sizing:border-box; overflow:auto; padding:8px; border:1px solid var(--dldr-color-ui-line); border-radius:12px; background:var(--dldr-color-ui-surface); color:var(--dldr-color-ui-ink); box-shadow:0 8px 28px var(--dldr-color-ui-shadow); }
  p { margin:4px 8px 8px; font-size:12px; color:var(--dldr-color-ui-dim); }
  .playlist-menu button { display:flex; align-items:center; justify-content:space-between; gap:10px; width:100%; min-height:38px; padding:8px; border:0; border-radius:6px; background:transparent; color:inherit; font:inherit; text-align:left; cursor:pointer; }
  span { overflow-wrap:anywhere; min-width:0; }
  button:hover:not(:disabled),button:focus-visible { background:var(--dldr-color-ui-selected); outline:2px solid var(--dldr-color-ui-focus); outline-offset:-2px; }
  button:disabled { color:var(--dldr-color-ui-dim); cursor:default; }
  .playlist-menu .manage { justify-content:flex-start; border-top:1px solid var(--dldr-color-ui-line-subtle); border-radius:0; margin-top:6px; }
</style>
