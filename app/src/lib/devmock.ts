// Browser preview: when the UI runs outside Tauri (plain `npm run dev` in a browser), answer
// the app's commands with a synthetic library so the interface can be developed and checked
// without the native shell. Never loaded inside the Tauri app.
import { mockIPC } from "@tauri-apps/api/mocks";

const genres = ["House", "Techno", "Drum & Bass", "Psy-Trance", "Trance"];
const tracks = Array.from({ length: 24 }, (_, i) => ({
  track_id: i + 1,
  title: `Demo Track ${String(i + 1).padStart(2, "0")}`,
  artist: `Example Artist ${String.fromCharCode(65 + (i % 8))}`,
  album: `Sample Album ${1 + (i % 5)}`,
  genre: genres[i % genres.length],
  bpm: [124, 126, 128, 87, 138, 140][i % 6] + (i % 3 === 0 ? 0 : 0.5),
  key: ["Am", "Cm", "F#m", "Ab Minor", ""][i % 5],
  play_time_secs: 180 + ((i * 37) % 240),
  rating: i % 6,
  location: `/pmdb_tracks/${i % 10}/${(i * 3) % 16}/demo${i}.mp3`,
  file_size: 8_000_000 + i * 12345,
  format: "MP3",
  analysed: i % 7 !== 3,
  times_played: i % 4,
}));

export function installDevMock() {
  mockIPC((cmd, args) => {
    if (cmd === "list_volumes") return ["P:\\"];
    if (cmd === "open_library") {
      const root = (args as { root?: string })?.root ?? "P:\\";
      return {
        root,
        problems: root.includes("broken") ? ["user_version is 12, device requires 677"] : [],
        tracks,
        mixes: [{ id: 1, name: "Example Mix", play_time_secs: 600, track_count: 3 }],
        cases: [{ id: 1, name: "Warm-up", play_time_secs: 3600, track_count: 12 }],
      };
    }
    if (cmd === "plugin:dialog|open") return "P:\\";
    throw new Error(`devmock: unhandled command ${cmd}`);
  });
}
