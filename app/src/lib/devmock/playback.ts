// Mock of src-tauri/src/commands/playback.rs. The browser preview has no audio files, so
// `playback_source` answers with an empty path and the engine runs a silent clock instead.
import { tracks } from "./db";
import type { Handlers } from "./types";

export const handlers: Handlers = {
  playback_source: () => ({ path: "" }),
  playback_allow_file: (a) => ({ path: String(a.path) }),
  // a straight grid at the track's BPM, 0.2 s in, first downbeat on the second beat; tracks
  // whose id is a multiple of 7 have no .str, to show that state in the player
  playback_beats: (a) => {
    const t = tracks.find((x) => x.location === a.location);
    if (!t || t.track_id % 7 === 0) {
      return { beats: [], first_downbeat: null, normalize_factor: 1, problem: "no .str file" };
    }
    const step = 60 / (t.bpm || 120);
    const beats: number[] = [];
    for (let s = 0.2; s < t.play_time_secs; s += step) beats.push(Math.round(s * 1e6) / 1e6);
    return { beats, first_downbeat: 1, normalize_factor: 0.9, problem: null };
  },
};
