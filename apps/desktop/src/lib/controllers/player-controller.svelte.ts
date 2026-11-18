import * as playerApi from "$lib/api/player";
import type { Cue, PlayerControl, PlayerState, WorkMedia } from "$lib/api/player";
import { errorMessage } from "$lib/utils/format";
export const emptyPlayerState = (): PlayerState => ({ workId: null, trackId: null, position: 0, duration: 0, paused: true, loading: false, volume: 70, speed: 1, subtitleId: null, subtitleOffset: 0, durations: {}, error: null, revision: 0 });
export class PlayerController {
  media = $state<WorkMedia | null>(null);
  state = $state<PlayerState>(emptyPlayerState());
  cues = $state<Cue[]>([]);
  title = $state(""); cover = $state<string | null>(null);
  expanded = $state(false); loading = $state(false); busy = $state(false);
  seeking = $state(false); seekTarget = $state<number | null>(null);
  private queuedSeek: { workId: string; trackId: string; position: number } | null = null;
  error = $state(""); subtitleError = $state(""); subtitleLoading = $state(false);
  private disposed = false;
  private generation = 0; private cueGeneration = 0; private cueKey = "";
  constructor(private readonly api: Pick<typeof playerApi, "openPlayer" | "controlPlayer" | "playerSnapshot" | "playerSubtitle" | "listenPlayer"> = playerApi) {}
  async start() {
    const unlisten = await this.api.listenPlayer((state) => this.apply(state));
    if (this.disposed) { unlisten(); return () => {}; }
    try {
      this.apply(await this.api.playerSnapshot());
      if (!this.disposed && this.generation === 0 && this.state.workId) {
        const restoreGeneration = this.generation + 1;
        await this.open({ workId:this.state.workId, title:this.state.workId });
        if (this.generation === restoreGeneration) this.expanded = false;
      }
    } catch (cause) { if (!this.disposed) this.error = errorMessage(cause); }
    return unlisten;
  }
  dispose() { this.disposed = true; this.queuedSeek = null; this.seekTarget = null; this.generation++; this.cueGeneration++; }
  private apply(state: PlayerState) {
    if (this.disposed || state.revision < this.state.revision) return;
    if (state.workId !== this.state.workId || state.trackId !== this.state.trackId) {
      this.queuedSeek = null; this.seekTarget = null;
    }
    this.state = state;
    const key = `${state.workId ?? ""}\0${state.subtitleId ?? ""}`;
    if (key !== this.cueKey) {
      this.cueKey = key; this.cues = []; this.subtitleError = "";
      const generation = ++this.cueGeneration;
      if (state.workId && state.subtitleId) {
        this.subtitleLoading = true;
        void this.api.playerSubtitle(state.workId, state.subtitleId).then(cues => {
          if (!this.disposed && generation === this.cueGeneration) this.cues = cues;
        }).catch(cause => {
          if (!this.disposed && generation === this.cueGeneration) this.subtitleError = errorMessage(cause);
        }).finally(() => { if (!this.disposed && generation === this.cueGeneration) this.subtitleLoading = false; });
      } else this.subtitleLoading = false;
    }
  }
  async open(work: { workId: string; title: string; thumbnailUrl?: string | null }) {
    if (this.busy) return;
    const generation = ++this.generation;
    this.loading = true; this.error = ""; this.expanded = true;
    try {
      const { media, state } = await this.api.openPlayer(work.workId);
      if (this.disposed || generation !== this.generation) return;
      this.media = media; this.title = work.title; this.cover = work.thumbnailUrl ?? null;
      this.apply(state);
    } catch (cause) { if (!this.disposed && generation === this.generation) this.error = errorMessage(cause); }
    finally { if (!this.disposed && generation === this.generation) this.loading = false; }
  }
  async run(control: PlayerControl) {
    if (this.disposed || this.loading || !this.media || this.media.workId !== this.state.workId) return;
    if (control.action === "seek") {
      if (!this.state.trackId || !Number.isFinite(control.position) || control.position < 0) return;
      const position = this.state.duration > 0 ? Math.min(control.position, this.state.duration) : control.position;
      // Keep the newest destination while an IPC call is in flight. Clock
      // events may update native state, but must not move the user's preview.
      this.queuedSeek = { workId: this.media.workId, trackId: this.state.trackId, position };
      this.seekTarget = position;
      await this.flushSeeks();
      return;
    }
    if (this.busy) return;
    this.busy = true; this.error = "";
    try { this.apply(await this.api.controlPlayer(this.media.workId, control)); }
    catch (cause) { if (!this.disposed) this.error = errorMessage(cause); }
    finally {
      if (!this.disposed) { this.busy = false; await this.flushSeeks(); }
    }
  }
  private async flushSeeks() {
    if (this.busy || this.disposed || !this.queuedSeek) return;
    this.busy = true; this.seeking = true; this.error = "";
    try {
      while (this.queuedSeek && !this.disposed) {
        const seek = this.queuedSeek;
        this.queuedSeek = null;
        if (seek.workId !== this.state.workId || seek.trackId !== this.state.trackId) continue;
        try { this.apply(await this.api.controlPlayer(seek.workId, {action: "seek", position: seek.position})); }
        catch (cause) { if (!this.disposed) this.error = errorMessage(cause); }
      }
    } finally {
      this.busy = false; this.seeking = false; this.seekTarget = null;
    }
  }
}
