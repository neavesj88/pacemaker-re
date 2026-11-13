// Mock of src-tauri/src/commands/grid/vtempo.rs: tempo sections, single-beat edits, drift and the
// onsets of the audio. The maths follows crates/pm-formats/src/ops_vtempo.rs closely enough to
// review the UI in a browser (the real code is the one with the tests); the audio is invented:
// the onsets are the original synthetic grid of the track, and every second track slows down
// after two thirds of its length so there is drift to find. Picked up automatically by index.ts.
import { demoGrid, SR, trackOf, type Doc } from "./grid";
import type { Handlers } from "./types";

type Beat = { pos: number; bpm: number };
type Onset = { frame: number; strength: number };

const MIN_BPM = 20;
const MAX_BPM = 400;
const gapLimits = (sr: number): [number, number] => [Math.ceil((sr * 60) / MAX_BPM), Math.floor((sr * 60) / MIN_BPM)];
const invalid = (m: string) => `invalid: ${m}`;

function finish(old: Doc, beats: Beat[], fd: number | null): Doc {
  if (beats.length === 0) throw invalid("the edit would leave the grid empty");
  for (let i = 1; i < beats.length; i++) if (beats[i].pos <= beats[i - 1].pos) throw invalid(`beat ${i} is not after the beat before it`);
  return {
    normalize_factor: old.normalize_factor,
    beat_start_offset: old.beat_start_offset + (beats[0].pos - old.beats[0].pos),
    first_downbeat: fd !== null && fd < beats.length ? fd : null,
    beats,
  };
}

function relabel(beats: Beat[], lo: number, hi: number, sr: number) {
  const n = beats.length;
  if (n < 2) return;
  for (let i = lo; i <= Math.min(hi, n - 1); i++) {
    const gap = i + 1 < n ? beats[i + 1].pos - beats[i].pos : beats[i].pos - beats[i - 1].pos;
    if (gap > 0) beats[i].bpm = (60 * sr) / gap;
  }
}

function checkGaps(beats: Beat[], lo: number, hi: number, sr: number) {
  const [min, max] = gapLimits(sr);
  for (let i = lo; i <= hi && i + 1 < beats.length; i++) {
    const gap = beats[i + 1].pos - beats[i].pos;
    if (gap < min) throw invalid(`beats ${i + 1} and ${i + 2} would be ${gap} frames apart, closer than ${MAX_BPM} BPM allows`);
    if (gap > max) throw invalid(`beats ${i + 1} and ${i + 2} would be further apart than ${MIN_BPM} BPM allows`);
  }
}

export function sections(doc: Doc, sr: number, o: { tolerance_pct: number; min_beats: number }, splits: number[]) {
  const b = doc.beats;
  const n = b.length;
  if (n === 0) return [];
  const tol = Math.max(0, o.tolerance_pct) / 100;
  let starts = [0];
  let sum = 0;
  let cnt = 0;
  for (let i = 0; i < n - 1; i++) {
    const gap = b[i + 1].pos - b[i].pos;
    if (cnt > 0 && Math.abs(gap - sum / cnt) / (sum / cnt) > tol) {
      starts.push(i);
      sum = 0;
      cnt = 0;
    }
    sum += gap;
    cnt++;
  }
  const forced = new Set<number>();
  for (const s of splits) {
    let k = 0;
    b.forEach((x, i) => {
      if (Math.abs(x.pos - s) < Math.abs(b[k].pos - s)) k = i;
    });
    const gap = k + 1 < n ? b[k + 1].pos - b[k].pos : k > 0 ? b[k].pos - b[k - 1].pos : 0;
    if (k > 0 && Math.abs(b[k].pos - s) * 2 <= Math.max(1, gap)) {
      forced.add(k);
      if (!starts.includes(k)) starts.push(k);
    }
  }
  starts = [...new Set(starts)].sort((x, y) => x - y);
  const meanGap = (first: number, end: number) => {
    const last = Math.min(end, n - 1);
    return last <= first ? NaN : (b[last].pos - b[first].pos) / (last - first);
  };
  for (;;) {
    let changed = false;
    for (let k = 0; k < starts.length && !changed; k++) {
      const end = starts[k + 1] ?? n;
      if (end - starts[k] >= o.min_beats || starts.length < 2) continue;
      if (k > 0 && !forced.has(starts[k])) {
        starts.splice(k, 1);
        changed = true;
      } else if (k + 1 < starts.length && !forced.has(starts[k + 1])) {
        starts.splice(k + 1, 1);
        changed = true;
      }
    }
    for (let k = 1; k < starts.length && !changed; k++) {
      if (forced.has(starts[k])) continue;
      const a = meanGap(starts[k - 1], starts[k]);
      const c = meanGap(starts[k], starts[k + 1] ?? n);
      if (Number.isFinite(a) && Number.isFinite(c) && Math.abs(a - c) / a <= tol) {
        starts.splice(k, 1);
        changed = true;
      }
    }
    if (!changed) break;
  }
  return starts.map((first, k) => {
    const end = starts[k + 1] ?? n;
    const gaps: number[] = [];
    for (let i = first; i < Math.min(end, n - 1); i++) gaps.push(b[i + 1].pos - b[i].pos);
    const tempo = (g: number) => (60 * sr) / g;
    const total = gaps.reduce((x, y) => x + y, 0);
    return {
      first_beat: first,
      end_beat: end,
      start_frame: b[first].pos,
      end_frame: b[Math.min(end, n - 1)].pos,
      beats: end - first,
      bpm: gaps.length ? tempo(total / gaps.length) : b[first].bpm,
      min_bpm: gaps.length ? tempo(Math.max(...gaps)) : b[first].bpm,
      max_bpm: gaps.length ? tempo(Math.min(...gaps)) : b[first].bpm,
      forced: forced.has(first),
    };
  });
}

type DriftOptions = { window_pct: number; window_max_ms: number; threshold_ms: number; min_run: number; max_gap?: number };

export function drift(doc: Doc, onsets: Onset[], sr: number, o: DriftOptions) {
  if (!(o.window_pct >= 1 && o.window_pct <= 50)) throw invalid("the match window must be between 1 and 50 % of a beat");
  if (!(o.window_max_ms >= 2)) throw invalid("the match window must be at least 2 ms");
  if (!(o.threshold_ms > 0)) throw invalid("the drift threshold must be above 0 ms");
  if (o.min_run < 1) throw invalid("a drift span needs at least 1 beat");
  const b = doc.beats;
  const n = b.length;
  const ms = 1000 / sr;
  const offsets: (number | null)[] = b.map((beat, i) => {
    const gap = i + 1 < n ? b[i + 1].pos - beat.pos : i > 0 ? beat.pos - b[i - 1].pos : (sr * 60) / beat.bpm;
    const window = Math.max(1, Math.min((gap * o.window_pct) / 100, o.window_max_ms / ms));
    let best: [number, number] | null = null;
    for (const on of onsets) {
      if (on.frame < beat.pos - window || on.frame > beat.pos + window) continue;
      const score = on.strength * (1 - (0.5 * Math.abs(on.frame - beat.pos)) / window);
      if (!best || score > best[0]) best = [score, on.frame];
    }
    return best ? (best[1] - beat.pos) * ms : null;
  });
  const vals = offsets.filter((v): v is number => v !== null).sort((x, y) => x - y);
  const bias = vals.length ? vals[Math.floor(vals.length / 2)] : 0;
  const worst = vals.reduce((m, v) => Math.max(m, Math.abs(v - bias)), 0);
  type Run = { first: number; last: number; sign: number; count: number; sum: number; max: number };
  const spans: any[] = [];
  let run: Run | null = null;
  const close = () => {
    if (run && run.count >= o.min_run) {
      spans.push({
        first_beat: run.first,
        last_beat: run.last,
        start_frame: b[run.first].pos,
        end_frame: b[run.last].pos,
        off_beats: run.count,
        mean_ms: run.sum / run.count,
        max_abs_ms: run.max,
      });
    }
    run = null;
  };
  offsets.forEach((v, i) => {
    if (v === null) {
      if (run && i - (run as Run).last > (o.max_gap ?? 2)) close();
      return;
    }
    const d = v - bias;
    if (Math.abs(d) > o.threshold_ms) {
      const sign = Math.sign(d);
      if (run && (run as Run).sign !== sign) close();
      run ??= { first: i, last: i, sign, count: 0, sum: 0, max: 0 };
      run.last = i;
      run.count++;
      run.sum += d;
      run.max = Math.max(run.max, Math.abs(d));
    } else close();
  });
  close();
  return { offsets_ms: offsets, bias_ms: bias, matched: vals.length, beats: n, worst_ms: worst, spans };
}

export function applyTempoEdit(doc: Doc, edit: any, sr: number, total: number | null): Doc {
  const n = doc.beats.length;
  const [minGap] = gapLimits(sr);
  const period = (bpm: number) => (sr * 60) / bpm;
  const normFd = (fd: number | null) => (fd === null ? null : fd % 4);
  switch (edit.kind) {
    case "set_section_bpm": {
      const from: number = edit.from_beat;
      const bpm: number = edit.bpm;
      if (!Number.isFinite(bpm) || bpm < MIN_BPM || bpm > MAX_BPM) throw invalid(`tempo must be between ${MIN_BPM} and ${MAX_BPM} BPM`);
      if (from >= n) throw invalid(`there is no beat ${from + 1}`);
      const to: number | null = edit.to_beat;
      if (to !== null && (to <= from || to >= n)) throw invalid("the section must end at a later beat");
      const start = doc.beats[from].pos;
      const p = period(bpm);
      const count =
        to !== null
          ? Math.max(1, Math.round((doc.beats[to].pos - start) / p))
          : Math.floor((Math.max(total ?? doc.beats[n - 1].pos, start) - start) / p) + 1;
      const mid: Beat[] = Array.from({ length: count }, (_, j) => ({ pos: start + Math.round(j * p), bpm }));
      const tailFrom = to ?? n;
      if (to !== null) {
        // the last generated beat may sit closer to the next section than the device allows: drop it
        while (mid.length > 1 && doc.beats[to].pos - mid[mid.length - 1].pos < minGap) mid.pop();
      }
      const beats = [...doc.beats.slice(0, from), ...mid, ...doc.beats.slice(tailFrom)];
      if (to !== null) {
        checkGaps(beats, from + mid.length - 1, from + mid.length - 1, sr);
        relabel(beats, from + mid.length - 1, from + mid.length - 1, sr);
      }
      const fd = doc.first_downbeat === null ? null : doc.first_downbeat >= tailFrom ? (doc.first_downbeat + mid.length - (tailFrom - from)) % 4 : doc.first_downbeat;
      return finish(doc, beats, fd);
    }
    case "move_beat": {
      const b: number = edit.beat;
      if (b >= n) throw invalid(`there is no beat ${b + 1}`);
      if (edit.frame < 0) throw invalid("a beat cannot be before the start of the track");
      if (b + 1 === n && total !== null && edit.frame > total) throw invalid("a beat cannot be after the end of the audio");
      const beats = doc.beats.map((x) => ({ ...x }));
      beats[b].pos = edit.frame;
      checkGaps(beats, Math.max(0, b - 1), b, sr);
      relabel(beats, Math.max(0, b - 1), b + 1, sr);
      return finish(doc, beats, doc.first_downbeat);
    }
    case "shift_from": {
      const b: number = edit.beat;
      if (b >= n) throw invalid(`there is no beat ${b + 1}`);
      if (edit.frames === 0) throw invalid("nothing to move");
      let beats = doc.beats.map((x, i) => (i >= b ? { ...x, pos: x.pos + edit.frames } : { ...x }));
      if (b === 0) {
        let dropped = 0;
        while (dropped < beats.length && beats[dropped].pos < 0) dropped++;
        beats = beats.slice(dropped);
        if (total !== null) beats = beats.filter((x) => x.pos <= total);
        const fd = doc.first_downbeat === null ? null : doc.first_downbeat >= dropped ? doc.first_downbeat - dropped : (((doc.first_downbeat - dropped) % 4) + 4) % 4;
        return finish(doc, beats, fd);
      }
      if (total !== null) beats = beats.filter((x) => x.pos <= total);
      if (beats.length <= b) throw invalid("the move would push every later beat past the end of the audio");
      checkGaps(beats, b - 1, b - 1, sr);
      relabel(beats, b - 1, b, sr);
      return finish(doc, beats, doc.first_downbeat !== null && doc.first_downbeat >= beats.length ? doc.first_downbeat % 4 : doc.first_downbeat);
    }
    case "even": {
      const { from_beat: from, to_beat: to } = edit;
      if (to >= n || from >= to || to - from < 2) throw invalid("choose two beats with at least one beat between them");
      const span = doc.beats[to].pos - doc.beats[from].pos;
      const steps = to - from;
      const bpm = (60 * sr * steps) / span;
      const beats = doc.beats.map((x) => ({ ...x }));
      for (let k = 0; from + k < to; k++) beats[from + k] = { pos: doc.beats[from].pos + Math.floor((k * span + Math.floor(steps / 2)) / steps), bpm };
      checkGaps(beats, from, to - 1, sr);
      return finish(doc, beats, doc.first_downbeat);
    }
    case "insert_beat": {
      const frame: number = edit.frame;
      if (frame < 0) throw invalid("a beat cannot be before the start of the track");
      if (total !== null && frame > total) throw invalid("a beat cannot be after the end of the audio");
      let i = doc.beats.findIndex((x) => x.pos >= frame);
      if (i < 0) i = n;
      if (doc.beats[i]?.pos === frame) throw invalid("there is already a beat there");
      const beats = doc.beats.map((x) => ({ ...x }));
      beats.splice(i, 0, { pos: frame, bpm: doc.beats[Math.min(i, n - 1)].bpm });
      checkGaps(beats, Math.max(0, i - 1), i, sr);
      relabel(beats, Math.max(0, i - 1), i + 1, sr);
      return finish(doc, beats, normFd(doc.first_downbeat === null ? null : i <= doc.first_downbeat ? doc.first_downbeat + 1 : doc.first_downbeat));
    }
    case "remove_beat": {
      const b: number = edit.beat;
      if (b >= n) throw invalid(`there is no beat ${b + 1}`);
      if (n < 2) throw invalid("the last beat cannot be removed");
      const beats = doc.beats.map((x) => ({ ...x }));
      beats.splice(b, 1);
      if (b > 0 && b < n - 1) checkGaps(beats, b - 1, b - 1, sr);
      relabel(beats, Math.max(0, b - 1), Math.min(Math.max(b, 1), beats.length - 1), sr);
      return finish(doc, beats, normFd(doc.first_downbeat === null ? null : b < doc.first_downbeat ? doc.first_downbeat - 1 : doc.first_downbeat));
    }
    case "follow_onsets": {
      const from: number = edit.from_beat;
      const to: number = edit.to_beat ?? n;
      if (from >= n || to > n || to <= from) throw invalid("choose a range of beats");
      const rep = drift(doc, edit.onsets, sr, edit.options);
      const win = Math.max(1, edit.smooth_beats) | 1;
      const raw = rep.offsets_ms.map((v) => (v === null ? null : v - rep.bias_ms));
      const corr: (number | null)[] = [];
      for (let i = from; i < to; i++) {
        const v = raw.slice(Math.max(0, i - (win >> 1)), Math.min(n, i + (win >> 1) + 1)).filter((x): x is number => x !== null).sort((x, y) => x - y);
        corr.push(v.length ? v[Math.floor(v.length / 2)] : null);
      }
      if (corr.every((c) => c === null)) throw invalid("no onsets lie near the beat lines of this range, so there is nothing to follow");
      const known = corr.map((c, k) => (c === null ? null : ([k, c] as const))).filter((x) => x !== null) as (readonly [number, number])[];
      const fill = corr.map((c, k) => {
        if (c !== null) return c;
        const before = [...known].reverse().find(([j]) => j < k);
        const after = known.find(([j]) => j > k);
        if (before && after) return before[1] + ((after[1] - before[1]) * (k - before[0])) / (after[0] - before[0]);
        return (before ?? after)?.[1] ?? 0;
      });
      let beats = doc.beats.map((x) => ({ ...x }));
      for (let i = from; i < to; i++) beats[i].pos = Math.max(0, beats[i].pos + Math.round((fill[i - from] / 1000) * sr));
      for (let i = 1; i < n; i++) if (beats[i].pos < beats[i - 1].pos + minGap) beats[i].pos = beats[i - 1].pos + minGap;
      if (total !== null) beats = beats.filter((x) => x.pos <= total);
      if (beats.length < 2) throw invalid("following the onsets would leave the grid empty");
      relabel(beats, Math.max(0, from - 1), to, sr);
      return finish(doc, beats, doc.first_downbeat !== null && doc.first_downbeat >= beats.length ? doc.first_downbeat % 4 : doc.first_downbeat);
    }
  }
  throw invalid("unknown edit");
}

/** The invented audio of a demo track: its original grid, slowing down after two thirds on every
 *  second track (1.5 ms later each beat), with a quieter off-beat hat between the kicks. */
function demoOnsets(id: number): Onset[] {
  const g = demoGrid(id);
  const from = Math.floor(g.beats.length * 0.66);
  const out: Onset[] = [];
  g.beats.forEach((b, i) => {
    const late = id % 2 === 0 && i >= from ? Math.round(((i - from + 1) * 1.5 * SR) / 1000) : 0;
    out.push({ frame: b.pos + late, strength: 6 });
    const next = g.beats[i + 1];
    if (next) out.push({ frame: Math.round((b.pos + next.pos) / 2) + late, strength: 2.2 });
  });
  return out;
}

export const handlers: Handlers = {
  grid_vtempo_edit: (a) => applyTempoEdit(a.doc, a.edit, a.sampleRate, a.totalFrames ?? null),
  grid_sections: (a) => {
    if (!(a.options.tolerance_pct >= 0)) throw "The section tolerance must be 0 or more.";
    return sections(a.doc, a.sampleRate, a.options, a.splits);
  },
  grid_drift: (a) => drift(a.doc, a.onsets, a.sampleRate, a.options),
  grid_onsets: (a) => {
    trackOf(a.trackId);
    // the decode takes a moment on a real file
    return new Promise((resolve) => setTimeout(() => resolve({ track_id: a.trackId, sample_rate: SR, onsets: demoOnsets(a.trackId).filter((o) => o.strength >= (a.minStrength ?? 1)) }), 700));
  },
};
