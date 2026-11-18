import * as api from "$lib/api/tauri";
import type { Playlist } from "$lib/model/types";

export class PlaylistController {
  lists = $state<Playlist[]>([]);
  busy = $state(false);
  revision = $state(0);
  constructor(private readonly port = {list:api.listPlaylists,edit:api.editPlaylist}) {}
  async load() {
    if (this.busy) return;
    this.busy = true;
    try { this.lists = await this.port.list(); this.revision++; }
    finally { this.busy = false; }
  }
  async edit(request: api.PlaylistEdit) {
    if (this.busy) return null;
    this.busy = true;
    try {
      const result = await this.port.edit(request);
      this.lists = result.playlists; this.revision++;
      return result.playlistId;
    } finally { this.busy = false; }
  }
}
