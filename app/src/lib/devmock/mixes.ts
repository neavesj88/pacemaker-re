// Mock of src-tauri/src/commands/mixes.rs. The mix fixture lives here (mixes-owned); library.ts
// reads `mixes` for the sidebar count. Invented placeholder data only.
//
// `docs` holds one realistic MixDoc ("Example Mix": three clips of the mock library at 128 BPM,
// two decks, tools, a loop chain, two transitions) plus whatever the UI creates; mix_open returns
// copies, mix_save stores them, so the editor can be exercised end to end in a browser. The
// beat-match and auto-transition mocks use the same pure functions as the editor.
import type { BeatGrid, BeatmatchSuggestion, Clip, MixDoc, MixMeta, TrackInfo, TransitionSuggestion } from "$lib/api/mixes";
import * as E from "$lib/state/mixedit";
import { needs, tracks } from "./db";
import type { Handlers } from "./types";

const SR = E.SR;

/** The mix list as the library listing and `mix_list` return it (kept in step with `docs`). */
export const mixes: { id: number; name: string; play_time_secs: number; track_count: number }[] = [];

const docs = new Map<number, MixDoc>();
let nextId = 2;

function grid(bpm: number, durationSecs: number, id: number): BeatGrid {
  const step = (60 / bpm) * SR;
  const first = Math.round(0.08 * SR + (id % 3) * 700);
  const n = Math.floor((durationSecs * SR - first) / step);
  return {
    sourceRate: SR,
    beatStartOffset: first,
    beatFrames: Array.from({ length: n }, (_, i) => Math.round(first + i * step)),
    beatBpm: Array.from({ length: n }, () => bpm),
    firstDownbeat: id % 2,
  };
}

function trackInfo(id: number): TrackInfo {
  const t = tracks.find((x) => x.track_id === id);
  if (!t) {
    return {
      trackId: id, available: false, problem: id < 0 ? "the track was removed from the library" : "the track is not in the library",
      title: "", artist: "", album: "", genre: "", key: "", bpm: 0, durationSecs: 0, sourceRate: SR, location: "", grid: null, gridProblem: null,
    };
  }
  const bad = needs.get(id);
  return {
    trackId: id,
    available: true,
    problem: null,
    title: t.title,
    artist: t.artist,
    album: t.album,
    genre: t.genre,
    key: t.key,
    bpm: t.bpm,
    durationSecs: t.play_time_secs,
    sourceRate: SR,
    location: t.location,
    grid: bad ? null : grid(t.bpm, t.play_time_secs, id),
    gridProblem: bad ?? null,
  };
}

/** What `mix_make_clip` builds: channel 0 at 0, speed for `tempo`, no tools. */
function makeClip(trackId: number, tempo: number | null): Clip {
  const info = trackInfo(trackId);
  if (!info.available) throw `track ${trackId} is not in the library`;
  const speed = tempo && info.bpm > 0 ? tempo / info.bpm : 1;
  const length = (info.durationSecs * SR) / speed;
  const z = { start: 0, length, trimStart: 0, trimEnd: 0, limitSize: true, startSec: 0, lengthSec: length / SR };
  return {
    ...z,
    id: E.newId(),
    trackId,
    channel: 0,
    visibleStart: 0,
    visibleEnd: length,
    visibleStartSec: 0,
    visibleEndSec: length / SR,
    bpm: info.bpm,
    tempo: [{ pos: 0, speed }],
    pitchMode: false,
    key: [{ pos: 0, cents: 0, flags: 1 }],
    gain: [{ pos: 0, value: 0 }, { pos: Math.round(length) - 1, value: 0 }],
    processes: [],
    filePath: info.location,
    offset: 0,
    clipName: "AudioClip",
    clipBypassed: false,
    toniumId: "",
    otherIds: { graceNoteId: "", musicDnsId: "" },
    fuzzyId: { artist: info.artist, album: info.album, title: info.title, genre: info.genre },
    volume: {
      name: "noname", bypassed: false, effects: [{ name: "Volume", bypassed: false, class: "volume" }],
      start: 0, length, trimStart: 0, trimEnd: 0, limitSize: false, startSec: 0, lengthSec: length / SR,
      sessionEventId: "", otherParams: {}, gainKeyPresent: true,
    },
    track: info,
  };
}

const emptyMeta = (): MixMeta => ({
  genre: "", year: 2026, rating: 0, producer: "", remixer: "", comments: "",
  dateCreated: 1_760_000_000_000, dateModified: 1_760_000_000_000, timesPlayed: 0, lastPlayed: 0, playTimeSecs: 0, creatorId: "pacemaker-re;Editor;0.1.0;0",
});

function emptyDoc(id: number, name: string): MixDoc {
  return E.refresh({ id, name, genre: "", artist: "", deviceInfo: "", sampleRate: SR, tempoBpm: null, clips: [], transitions: [], length: 0, lengthSec: 0, meta: emptyMeta() });
}

function example(): MixDoc {
  const tempo = 128;
  const bar = ((60 / tempo) * 4) * SR;
  let d = emptyDoc(1, "Example Mix");
  d = { ...d, tempoBpm: tempo, genre: "House", artist: "Example Artist", meta: { ...emptyMeta(), genre: "House", rating: 3, comments: "Invented demo mix" } };

  const a = makeClip(1, tempo);
  d = E.addClip(d, a, 0, 0);
  const aEnd = E.clipOf(d, a.id)!.visibleEnd;

  // B enters 16 bars before A ends, on a beat of A
  const b = makeClip(2, tempo);
  const bStart = E.snap(aEnd - 16 * bar, E.clipBeats(E.clipOf(d, a.id)!), bar);
  d = E.addClip(d, b, bStart, 1);

  // C has a cue 8 bars in and enters 16 bars before B ends
  const c0 = makeClip(3, tempo);
  const cCue = 8 * bar;
  const c = { ...c0, trimStart: cCue };
  const bEnd = E.clipOf(d, b.id)!.visibleEnd;
  d = E.addClip(d, c, bEnd - 16 * bar, 0);

  const tool = (clipId: string, type: string, at: number, bars: number) => {
    d = E.addProcess(d, clipId, E.newTool(type, at, bars * bar));
  };
  const aLen = E.clipOf(d, a.id)!.length;
  tool(a.id, "Reverb", aLen - 14 * bar, 2);
  tool(a.id, "FILTER", aLen - 8 * bar, 4);
  const bLen = E.clipOf(d, b.id)!.length;
  tool(b.id, "EQ", 0, 8);
  tool(b.id, "Echo", 40 * bar, 1);
  tool(b.id, "Crush", bLen - 12 * bar, 1);
  tool(b.id, "FILTER", bLen - 8 * bar, 4);
  d = E.addProcess(d, b.id, E.newRegion("loop", 24 * bar, bar));
  d = E.addProcess(d, c.id, E.newTool("Roll", cCue + 32 * bar, bar / 2));

  const ca = E.clipOf(d, a.id)!;
  const cb = E.clipOf(d, b.id)!;
  const cc = E.clipOf(d, c.id)!;
  d = E.addTransition(d, E.newTransition(ca.id, cb.id, cb.visibleStart, ca.visibleEnd - cb.visibleStart));
  d = E.addTransition(d, E.newTransition(cb.id, cc.id, cc.visibleStart, cb.visibleEnd - cc.visibleStart));
  d = E.updateClip(d, a.id, (x) => ({ ...x, gain: [{ pos: 0, value: 0 }, { pos: Math.round(x.length * 0.9), value: 0 }, { pos: Math.round(x.length) - 1, value: -4 }] }));
  return d;
}

function sync(doc: MixDoc) {
  const row = { id: doc.id, name: doc.name, play_time_secs: Math.floor(doc.length / SR), track_count: doc.clips.length };
  const i = mixes.findIndex((m) => m.id === doc.id);
  if (i >= 0) mixes[i] = row;
  else mixes.push(row);
}

{
  const ex = example();
  docs.set(1, ex);
  sync(ex);
}

const copy = <T>(x: T): T => structuredClone(x);

/** The part of mix_save's checks that is cheap to repeat in the browser. */
function problems(doc: MixDoc): string[] {
  const out: string[] = [];
  if (!doc.name.trim()) out.push("the mix name is empty");
  for (const [i, a] of doc.clips.entries()) {
    if (!tracks.some((t) => t.track_id === a.trackId)) out.push(`clip ${i + 1}: track ${a.trackId} is not in the library`);
    if (a.length <= a.trimStart + a.trimEnd) out.push(`clip ${i + 1}: nothing left to play`);
    for (const b of doc.clips.slice(i + 1)) {
      const lo = Math.max(a.visibleStart, b.visibleStart);
      const hi = Math.min(a.visibleEnd, b.visibleEnd);
      if (a.channel === b.channel && hi - lo > 1 && !doc.transitions.some((t) => Math.min(t.start + t.length, hi) - Math.max(t.start, lo) >= (hi - lo) / 2)) {
        out.push(`two clips overlap on channel ${a.channel} without a transition`);
      }
    }
  }
  return out;
}

function mockBeatmatch(doc: MixDoc, clipId: string): BeatmatchSuggestion {
  const b = E.clipOf(doc, clipId);
  if (!b) throw `no clip ${clipId}`;
  const prev = E.sortedClips(doc).filter((c) => c.id !== b.id && c.visibleStart < b.visibleStart).sort((x, y) => y.visibleEnd - x.visibleEnd)[0];
  if (!prev) throw "there is no earlier clip to match to";
  if (!b.track?.grid || !prev.track?.grid) throw `the track of clip ${b.track?.grid ? prev.id : b.id} has no beat grid (analyse it first)`;
  const target = E.effectiveBpm(prev, Math.min(Math.max(b.visibleStart - prev.start, prev.trimStart), prev.length - prev.trimEnd));
  const tempo = [{ pos: 0, speed: target / b.bpm }];
  const r = E.retempoClip(b, tempo);
  const beats = E.clipBeats(r);
  const refBeats = E.clipBeats(prev);
  const cue = r.start + r.trimStart;
  const anchor = E.nearestIndex(beats, cue);
  const ref = E.nearestIndex(refBeats, beats[anchor]);
  const shift = refBeats[ref] - beats[anchor];
  return {
    clipId, referenceClipId: prev.id, tempo, start: r.start + shift, length: r.length, trimStart: r.trimStart, trimEnd: r.trimEnd,
    targetBpm: target, speed: target / b.bpm, shift: r.start + shift + r.trimStart - b.visibleStart, anchorBeat: anchor, referenceBeat: ref,
    barAligned: false, overlaps: prev.visibleEnd > cue + shift, notes: ["Browser preview: beats aligned, bars not checked"],
  };
}

function mockTransition(doc: MixDoc, fromId: string, toId: string): TransitionSuggestion {
  const a = E.clipOf(doc, fromId);
  const b = E.clipOf(doc, toId);
  if (!a || !b || a.id === b.id) throw "a transition needs two different clips";
  const bpm = E.effectiveBpm(b, b.trimStart);
  const bar = ((60 / bpm) * 4) * SR;
  const t0 = E.clipBeats(b).find((t) => t >= b.visibleStart - 1) ?? b.visibleStart;
  const avail = a.visibleEnd - t0;
  if (avail < bar / 8) throw "the clips do not overlap (move the later clip earlier)";
  const bars = [16, 8, 4, 2, 1].find((n) => n * bar <= avail + 1) ?? 1;
  const length = Math.min(bars * bar, avail);
  const t = E.newTransition(a.id, b.id, t0, length);
  const mk = (clip: Clip, after: boolean) => {
    const tool = E.newTool("EQ", t0 - clip.start, length);
    const last = Math.round(length) - 1;
    const half = Math.round(length / 2);
    const lo = after
      ? [{ pos: 0, value: 0 }, { pos: half - 20000, value: 0 }, { pos: half, value: -100 }, { pos: last, value: -100 }]
      : [{ pos: 0, value: -100 }, { pos: half, value: -100 }, { pos: half + 20000, value: 0 }, { pos: last, value: 0 }];
    tool.params["EQ-Lo"] = lo;
    return { clipId: clip.id, tool, conflicts: [] as string[] };
  };
  return { transition: t, tools: [mk(a, true), mk(b, false)], bars: length / bar, notes: ["Browser preview"] };
}

export const handlers: Handlers = {
  mix_list: () => mixes.map((m) => ({ ...m })),
  mix_open: (a) => {
    const d = docs.get(a.id);
    if (!d) throw `there is no mix ${a.id}`;
    return copy(d);
  },
  mix_new: (a) => {
    const name = String(a.name ?? "").trim();
    if (!name) throw "mix name is empty";
    if (mixes.some((m) => m.name === name)) throw `a mix called "${name}" already exists`;
    const d = emptyDoc(nextId++, name);
    docs.set(d.id, d);
    sync(d);
    return d.id;
  },
  mix_save: (a) => {
    const doc = a.doc as MixDoc;
    if (doc.id !== a.id) throw `this document belongs to mix ${doc.id}, not mix ${a.id}`;
    const p = problems(doc);
    if (p.length) throw `The mix cannot be saved: ${p.join("; ")}`;
    if (mixes.some((m) => m.name === doc.name && m.id !== doc.id)) throw `a mix called "${doc.name}" already exists`;
    const saved = E.refresh({ ...copy(doc), meta: doc.meta ? { ...doc.meta, dateModified: Date.now(), playTimeSecs: Math.floor(doc.length / SR) } : null });
    docs.set(doc.id, saved);
    sync(saved);
    return { id: doc.id, name: doc.name, playTimeSecs: Math.floor(saved.length / SR), trackCount: saved.clips.length, dateModified: Date.now() };
  },
  mix_validate: (a) => {
    const doc = a.doc as MixDoc;
    const p = problems(doc);
    if (mixes.some((m) => m.name === doc.name && m.id !== doc.id)) p.push(`a mix called "${doc.name}" already exists`);
    return p;
  },
  mix_rename: (a) => {
    const d = docs.get(a.id);
    if (!d) throw `no mix ${a.id}`;
    if (!String(a.name).trim()) throw "mix name is empty";
    if (mixes.some((m) => m.name === a.name && m.id !== a.id)) throw `a mix called "${a.name}" already exists`;
    d.name = a.name;
    sync(d);
    return null;
  },
  mix_delete: (a) => {
    const had = docs.delete(a.id);
    const i = mixes.findIndex((m) => m.id === a.id);
    if (i >= 0) mixes.splice(i, 1);
    return had;
  },
  mix_set_meta: (a) => {
    const d = docs.get(a.id);
    if (!d) throw `no mix ${a.id}`;
    const m = a.meta as MixMeta;
    if (m.rating < 0 || m.rating > 5) throw `mix rating ${m.rating} is outside 0..=5`;
    d.meta = { ...(d.meta ?? emptyMeta()), genre: m.genre, year: m.year, rating: m.rating, producer: m.producer, remixer: m.remixer, comments: m.comments };
    return null;
  },
  mix_import_pmk: () => {
    throw "Importing a .pmk needs the desktop app (browser preview)";
  },
  mix_export_pmk: () => null,
  mix_track_info: (a) => (a.trackIds as number[]).map(trackInfo),
  mix_make_clip: (a) => makeClip(a.trackId, a.tempoBpm ?? null),
  mix_beatmatch: (a) => mockBeatmatch(a.doc as MixDoc, a.clipId),
  mix_auto_transition: (a) => mockTransition(a.doc as MixDoc, a.fromClipId, a.toClipId),
};
