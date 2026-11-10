// Mock of src-tauri/src/commands/tracks.rs.
import { needs, tracks } from "./db";
import { jobLock, runJob } from "./jobs";
import type { Handlers } from "./types";

export const handlers: Handlers = {
  reanalyse_tracks: (a) => {
    if (jobLock.running) throw "Another job is still running.";
    const ids = a.trackIds as number[];
    runJob("reanalyse", ids.map((id) => tracks.find((t) => t.track_id === id)?.title ?? String(id)), (_l, i) => {
      needs.delete(ids[i]);
      const t = tracks.find((x) => x.track_id === ids[i]);
      if (t) t.analysed = true;
      return { bpm: 126 };
    });
    return null;
  },
};
