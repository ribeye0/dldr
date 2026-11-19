<script lang="ts">
  import { onMount } from "svelte";
  import { locale, type UiLocale } from "$lib/i18n";
  import { get } from "svelte/store";
  import { PlayerController, emptyPlayerState } from "$lib/controllers/player-controller.svelte";
  import PlayerView from "$lib/features/player/PlayerView.svelte";
  import type { PlayerState } from "$lib/api/player";
  let { language = "zh-TW" }: {language?: UiLocale} = $props();
  const tracks = [
    {id:"01.wav",name:"01. 午後的書房 — 午後の書斎",group:"WAV",format:"WAV",size:24000000},
    {id:"02.wav",name:"02. 雨聲 — 雨の音",group:"WAV",format:"WAV",size:24000000},
  ];
  const paragraphs = [
    "窗外開始下雨了。你想先聽一會兒雨聲，還是繼續剛才的故事？",
    "急がなくても大丈夫。ここで、少しゆっくりしていこう。",
    "我把書翻到折起來的那一頁。紙張輕輕摩擦的聲音，在安靜的房間裡格外清晰。",
    "Take your time. We can stay here until the rain stops.",
    "這是可以選取、搜尋的台本文字。點擊字幕或時間戳即可跳轉；拖曳選字不會改變播放位置。",
  ];
  const cues = Array.from({length:80},(_,index)=>({start:index*6,end:index*6+5,text:`${paragraphs[index%paragraphs.length]}${index%7===0 ? '\n遠くで、時計の音が聞こえる。' : ''}`}));
  let state: PlayerState = {...emptyPlayerState(),workId:"RJ99999999",trackId:"01.wav",subtitleId:"01.wav.vtt",duration:480,position:42,paused:true};
  let notify: ((value:PlayerState)=>void) | undefined;
  const player = new PlayerController({
    listenPlayer:async callback => {notify=callback;return ()=>{notify=undefined;};},
    playerSnapshot:async()=>state,
    playerSubtitle:async()=>cues,
    openPlayer:async()=>({media:{workId:"RJ99999999",tracks,subtitles:[{id:"01.wav.vtt",name:"01.wav.vtt"}],hasArchives:false},state}),
    controlPlayer:async(_workId,control)=>{
      const next = {...state,revision:state.revision+1};
      if(control.action==="seek") next.position=control.position;
      if(control.action==="pause") next.paused=control.paused;
      if(control.action==="speed") next.speed=control.speed;
      if(control.action==="volume") next.volume=control.volume;
      if(control.action==="offset") next.subtitleOffset=control.offset;
      if(control.action==="subtitle") next.subtitleId=control.subtitleId;
      if(control.action==="play") {next.trackId=control.trackId;next.position=0;next.paused=false;}
      if(control.action==="stop") {next.trackId=null;next.subtitleId=null;next.paused=true;}
      if(control.action==="next") {next.trackId=next.trackId==="01.wav" ? "02.wav" : "01.wav";next.position=0;}
      state=next;return next;
    },
  });
  onMount(()=>{
    const previousLocale=get(locale);locale.set(language);
    void player.start().then(()=>player.open({workId:"RJ99999999",title:"雨の日の書斎 — A quiet afternoon"}));
    const timer=setInterval(()=>{if(!state.paused){state={...state,position:Math.min(479,state.position+.25*state.speed),revision:state.revision+1};notify?.(state);}},250);
    return ()=>{clearInterval(timer);player.dispose();locale.set(previousLocale);};
  });
</script>
<div style="height:100dvh;background:var(--dldr-color-ui-background);color:var(--dldr-color-ui-ink);padding:32px;box-sizing:border-box;">
  <h1>DLDR</h1><p>Player preview · synthetic audio timeline and script</p>
</div>
<PlayerView {player} />
