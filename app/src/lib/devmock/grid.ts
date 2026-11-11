// Mock of src-tauri/src/commands/grid.rs. Synthetic constant grids per demo track; edits mirror
// the Rust math in crates/pm-formats/src/ops_grid.rs closely enough to review the UI, and saves
// are kept in memory only. Picked up automatically by devmock/index.ts.
import { emit } from "@tauri-apps/api/event";
import { needs, tracks } from "./db";
import { jobLock } from "./jobs";
import { mixNamesUsing } from "./mixes";
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
  return constantGrid(t.bpm, 3000 + ((id * 7919) % 20000), t.play_time_secs * SR, id % 4);
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

type PlanRow = { track_id: number; label: string; before: number | null; after: number | null; skip: string | null };

/** Median local tempo of a grid: what the real backend stores in `tracks.bpm`. */
function medianBpm(doc: Doc): number {
  const v = doc.beats.map((b) => b.bpm).sort((a, b) => a - b);
  return v[Math.floor(v.length / 2)] ?? 0;
}

/** Mirror of commands/grid/tempo.rs `plan`: the preview and the job share it here too. */
function tempoPlan(ids: number[], op: any): (PlanRow & { doc: Doc | null })[] {
  if (op.kind === "set" && !(Number.isFinite(op.bpm) && op.bpm >= 20 && op.bpm <= 400)) throw "The tempo must be between 20 and 400 BPM.";
  const out: (PlanRow & { doc: Doc | null })[] = [];
  for (const id of new Set(ids)) {
    const t = tracks.find((x) => x.track_id === id);
    const skip = (label: string, before: number | null, why: string) => out.push({ track_id: id, label, before, after: null, skip: why, doc: null });
    if (!t) {
      skip(`track ${id}`, null, "not in the library");
      continue;
    }
    const label = `${t.artist} - ${t.title}`;
    const reason = needs.get(id);
    if (reason === "no .str file" || reason?.startsWith("empty")) {
      skip(label, null, `${reason}; re-analyse it first`);
      continue;
    }
    const doc = saved.get(id) ?? demoGrid(id);
    const before = medianBpm(doc);
    let next: Doc;
    try {
      next = applyEdit(doc, op.kind === "set" ? { kind: "set_bpm", bpm: op.bpm } : { kind: op.kind }, SR, (t.play_time_secs + 1) * SR);
    } catch (e) {
      skip(label, before, String(e).replace(/^invalid: /, ""));
      continue;
    }
    if (JSON.stringify(next) === JSON.stringify(doc)) skip(label, before, "the grid is already like that");
    else out.push({ track_id: id, label, before, after: medianBpm(next), skip: null, doc: next });
  }
  return out;
}

export const handlers: Handlers = {
  grid_tempo_plan: (a) => tempoPlan(a.trackIds, a.op).map(({ doc: _doc, ...row }) => row),
  // like the real write job: re-plans with the same code, one progress event per track, rows and
  // grids change together (the row's bpm is the new grid's median), skipped tracks are reported
  grid_tempo_apply: (a) => {
    if (jobLock.running) throw "Another job is still running.";
    if (a.trackIds.length === 0) throw "No tracks to change.";
    const planned = tempoPlan(a.trackIds, a.op);
    jobLock.running = true;
    let ok = 0;
    let skipped = 0;
    planned.forEach((p, i) =>
      setTimeout(async () => {
        let detail: string | null = null;
        if (p.doc) {
          saved.set(p.track_id, p.doc);
          const t = trackOf(p.track_id);
          t.bpm = p.after!;
          ok++;
        } else {
          detail = p.skip;
          skipped++;
        }
        await emit("job-progress", {
          job: "tempo-edit",
          label: p.label,
          done: i + 1,
          total: planned.length,
          status: p.doc ? "retimed" : "skipped",
          bpm: p.after,
          detail,
          error: null,
        });
        if (i === planned.length - 1) {
          jobLock.running = false;
          const parts = [ok && `${ok} track${ok === 1 ? "" : "s"} retimed`, skipped && `${skipped} skipped`].filter(Boolean);
          await emit("job-done", { job: "tempo-edit", ok, failed: 0, summary: parts.join(", ") || "nothing to do", error: null });
        }
      }, 500 * (i + 1)),
    );
    return null;
  },
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
      total_frames: (t.play_time_secs + 1) * SR, // the stored length is truncated: one second of slack
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
  grid_mixes_using: (a) => mixNamesUsing([a.trackId]),
  grid_save: (a) => {
    if (a.doc) saved.set(a.trackId, a.doc);
    if (a.cue) cues.set(a.trackId, a.cue.frames);
    return { wrote_grid: !!a.doc, wrote_cue: !!a.cue };
  },
};
