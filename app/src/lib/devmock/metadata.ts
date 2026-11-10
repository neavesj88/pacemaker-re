// Mock of src-tauri/src/commands/metadata.rs. Add one entry per command, keyed by command name;
// this file is picked up automatically by devmock/index.ts.
import { tracks } from "./db";
import type { Handlers } from "./types";

export const handlers: Handlers = {
  metadata_set_rating: (a) => {
    const t = tracks.find((x) => x.track_id === a.trackId);
    if (!t) throw `no track ${a.trackId}`;
    if (a.stars < 0 || a.stars > 5) throw `rating ${a.stars} is above 5`;
    t.rating = a.stars;
  },
};
