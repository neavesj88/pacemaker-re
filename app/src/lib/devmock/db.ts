// The synthetic library every area's mock reads and mutates. Shared, integration-owned: an area
// with its own fixtures (history sessions, grids, ...) keeps them in its own devmock/<area>.ts.
// Nothing here is real data; it is invented placeholder content.

const genres = ["House", "Techno", "Drum & Bass", "Psy-Trance", "Trance"];

export const tracks = Array.from({ length: 24 }, (_, i) => ({
  track_id: i + 1,
  title: `Demo Track ${String(i + 1).padStart(2, "0")}`,
  artist: `Example Artist ${String.fromCharCode(65 + (i % 8))}`,
  album: `Sample Album ${1 + (i % 5)}`,
  genre: genres[i % genres.length],
  bpm: [124, 126, 128, 87, 138, 140][i % 6] + (i % 3 === 0 ? 0 : 0.5),
  key: ["Am", "Cm", "F#m", "Abm", ""][i % 5],
  // three long ones (25:30, 59:59, 1:12:05) so every Time column is seen at its widest
  play_time_secs: i === 5 ? 1530 : i === 9 ? 3599 : i === 11 ? 4325 : 180 + ((i * 37) % 240),
  rating: i % 6,
  location: `/pmdb_tracks/${i % 10}/${(i * 3) % 16}/demo${i}.mp3`,
  file_size: 8_000_000 + i * 12345,
  format: "MP3",
  analysed: i % 7 !== 3,
  times_played: i % 4,
  last_played: i % 4 === 0 ? -1 : Date.UTC(2026, 4, 1) + i * 86_400_000,
  year: i % 7 === 0 ? 0 : 2008 + (i % 15),
  label: i % 3 === 0 ? "" : `Example Label ${String.fromCharCode(65 + (i % 4))}`,
  remixer: i % 5 === 0 ? `Example Remixer ${1 + (i % 3)}` : "",
  composer: i % 4 === 1 ? `Example Composer ${1 + (i % 2)}` : "",
  comments: i % 6 === 2 ? `Demo note ${i}` : "",
  date_added: i % 9 === 8 ? -1 : Date.UTC(2025, 9, 1) + i * 5 * 86_400_000,
  bit_rate: [320, 256, 192, 0][i % 4],
}));

// tracks whose .str is bad, with the reasons the real backend reports
export const needs = new Map<number, string>([
  [4, "never analysed"],
  [11, "empty .str file (0 bytes)"],
  [18, "no .str file"],
]);
for (const t of tracks) t.analysed = !needs.has(t.track_id) || needs.get(t.track_id) !== "never analysed";

export const cases = [
  { id: 1, name: "Warm-up", play_time_secs: 3600, track_count: 6, ids: [1, 2, 3, 5, 8, 13] },
  { id: 2, name: "Peak time", play_time_secs: 2400, track_count: 4, ids: [21, 22, 4, 999] },
  { id: 3, name: "Empty case", play_time_secs: 0, track_count: 0, ids: [] as number[] },
];

export const AUDIO = /\.(mp3|flac|wav|m4a|aac|ogg|aiff?)$/i;
