// Mock of src-tauri/src/commands/import.rs.
import { baseName } from "$lib/format";
import { AUDIO, tracks } from "./db";
import { jobLock, runJob } from "./jobs";
import type { Handlers } from "./types";

export const handlers: Handlers = {
  expand_audio_paths: (a) => (a.paths as string[]).filter((p) => AUDIO.test(p) || !/\.[a-z0-9]+$/i.test(p)),
  add_tracks: (a) => {
    if (jobLock.running) throw "Another job is still running.";
    const files = (a.files as string[]).map(baseName);
    runJob("add", files, (label, i) => {
      if (/bad/i.test(label)) return { error: 'Decode(Unsupported("no suitable format reader found"))' };
      if (!/skip/i.test(label)) {
        const id = tracks.length + 1;
        tracks.push({
          ...tracks[i % 6],
          track_id: id,
          title: label.replace(/\.[^.]+$/, ""),
          location: `/pmdb_tracks/0/0/new${id}.mp3`,
          analysed: true,
        });
      }
      return { bpm: 120 + i * 3.5 };
    });
    return null;
  },
};
