import type { Cue } from "$lib/api/player";
export function activeCues(cues: Cue[], position: number, offset: number): Cue[] {
  const time = position - offset;
  return cues.filter(cue => cue.start <= time && time < cue.end);
}
export function mediaTime(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return "0:00";
  const total = Math.floor(seconds), hours = Math.floor(total / 3600);
  return `${hours ? `${hours}:` : ""}${hours ? String(Math.floor(total / 60) % 60).padStart(2, "0") : Math.floor(total / 60)}:${String(total % 60).padStart(2, "0")}`;
}
