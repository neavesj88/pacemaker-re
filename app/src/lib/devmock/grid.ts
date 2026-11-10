// Mock of src-tauri/src/commands/grid.rs. Synthetic constant grids per demo track; edits mirror
// the Rust math in crates/pm-formats/src/ops_grid.rs closely enough to review the UI, and saves
// are kept in memory only. Picked up automatically by devmock/index.ts.
import { needs, tracks } from "./db";
import type { Handlers } from "./types";

type Beat = { pos: number; bpm: number };
type Doc = { normalize_factor: number; beat_start_offset: number; beats: Beat[]; first_downbeat: number | null };

const SR = 44100;
const saved = new Map<number, Doc>();
const cues = new Map<number, number | null>();

function constantGrid(bpm: number, first: number, total: number, fd: number | null): Doc {
  const period = (SR * 60) / bpm;
  const n = Math.floor((total - first) / period) + 1;
  const beats = Array.from({ length: n }, (_, i) => ({ pos: first + Math.round(i * period), bpm }));
  return { normalize_factor: 1, beat_start_offset: first, beats, first_downbeat: fd };
}

const trackOf = (id: number) => {
  const t = tracks.find((x) => x.track_id === id);
  if (!t) throw `no track ${id}`;
  return t;
};

function demoGrid(id: number): Doc {
  const t = trackOf(id);
  return constantGrid(Math.round(t.bpm), 3000 + ((id * 7919) % 20000), t.play_time_secs * SR, id % 4);
}

const mod4 = (n: number) => ((n % 4) + 4) % 4;

function clip(beats: Beat[], fd: number | null, total: number | null): [Beat[], number | null] {
  let dropped = 0;
  while (dropped < beats.length && beats[dropped].pos < 0) dropped++;
  let out = beats.slice(dropped);
  if (total !== null) out = out.filter((b) => b.pos <= total);
  return [out, fd === null ? null : fd >= dropped ? fd - dropped : mod4(fd - dropped)];
}

function finish(old: Doc, beats: Beat[], fd: number | null): Doc {
  if (beats.length === 0) throw "invalid: the edit would leave the grid empty";
  return {
    normalize_factor: old.normalize_factor,
    beat_start_offset: old.beat_start_offset + (beats[0].pos - old.beats[0].pos),
    first_downbeat: fd !== null && fd < beats.length ? fd : null,
    beats,
  };
}

function applyEdit(doc: Doc, edit: any, sr: number, total: number | null): Doc {
  const period = (bpm: number) => (sr * 60) / bpm;
  switch (edit.kind) {
    case "nudge": {
      const [b, fd] = clip(
        doc.beats.map((x) => ({ ...x, pos: x.pos + edit.frames })),
        doc.first_downbeat,
        total,
      );
      return finish(doc, b, fd);
    }
    case "set_downbeat": {
      let k = 0;
      doc.beats.forEach((b, i) => {
        if (Math.abs(b.pos - edit.frame) < Math.abs(doc.beats[k].pos - edit.frame)) k = i;
      });
      return finish(doc, doc.beats, k % 4);
    }
    case "halve": {
      if (doc.beats.length < 2) throw "invalid: a one-beat grid cannot be halved";
      const fd = doc.first_downbeat;
      const parity = (fd ?? 0) % 2;
      const beats = doc.beats.filter((_, i) => i >= parity && (i - parity) % 2 === 0).map((b) => ({ pos: b.pos, bpm: b.bpm / 2 }));
      if (beats[0].bpm < 20) throw `invalid: ${beats[0].bpm.toFixed(2)} BPM is below the minimum of 20`;
      return finish(doc, beats, fd === null ? null : Math.floor(fd / 2) % 4);
    }
    case "double": {
      if (doc.beats.length < 2) throw "invalid: a one-beat grid cannot be doubled";
      const beats: Beat[] = [];
      doc.beats.forEach((b, i) => {
        const bpm = b.bpm * 2;
        if (bpm > 400) throw `invalid: ${bpm.toFixed(2)} BPM is above the maximum of 400`;
        beats.push({ pos: b.pos, bpm });
        const gap = i + 1 < doc.beats.length ? doc.beats[i + 1].pos - b.pos : b.pos - doc.beats[i - 1].pos;
        const mid = b.pos + Math.trunc(gap / 2);
        if (mid > b.pos && (total === null || mid <= total)) beats.push({ pos: mid, bpm });
      });
      return finish(doc, beats, doc.first_downbeat === null ? null : (doc.first_downbeat * 2) % 4);
    }
    case "set_bpm": {
      if (!Number.isFinite(edit.bpm) || edit.bpm < 20 || edit.bpm > 400) throw "invalid: tempo must be between 20 and 400 BPM";
      const first = doc.beats[0].pos;
      const end = Math.max(first, total ?? doc.beats[doc.beats.length - 1].pos);
      const p = period(edit.bpm);
      const n = Math.floor((end - first) / p) + 1;
      return finish(
        doc,
        Array.from({ length: n }, (_, i) => ({ pos: first + Math.round(i * p), bpm: edit.bpm })),
        doc.first_downbeat,
      );
    }
    case "shift_beats": {
      const n = edit.beats as number;
      if (n === 0 || Math.abs(n) > 64) throw "invalid: shift by 1 to 64 beats";
      const delta = Math.round(n * period(doc.beats[0].bpm));
      let [beats, fd] = clip(
        doc.beats.map((x) => ({ ...x, pos: x.pos + delta })),
        doc.first_downbeat,
        total,
      );
      if (n > 0 && beats.length > 0) {
        const front: Beat[] = [];
        let head = beats[0];
        for (;;) {
          const p = head.pos - Math.round(period(head.bpm));
          if (p < 0) break;
          head = { pos: p, bpm: head.bpm };
          front.unshift(head);
        }
        if (front.length) {
          beats = [...front, ...beats];
          fd = fd === null ? null : (fd + front.length) % 4;
        }
      }
      return finish(doc, beats, fd);
    }
  }
  throw "invalid: unknown edit";
}

export const handlers: Handlers = {
  grid_load: (a) => {
    const t = trackOf(a.trackId);
    const reason = needs.get(a.trackId);
    const broken = reason === "no .str file" || (reason?.startsWith("empty") ?? false);
    const cue = cues.has(a.trackId) ? cues.get(a.trackId)! : null;
    return {
      track_id: t.track_id,
      location: t.location,
      title: t.title,
      artist: t.artist,
      sample_rate: SR,
      duration_secs: t.play_time_secs,
      total_frames: t.play_time_secs * SR,
      cue_frames: cue,
      doc: broken ? null : (saved.get(a.trackId) ?? demoGrid(a.trackId)),
      problem: broken ? reason : null,
    };
  },
  grid_edit: (a) => applyEdit(a.doc, a.edit, a.sampleRate, a.totalFrames ?? null),
  // a different, plausible answer from the "analyser": same tempo, first beat a few ms later
  grid_analyse: (a) => {
    const t = trackOf(a.trackId);
    return constantGrid(t.bpm, 4200 + ((a.trackId * 104729) % 9000), t.play_time_secs * SR, 0);
  },
  grid_save: (a) => {
    if (a.doc) saved.set(a.trackId, a.doc);
    if (a.cue) cues.set(a.trackId, a.cue.frames);
    return { wrote_grid: !!a.doc, wrote_cue: !!a.cue };
  },
};
