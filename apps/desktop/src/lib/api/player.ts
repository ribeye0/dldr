import { diagnosticInvoke as invoke } from "./diagnostics";
import { listen } from "@tauri-apps/api/event";
export type MediaTrack = { id: string; name: string; group: string; format: string; size: number };
export type MediaSubtitle = { id: string; name: string };
export type WorkMedia = { workId: string; tracks: MediaTrack[]; subtitles: MediaSubtitle[]; hasArchives: boolean; warnings?: string[] };
export type Cue = { start: number; end: number; text: string };
export type PlayerState = {
  workId: string | null; trackId: string | null; position: number; duration: number;
  paused: boolean; loading: boolean; volume: number; speed: number; subtitleId: string | null;
  subtitleOffset: number; durations: Record<string, number>; error: string | null; revision: number;
};
export type PlayerControl =
  | { action: "play"; trackId: string } | { action: "pause"; paused: boolean }
  | { action: "seek"; position: number } | { action: "volume"; volume: number }
  | { action: "speed"; speed: number } | { action: "subtitle"; subtitleId: string | null }
  | { action: "offset"; offset: number } | { action: "next"; direction: number } | { action: "stop" };
export const openPlayer = (workId: string) => invoke<{ media: WorkMedia; state: PlayerState }>("open_player", { request: { workId } });
export const controlPlayer = (workId: string, control: PlayerControl) => invoke<PlayerState>("control_player", { request: { workId, control } });
export const playerSnapshot = () => invoke<PlayerState>("player_snapshot");
export const playerSubtitle = (workId: string, subtitleId: string) => invoke<Cue[]>("player_subtitle", { request: { workId, subtitleId } });
export const listenPlayer = (handler: (value: PlayerState) => void) => listen<PlayerState>("dldr-player-state", (event) => handler(event.payload));
