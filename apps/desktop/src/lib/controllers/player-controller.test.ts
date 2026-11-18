import { describe, it, expect, vi } from "vitest";
import { PlayerController, emptyPlayerState } from "./player-controller.svelte";
import type { PlayerState, WorkMedia } from "$lib/api/player";
function deferred<T>() { let resolve!: (value:T)=>void; let reject!: (error:unknown)=>void; const promise = new Promise<T>((a,b)=>{resolve=a;reject=b;}); return {promise,resolve,reject}; }
const media = (workId:string): WorkMedia => ({workId,tracks:[{id:"01.wav",name:"01.wav",group:"",format:"WAV",size:1}],subtitles:[{id:"01.vtt",name:"01.vtt"}],hasArchives:false});
function fixture() {
  let event!: (s:PlayerState)=>void;
  const unlisten = vi.fn();
  const api = {openPlayer:vi.fn(),controlPlayer:vi.fn(),playerSubtitle:vi.fn(),playerSnapshot:vi.fn(async()=>emptyPlayerState()),listenPlayer:vi.fn(async handler=>{event=handler;return unlisten;})};
  const controller = new PlayerController(api);
  return {controller,api,emit:(s:PlayerState)=>event(s),unlisten};
}
describe("player controller",()=>{
  it("reconnects to an existing native session without sending play or stop",async()=>{
    const {controller,api}=fixture();
    const state={...emptyPlayerState(),workId:"one",trackId:"01.wav",paused:false,position:30,duration:120,revision:5};
    api.playerSnapshot.mockResolvedValue(state);
    api.openPlayer.mockResolvedValue({media:media("one"),state});
    await controller.start();
    expect(controller.media?.workId).toBe("one");expect(controller.state.position).toBe(30);
    expect(controller.state.paused).toBe(false);expect(controller.expanded).toBe(false);
    expect(api.controlPlayer).not.toHaveBeenCalled();
  });
  it("keeps latest work and ignores late open responses",async()=>{
    const {controller,api}=fixture(); const old=deferred<{media:WorkMedia,state:PlayerState}>();
    api.openPlayer.mockReturnValueOnce(old.promise).mockResolvedValueOnce({media:media("new"),state:{...emptyPlayerState(),workId:"new",revision:2}});
    const first=controller.open({workId:"old",title:"Old"}); await controller.open({workId:"new",title:"New"});
    old.resolve({media:media("old"),state:{...emptyPlayerState(),workId:"old",revision:1}});await first;
    expect(controller.media?.workId).toBe("new");expect(controller.title).toBe("New");expect(controller.loading).toBe(false);
  });
  it("rejects out-of-order state and old subtitle reads after changing cues",async()=>{
    const {controller,api,emit}=fixture();await controller.start();
    const old=deferred<{start:number,end:number,text:string}[]>();api.playerSubtitle.mockReturnValueOnce(old.promise);
    emit({...emptyPlayerState(),workId:"one",subtitleId:"01.vtt",revision:4});
    emit({...emptyPlayerState(),workId:"two",revision:5});
    old.resolve([{start:0,end:1,text:"wrong work"}]);await old.promise;await Promise.resolve();
    emit({...emptyPlayerState(),workId:"one",revision:3});
    expect(controller.state.workId).toBe("two");expect(controller.cues).toEqual([]);expect(controller.subtitleLoading).toBe(false);
  });
  it("serializes controls, reports failures and keeps native playback on closing the panel",async()=>{
    const {controller,api}=fixture();
    api.openPlayer.mockResolvedValue({media:media("one"),state:{...emptyPlayerState(),workId:"one",revision:1}});
    await controller.open({workId:"one",title:"One"});
    const pending=deferred<PlayerState>();api.controlPlayer.mockReturnValueOnce(pending.promise);
    const first=controller.run({action:"play",trackId:"01.wav"});await controller.run({action:"stop"});
    expect(api.controlPlayer).toHaveBeenCalledTimes(1);controller.expanded=false;
    pending.reject("Synthetic playback error");await first;
    expect(controller.error).toContain("Synthetic playback error");expect(controller.busy).toBe(false);
    expect(api.controlPlayer).toHaveBeenCalledTimes(1);
  });
});

it("keeps the latest seek during a slow IPC call and shields its preview from clock events",async()=>{
  const {controller,api,emit}=fixture();await controller.start();
  const state={...emptyPlayerState(),workId:"one",trackId:"01.wav",duration:120,position:2,paused:false,revision:1};
  api.openPlayer.mockResolvedValue({media:media("one"),state});
  await controller.open({workId:"one",title:"One"});
  const first=deferred<PlayerState>(),last=deferred<PlayerState>();
  api.controlPlayer.mockReturnValueOnce(first.promise).mockReturnValueOnce(last.promise);
  const running=controller.run({action:"seek",position:10});
  await controller.run({action:"seek",position:30});
  await controller.run({action:"seek",position:60});
  emit({...state,position:3,revision:2});
  expect(controller.seekTarget).toBe(60);expect(controller.seeking).toBe(true);
  expect(api.controlPlayer).toHaveBeenCalledTimes(1);
  first.resolve({...state,position:10,revision:3});await first.promise;await Promise.resolve();
  expect(api.controlPlayer).toHaveBeenLastCalledWith("one",{action:"seek",position:60});
  expect(api.controlPlayer).toHaveBeenCalledTimes(2);expect(controller.seekTarget).toBe(60);
  last.resolve({...state,position:60,revision:4});await running;
  expect(controller.state.position).toBe(60);expect(controller.seekTarget).toBeNull();
  expect(controller.seeking).toBe(false);expect(controller.busy).toBe(false);
});

it("clears pending seeks when the playing track changes",async()=>{
  const {controller,api,emit}=fixture();await controller.start();
  const state={...emptyPlayerState(),workId:"one",trackId:"01.wav",duration:120,revision:1};
  api.openPlayer.mockResolvedValue({media:media("one"),state});await controller.open({workId:"one",title:"One"});
  const first=deferred<PlayerState>();api.controlPlayer.mockReturnValueOnce(first.promise);
  const running=controller.run({action:"seek",position:10});await controller.run({action:"seek",position:60});
  emit({...state,trackId:"02.wav",revision:3});
  first.resolve({...state,position:10,revision:2});await running;
  expect(api.controlPlayer).toHaveBeenCalledTimes(1);expect(controller.state.trackId).toBe("02.wav");
  expect(controller.seekTarget).toBeNull();expect(controller.busy).toBe(false);
});
