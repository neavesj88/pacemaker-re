// Mock of src-tauri/src/commands/tracks.rs.
import { needs, tracks } from "./db";
import { jobLock, runJob } from "./jobs";
import { mixNamesUsing } from "./mixes";
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
      last_played: i % 3 === 0 ? -1 : Date.UTC(2026, 3, 1) + (i % 200) * 86_400_000,
      year: i % 11 === 0 ? 0 : 1995 + (i % 30),
      label: i % 4 === 0 ? "" : `Example Label ${1 + (i % 12)}`,
      remixer: i % 9 === 0 ? `Example Remixer ${1 + (i % 5)}` : "",
      composer: i % 7 === 0 ? `Example Composer ${1 + (i % 6)}` : "",
      comments: i % 13 === 0 ? `Demo note ${i}` : "",
      date_added: Date.UTC(2024, 0, 1) + (i % 700) * 86_400_000,
      bit_rate: [320, 256, 192, 128, 0][i % 5],
    });
  }
}

// Saved filter presets and the remembered view, per library, in memory (the real ones are a JSON
// file per library in the app config dir). Same rules as commands/tracks/views.rs: names are
// trimmed and unique ignoring case, a taken name needs `replace`.
type MockPreset = { id: string; name: string; state: unknown };
const views = new Map<string, { presets: MockPreset[]; state: unknown | null }>();
const viewsOf = (root: string) => {
  let v = views.get(root);
  if (!v) views.set(root, (v = { presets: [], state: null }));
  return v;
};
const payload = (root: string) => ({ presets: [...viewsOf(root).presets].sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase())), state: viewsOf(root).state, notice: null });
let presetSeq = 0;

export const handlers: Handlers = {
  tracks_views_load: (a) => payload(a.root),
  tracks_views_save_preset: (a) => {
    const name = String(a.name).split(/s+/).filter(Boolean).join(" ");
    if (!name) throw "A preset needs a name.";
    if (name.length > 60) throw "The preset name is longer than 60 characters.";
    if (!a.state || typeof a.state !== "object" || Array.isArray(a.state)) throw "The filter state is not an object.";
    const v = viewsOf(a.root);
    const same = v.presets.find((p) => p.name.toLowerCase() === name.toLowerCase());
    if (same && !a.replace) throw `A preset named "${same.name}" already exists.`;
    if (same) Object.assign(same, { name, state: a.state });
    else v.presets.push({ id: `p${++presetSeq}`, name, state: a.state });
    return payload(a.root);
  },
  tracks_views_delete_preset: (a) => {
    const v = viewsOf(a.root);
    if (!v.presets.some((p) => p.id === a.id)) throw "That preset no longer exists.";
    v.presets = v.presets.filter((p) => p.id !== a.id);
    return payload(a.root);
  },
  tracks_views_set_state: (a) => {
    viewsOf(a.root).state = a.state ?? null;
    return null;
  },
  tracks_mixes_using: (a) => mixNamesUsing(a.trackIds as number[]),
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
