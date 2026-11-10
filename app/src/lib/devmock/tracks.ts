// Mock of src-tauri/src/commands/tracks.rs.
import { needs, tracks } from "./db";
import { jobLock, runJob } from "./jobs";
import type { Handlers } from "./types";

// `?tracks=5000` on the dev URL pads the shared fixture with synthetic tracks, to check that the
// table stays smooth with a big library. Invented data only; nothing is written anywhere.
const extra = typeof location === "undefined" ? 0 : Math.min(20000, Number(new URLSearchParams(location.search).get("tracks")) || 0);
if (extra > tracks.length) {
  const genres = ["House", "Techno", "Drum & Bass", "Psy-Trance", "Trance", "Electro", "Breaks"];
  const keys = ["Am", "Em", "Dm", "C", "G", "F#m", "Bbm", "Ab", "Cm", "Gm", "D", "Bm", ""];
  for (let i = tracks.length; i < extra; i++) {
    tracks.push({
      track_id: i + 1,
      title: `Demo Track ${String(i + 1).padStart(4, "0")}`,
      artist: `Example Artist ${String.fromCharCode(65 + (i % 26))}`,
      album: `Sample Album ${1 + (i % 40)}`,
      genre: genres[i % genres.length],
      bpm: 100 + ((i * 7) % 80) + (i % 4) / 4,
      key: keys[(i * 7 + Math.floor(i / 5)) % keys.length],
      play_time_secs: 150 + ((i * 53) % 330),
      rating: (i * 3) % 6,
      location: `/pmdb_tracks/${i % 10}/${i % 16}/synthetic${i}.mp3`,
      file_size: 6_000_000 + i * 321,
      format: "MP3",
      analysed: true,
      times_played: (i * 11) % 9,
    });
  }
}

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
  // the browser has no file manager to open; only check the id like the real command does
  tracks_reveal: (a) => {
    if (!tracks.some((t) => t.track_id === a.trackId)) throw "That track is not in the library.";
    return null;
  },
};
