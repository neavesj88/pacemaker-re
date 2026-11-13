// Mock of the play log commands (src-tauri/src/commands/history/logcmds.rs) over the shared
// track fixture. Invented nights of plays; the grouping and ranking follow the Rust rules
// (a pause longer than the gap starts a session; most plays, then newest, then lowest id).
// Dev URL switches: ?nolog (no log yet), ?logoff (logging switched off), ?logmismatch (a log
// that belongs to another card), ?logproblem (an unreadable file), ?logcandidate (no log yet, but
// the log of another drive matches this card).
import { tracks } from "./db";
import type { Handlers } from "./types";

const H = 3_600_000;
const D = 24 * H;
const NOW = Date.now();

const flag = (name: string) => {
  try {
    return new URLSearchParams(location.search).has(name);
  } catch {
    return false;
  }
};
const candidate = flag("logcandidate");
const noLog = flag("nolog") || candidate;
let mismatch = flag("logmismatch");
const problem = flag("logproblem");

type Hit = { track_id: number; at: number; plays: number; approx: boolean; time_known: boolean; reset: boolean };

// [days ago, start hour, tracks]: a night out is a run of plays 12 to 25 minutes apart.
const NIGHTS: [number, number, number][] = [
  [1, 20.5, 7], [3, 21, 9], [3.9, 9.5, 3], [8, 21.25, 8], [15, 20.75, 6], [22, 21, 10], [41, 21.5, 7], [77, 20.75, 9], [130, 21, 8], [200, 21.25, 6],
];

function build(): Hit[] {
  let seed = 7;
  const rnd = (n: number) => {
    seed = (seed * 1103515245 + 12345) & 0x7fffffff;
    return (seed >> 8) % n;
  };
  const out: Hit[] = [];
  for (const [ago, hour, count] of NIGHTS) {
    const day = new Date(NOW - ago * D);
    day.setHours(0, 0, 0, 0);
    let t = day.getTime() + hour * H;
    for (let i = 0; i < count; i++) {
      const plays = rnd(7) === 0 ? 2 : 1;
      const unknown = rnd(23) === 0;
      out.push({
        track_id: tracks[rnd(tracks.length)].track_id,
        at: Math.min(t, NOW - 1000),
        plays,
        approx: plays > 1 || unknown,
        time_known: !unknown,
        reset: false,
      });
      t += (12 + rnd(14)) * 60_000;
    }
  }
  return out.sort((a, b) => a.at - b.at || a.track_id - b.track_id);
}

let hits = build();
let cleared = noLog;
/** The user chose between continuing the other drive's log and starting a new one. */
let chose = false;
let settings = {
  enabled: !flag("logoff"),
  retention_days: 730,
  session_gap_minutes: 240,
  max_events: 400_000,
  identity_min_present_pct: 50,
  ask_before_new_log: true,
};
const CANDIDATE = { file: "playlog-0123456789abcdef.json", root: "E:\\", started_at: NOW - 230 * D, last_change_at: NOW - 2 * D, played_tracks: 14, present_tracks: 13 };
const STARTED = NOW - 230 * D;
let started = STARTED;
const BEFORE = new Map(tracks.map((t) => [t.track_id, { plays: (t.track_id * 3) % 11, last: NOW - ((t.track_id * 7) % 300 + 231) * D }]));

function inRange(a: Record<string, any>) {
  return hits.filter((h) => (a.from == null || h.at >= a.from) && (a.to == null || h.at <= a.to));
}

function overview(a: Record<string, any>) {
  const gap = Math.min(Math.max(a.gapMinutes ?? settings.session_gap_minutes, 1), 1440);
  const list = inRange(a);
  const groups: Hit[][] = [];
  for (const h of list) {
    const g = groups[groups.length - 1];
    if (g && h.at - g[g.length - 1].at <= gap * 60_000) g.push(h);
    else groups.push([h]);
  }
  const by = new Map<number, { track_id: number; plays: number; windows: number; first: number; last: number }>();
  for (const h of list) {
    const r = by.get(h.track_id) ?? { track_id: h.track_id, plays: 0, windows: 0, first: h.at, last: h.at };
    r.plays += h.plays;
    r.windows++;
    r.first = Math.min(r.first, h.at);
    r.last = Math.max(r.last, h.at);
    by.set(h.track_id, r);
  }
  const ranks = [...by.values()].sort((x, y) => y.plays - x.plays || y.last - x.last || x.track_id - y.track_id);
  return {
    enabled: settings.enabled,
    has_log: !cleared,
    problem: problem ? "playlog-0123456789abcdef.json is not readable: expected value at line 1 column 1" : null,
    problem_moves_aside: !!problem,
    mismatch,
    log_root: cleared ? "" : "E:\\",
    candidates: cleared && candidate && settings.ask_before_new_log ? [CANDIDATE] : [],
    started_at: cleared ? 0 : started,
    last_change_at: cleared ? 0 : hits.length ? hits[hits.length - 1].at + 40 * 60_000 : NOW,
    events: hits.length,
    file_bytes: hits.length * 21 + 1800,
    retention_days: settings.retention_days,
    gap_minutes: gap,
    first_play_at: hits[0]?.at ?? 0,
    last_play_at: hits[hits.length - 1]?.at ?? 0,
    total_plays: list.reduce((n, h) => n + h.plays, 0),
    removed_plays: 0,
    sessions_total: groups.length,
    sessions: groups
      .slice()
      .reverse()
      .slice(0, a.sessionLimit ?? 60)
      .map((g) => ({ start: g[0].at, end: g[g.length - 1].at, plays: g.reduce((n, h) => n + h.plays, 0), hits: g })),
    ranking_total: ranks.length,
    ranking: ranks.slice(0, a.rankLimit ?? 100),
    ranking_ids: ranks.map((r) => r.track_id),
  };
}

const empty = () => ({ ...overview({ from: 1, to: 0 }), has_log: false, started_at: 0, last_change_at: 0, events: 0, file_bytes: 0, first_play_at: 0, last_play_at: 0 });

export const playlogHandlers: Handlers = {
  playlog_settings_get: () => ({ ...settings }),
  playlog_settings_set: (a) => {
    const s = a.settings ?? {};
    settings = {
      enabled: !!s.enabled,
      retention_days: Math.min(Math.max(Math.trunc(s.retention_days ?? 730), 0), 36500),
      session_gap_minutes: Math.min(Math.max(Math.trunc(s.session_gap_minutes ?? 240), 1), 1440),
      max_events: Math.min(Math.max(Math.trunc(s.max_events ?? 400_000), 1000), 2_000_000),
      identity_min_present_pct: Math.min(Math.max(Math.trunc(s.identity_min_present_pct ?? 50), 0), 100),
      ask_before_new_log: s.ask_before_new_log !== false,
    };
    return { ...settings };
  },
  playlog_record: () => {
    if (!settings.enabled) return { enabled: false, status: "off", tracks: 0, pruned: 0, detail: null };
    if (cleared && candidate && settings.ask_before_new_log && !chose) {
      return { enabled: true, status: "candidate", tracks: 0, pruned: 0, detail: "the play log from E:\\ has 13 of the 14 tracks it has seen played on this card" };
    }
    if (cleared) {
      cleared = false;
      started = NOW; // a log started now has no plays yet
      hits = [];
      return { enabled: true, status: "started", tracks: 14, pruned: 0, detail: null };
    }
    if (mismatch) return { enabled: true, status: "mismatch", tracks: 0, pruned: 0, detail: "only 1 of the 14 tracks this log has seen played are in this library" };
    return { enabled: true, status: "unchanged", tracks: 0, pruned: 0, detail: null };
  },
  playlog_adopt: () => {
    if (!settings.enabled) return { enabled: false, status: "off", tracks: 0, pruned: 0, detail: null };
    mismatch = false;
    return { enabled: true, status: "adopted", tracks: 2, pruned: 0, detail: "9 tracks this library lacks set aside, 4 tracks new to the log taken as they are now" };
  },
  playlog_start_new: () => {
    chose = true;
    cleared = false;
    started = NOW;
    hits = [];
    return { enabled: true, status: "started", tracks: 14, pruned: 0, detail: null };
  },
  playlog_adopt_log: () => {
    chose = true;
    cleared = false; // the other drive's log (and its nights) is this library's now
  },
  playlog_overview: (a) => (cleared || problem ? { ...empty(), problem: problem ? overview({}).problem : null, enabled: settings.enabled } : overview(a)),
  playlog_track: (a) => {
    if (cleared) return null;
    const t = tracks.find((x) => x.track_id === a.trackId);
    if (!t) return null;
    const mine = hits.filter((h) => h.track_id === a.trackId).reverse();
    const b = BEFORE.get(a.trackId)!;
    const logged = mine.reduce((n, h) => n + h.plays, 0);
    return {
      track_id: a.trackId,
      before_plays: b.plays,
      before_last: b.plays > 0 ? b.last : -1,
      logged_plays: logged,
      now_plays: b.plays + logged,
      now_last: mine.length ? mine[0].at : b.plays > 0 ? b.last : -1,
      hits: mine,
    };
  },
  playlog_clear: () => {
    const had = !cleared;
    cleared = true;
    return had;
  },
};
