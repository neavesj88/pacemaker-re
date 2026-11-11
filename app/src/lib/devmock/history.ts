// Mock of src-tauri/src/commands/history.rs. Invented counters over the shared track fixture.
import { cases, tracks } from "./db";
import { playlogHandlers } from "./playlogMock";
import type { Handlers } from "./types";

const H = 3_600_000;
const NOW = Date.now();

// Three nights of plays: [track_id, hours before now, times_played]
const plays: [number, number, number][] = [
  [3, 3, 8], [9, 3.2, 5], [14, 3.5, 2], [6, 3.8, 11], [20, 4.1, 1], [2, 4.4, 4],
  [7, 52, 6], [12, 52.3, 3], [1, 52.6, 7], [18, 53, 2], [5, 53.4, 9],
  [16, 170, 1], [21, 170.4, 3], [8, 170.8, 2],
];

// Add ?noplays to the dev URL (or set localStorage.mockNoPlays = "1") to see the zero-plays state.
const noPlays = (() => {
  try {
    return new URLSearchParams(location.search).has("noplays") || localStorage.getItem("mockNoPlays") === "1";
  } catch {
    return false;
  }
})();

const counters = new Map<number, { times_played: number; last_played: number }>();
if (!noPlays) for (const [id, ago, n] of plays) counters.set(id, { times_played: n, last_played: Math.round(NOW - ago * H) });

const entries = () => tracks.map((t) => ({ track_id: t.track_id, ...(counters.get(t.track_id) ?? { times_played: 0, last_played: -1 }) }));

function sessionsOf(gapMin: number) {
  const timed = entries().filter((e) => e.last_played > 0).sort((a, b) => a.last_played - b.last_played || a.track_id - b.track_id);
  const out: { start: number; end: number; track_ids: number[]; played_at: number[] }[] = [];
  for (const e of timed) {
    const s = out[out.length - 1];
    if (s && e.last_played - s.end <= gapMin * 60_000) {
      s.end = e.last_played;
      s.track_ids.push(e.track_id);
      s.played_at.push(e.last_played);
    } else out.push({ start: e.last_played, end: e.last_played, track_ids: [e.track_id], played_at: [e.last_played] });
  }
  return out.reverse();
}

let nextCase = 100;

export const handlers: Handlers = {
  ...playlogHandlers,
  history_overview: (a) => {
    const gap = Math.min(Math.max(a.gapMinutes ?? 240, 1), 1440);
    const e = entries();
    const played = e.filter((x) => x.times_played > 0 || x.last_played > 0);
    return {
      any_plays: played.length > 0,
      tracks_total: e.length,
      tracks_played: played.length,
      total_plays: e.reduce((n, x) => n + x.times_played, 0),
      sessions: sessionsOf(gap),
      gap_minutes: gap,
      most_played: e
        .filter((x) => x.times_played > 0)
        .sort((x, y) => y.times_played - x.times_played || y.last_played - x.last_played || x.track_id - y.track_id)
        .slice(0, 100),
      never_played: e.filter((x) => x.times_played <= 0 && x.last_played <= 0).map((x) => x.track_id),
      cases: cases.map((c, i) => ({
        id: c.id,
        name: c.name,
        times_played: noPlays ? 0 : ([8, 3, 0][i] ?? 0),
        last_played: noPlays || i >= 2 ? 0 : Math.round(NOW - (6 + i * 60) * H),
        track_count: c.track_count,
      })),
      mixes: [{ id: 1, name: "Demo Mix 1", times_played: noPlays ? 0 : 2, last_played: noPlays ? 0 : Math.round(NOW - 30 * H), track_count: 5 }],
    };
  },
  history_save_as_case: (a) => {
    const name = String(a.name ?? "").trim();
    if (!name) throw "case name is empty";
    if (!a.trackIds?.length) throw "no tracks to put in the case";
    const ids = [...new Set<number>(a.trackIds)];
    const id = nextCase++;
    cases.push({ id, name, play_time_secs: 0, track_count: ids.length, ids });
    return id;
  },
};
