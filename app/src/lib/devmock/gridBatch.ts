// Mock of src-tauri/src/commands/grid/suspect.rs and batch.rs: the suspect-grid scan and the batch
// grid operations (nudge, halve, double, suggested fix, re-analyse). The grids are the synthetic ones
// of devmock/grid.ts; the rules and the preview-is-the-write behaviour follow the Rust code, the
// numbers (confidence, analysis result) are invented. Picked up automatically by devmock/index.ts.
import { emit } from "@tauri-apps/api/event";
import { needs, tracks } from "./db";
import { applyEdit, constantGrid, demoGrid, medianBpm, saved, SR, trackOf, type Doc } from "./grid";
import { jobLock } from "./jobs";
import { mixNamesUsing } from "./mixes";
import type { Handlers } from "./types";

// a few fixture tracks get a tempo that is half or double their genre's, so the suspect rules, the
// suggested fix and the preview have something to show (the shared fixture has none)
for (const [id, bpm] of [
  [3, 86.5],
  [5, 277],
  [8, 63.25],
] as const) {
  const t = tracks.find((x) => x.track_id === id);
  if (t) t.bpm = bpm;
}

type Rules = {
  tempo_rule: boolean;
  fallback_min: number;
  fallback_max: number;
  genres: { name: string; min: number; max: number }[];
  quality_rule: boolean;
  min_confidence_pct: number;
};

function windowFor(genre: string, r: Rules): [number, number, string | null] {
  const g = genre.toLowerCase();
  const hit = r.genres
    .filter((w) => w.name.trim() && g.includes(w.name.trim().toLowerCase()))
    .sort((a, b) => b.name.trim().length - a.name.trim().length)[0];
  return hit ? [hit.min, hit.max, hit.name.trim()] : [r.fallback_min, r.fallback_max, null];
}

/** Mock of `suspect::assess`, worded the same way. The confidence is invented: every ninth track has
 *  a wandering grid. */
function assess(id: number, r: Rules) {
  const t = trackOf(id);
  const bpm = medianBpm(saved.get(id) ?? demoGrid(id));
  const [lo, hi, named] = windowFor(t.genre, r);
  const reasons: string[] = [];
  let fix: "halve" | "double" | null = null;
  let tempoFlag = false;
  if (r.tempo_rule && bpm > 0 && (bpm < lo || bpm > hi)) {
    tempoFlag = true;
    const which = named ? `the ${lo}–${hi} BPM window for “${named}”` : `the general ${lo}–${hi} BPM window`;
    let why = `${bpm.toFixed(2)} BPM is ${bpm > hi ? "above" : "below"} ${which}`;
    const fits = (v: number) => v >= lo && v <= hi && v >= 20 && v <= 400;
    if (bpm > hi && fits(bpm / 2)) {
      fix = "halve";
      why += `; half of it, ${(bpm / 2).toFixed(2)} BPM, fits`;
    } else if (bpm < lo && fits(bpm * 2)) {
      fix = "double";
      why += `; double it, ${(bpm * 2).toFixed(2)} BPM, fits`;
    } else {
      why += "; neither half nor double fits";
    }
    reasons.push(why);
  }
  const confidence = id % 9 === 0 ? 46.8 : 100;
  let qualityFlag = false;
  if (r.quality_rule && confidence < r.min_confidence_pct) {
    qualityFlag = true;
    reasons.push(`grid confidence ${confidence.toFixed(0)}% is below ${r.min_confidence_pct}%: only 52% of the gaps between beat lines are steady (within 4%)`);
  }
  if (!tempoFlag && !qualityFlag) return null;
  return { track_id: id, bpm, reasons, tempo_flag: tempoFlag, quality_flag: qualityFlag, fix, window: [lo, hi], confidence_pct: confidence };
}

const hasGrid = (id: number) => {
  const reason = needs.get(id);
  return !(reason === "no .str file" || reason?.startsWith("empty"));
};

type Row = {
  track_id: number;
  label: string;
  op: string;
  before_bpm: number | null;
  after_bpm: number | null;
  before_first: number | null;
  after_first: number | null;
  before_beats: number;
  after_beats: number;
  sample_rate: number;
  skip: string | null;
  note: string | null;
  mixes: { id: number; name: string }[];
};
type Plan = { plan_id: number; rows: Row[]; docs: Map<number, { before: Doc | null; after: Doc }> };

let planSeq = 0;
let current: Plan | null = null;
let cancelled = false;

// a stable id per mix name, like the ids the real rows carry
const mixRefs = (id: number) =>
  mixNamesUsing([id]).map((name) => ({ id: 1 + [...name].reduce((h, c) => (h * 31 + c.charCodeAt(0)) % 9973, 7), name }));

function baseRow(id: number, op: string): Row {
  const t = tracks.find((x) => x.track_id === id);
  return {
    track_id: id,
    label: t ? `${t.artist} - ${t.title}` : `track ${id}`,
    op,
    before_bpm: null,
    after_bpm: null,
    before_first: null,
    after_first: null,
    before_beats: 0,
    after_beats: 0,
    sample_rate: SR,
    skip: null,
    note: null,
    mixes: t ? mixRefs(id) : [],
  };
}

function fillRow(row: Row, before: Doc | null, after: Doc) {
  row.before_bpm = before ? medianBpm(before) : null;
  row.before_first = before?.beats[0]?.pos ?? null;
  row.before_beats = before?.beats.length ?? 0;
  row.after_bpm = medianBpm(after);
  row.after_first = after.beats[0]?.pos ?? null;
  row.after_beats = after.beats.length;
}

const publicPlan = (p: Plan) => ({ plan_id: p.plan_id, rows: p.rows });

/** Mock of `batch::plan`: computes every new grid, keeps them, returns the rows. */
function plan(ids: number[], op: any): Plan {
  if (op.kind === "nudge" && !(Number.isFinite(op.ms) && op.ms !== 0 && Math.abs(op.ms) <= 5000)) throw "A nudge must be between 0 and 5000 ms either way.";
  const out: Plan = { plan_id: ++planSeq, rows: [], docs: new Map() };
  for (const id of new Set<number>(ids)) {
    const t = tracks.find((x) => x.track_id === id);
    const row = baseRow(id, op.kind);
    out.rows.push(row);
    if (!t) {
      row.skip = "not in the library";
      continue;
    }
    if (!hasGrid(id)) {
      row.skip = `${needs.get(id)}; re-analyse it first`;
      continue;
    }
    const doc = saved.get(id) ?? demoGrid(id);
    row.before_bpm = medianBpm(doc);
    row.before_first = doc.beats[0].pos;
    row.before_beats = doc.beats.length;
    let edit: any;
    if (op.kind === "nudge") edit = { kind: "nudge", frames: Math.round((op.ms / 1000) * SR) };
    else if (op.kind === "suggested") {
      const s = assess(id, { ...op.settings, quality_rule: false });
      if (!s) {
        row.skip = "the tempo rule does not flag this track";
        continue;
      }
      if (!s.fix) {
        row.skip = "flagged, but neither half nor double fits its window";
        continue;
      }
      edit = { kind: s.fix };
      row.op = s.fix;
    } else edit = { kind: op.kind };
    try {
      const next = applyEdit(doc, edit, SR, (t.play_time_secs + 1) * SR);
      if (JSON.stringify(next) === JSON.stringify(doc)) row.skip = "the grid is already like that";
      else {
        fillRow(row, doc, next);
        out.docs.set(id, { before: doc, after: next });
      }
    } catch (e) {
      row.skip = String(e).replace(/^invalid: /, "");
    }
  }
  current = out;
  return out;
}

// the windows the analysis uses to pick the tempo it "finds"
const ANALYSER_RULES: Rules = {
  tempo_rule: true,
  fallback_min: 70,
  fallback_max: 180,
  genres: [
    { name: "house", min: 115, max: 132 },
    { name: "techno", min: 120, max: 150 },
    { name: "trance", min: 128, max: 145 },
    { name: "drum & bass", min: 160, max: 180 },
  ],
  quality_rule: false,
  min_confidence_pct: 0,
};

export const handlers: Handlers = {
  grid_suspects: (a) => {
    const r = a.settings as Rules;
    if (!(r.fallback_min >= 20 && r.fallback_max <= 400 && r.fallback_min < r.fallback_max)) {
      throw "The general tempo window must run from 20 up to 400 BPM, with the first number below the second.";
    }
    const suspects = [];
    let unreadable = 0;
    for (const t of tracks) {
      if (!hasGrid(t.track_id)) {
        unreadable++;
        continue;
      }
      const s = assess(t.track_id, r);
      if (s) suspects.push(s);
    }
    return { suspects, checked: tracks.length - unreadable, unreadable };
  },
  grid_batch_plan: (a) => publicPlan(plan(a.trackIds, a.op)),
  // like the real read-only job: one progress event per track, then job-done; the "analyser" finds
  // the tempo the genre rules suggest, with the first beat a few ms off
  grid_batch_analyse: (a) => {
    if (jobLock.running) throw "Another job is still running.";
    const ids = [...new Set<number>(a.trackIds)];
    if (ids.length === 0) throw "No tracks to analyse.";
    cancelled = false;
    current = null;
    jobLock.running = true;
    const out: Plan = { plan_id: ++planSeq, rows: ids.map((id) => baseRow(id, "reanalyse")), docs: new Map() };
    const finish = async (stopped: boolean) => {
      for (const row of out.rows) if (!row.skip && !out.docs.has(row.track_id)) row.skip = "not analysed: the run was stopped";
      current = out;
      jobLock.running = false;
      const n = out.docs.size;
      await emit("job-done", { job: "grid-batch-analyse", ok: n, failed: 0, summary: `${n} new grid${n === 1 ? "" : "s"} ready to review${stopped ? " (stopped early)" : ""}`, error: null });
    };
    const step = (i: number) =>
      setTimeout(async () => {
        if (cancelled) return finish(true);
        const row = out.rows[i];
        const t = tracks.find((x) => x.track_id === row.track_id);
        let status = "analysed";
        if (!t) {
          row.skip = "not in the library";
          status = "skipped";
        } else {
          const before = hasGrid(t.track_id) ? (saved.get(t.track_id) ?? demoGrid(t.track_id)) : null;
          const s = before ? assess(t.track_id, ANALYSER_RULES) : null;
          const tempo = before ? (s?.fix === "halve" ? medianBpm(before) / 2 : s?.fix === "double" ? medianBpm(before) * 2 : medianBpm(before)) : t.bpm;
          const after = constantGrid(tempo, 4200 + ((t.track_id * 104729) % 9000), t.play_time_secs * SR, 0);
          fillRow(row, before, after);
          row.note = `even grid at ${tempo.toFixed(2)} BPM`;
          out.docs.set(t.track_id, { before, after });
        }
        await emit("job-progress", { job: "grid-batch-analyse", label: row.label, done: i + 1, total: ids.length, status, bpm: row.after_bpm, detail: row.skip, error: null });
        if (i + 1 === ids.length) await finish(false);
        else step(i + 1);
      }, 450);
    step(0);
    return null;
  },
  grid_batch_result: () => (current ? publicPlan(current) : null),
  grid_batch_cancel: () => {
    cancelled = true;
    return null;
  },
  grid_batch_diff: (a) => {
    if (!current || current.plan_id !== a.planId) throw "That preview is no longer current. Preview the change again.";
    const d = current.docs.get(a.trackId);
    if (!d) throw "That track has no new grid in this preview.";
    return { before: d.before, after: d.after, sample_rate: SR, total_frames: (trackOf(a.trackId).play_time_secs + 1) * SR };
  },
  // like the real write job: saves the previewed grids of the named tracks, one event per track
  grid_batch_apply: (a) => {
    if (jobLock.running) throw "Another job is still running.";
    if (a.trackIds.length === 0) throw "No tracks to change.";
    const p = current;
    if (!p || p.plan_id !== a.planId) throw "That preview is no longer current. Preview the change again.";
    const ids = [...new Set<number>(a.trackIds)];
    jobLock.running = true;
    let ok = 0;
    let skipped = 0;
    ids.forEach((id, i) =>
      setTimeout(async () => {
        const d = p.docs.get(id);
        const row = p.rows.find((r) => r.track_id === id);
        let detail: string | null = null;
        if (d) {
          saved.set(id, d.after);
          trackOf(id).bpm = medianBpm(d.after);
          ok++;
        } else {
          detail = row?.skip ?? "it is not part of this preview";
          skipped++;
        }
        await emit("job-progress", { job: "grid-batch", label: row?.label ?? `track ${id}`, done: i + 1, total: ids.length, status: d ? "changed" : "skipped", bpm: d ? medianBpm(d.after) : null, detail, error: null });
        if (i === ids.length - 1) {
          jobLock.running = false;
          const parts = [ok && `${ok} beat grid${ok === 1 ? "" : "s"} changed`, skipped && `${skipped} skipped`].filter(Boolean);
          await emit("job-done", { job: "grid-batch", ok, failed: 0, summary: parts.join(", ") || "nothing to do", error: null });
        }
      }, 400 * (i + 1)),
    );
    return null;
  },
};
