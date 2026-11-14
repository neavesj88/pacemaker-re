// Mock of src-tauri/src/commands/analysis.rs. Add one entry per command, keyed by command name;
// this file is picked up automatically by devmock/index.ts.
import { emit } from "@tauri-apps/api/event";
import { canonicalKeyName } from "../state/keyLogic";
import { tracks } from "./db";
import { jobLock } from "./jobs";
import type { Handlers } from "./types";

function seeded(text: string) {
  let s = 0;
  for (const c of text) s = (s * 31 + c.charCodeAt(0)) >>> 0;
  return () => ((s = (s * 1103515245 + 12345) >>> 0) / 2 ** 32);
}

const KEYS = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

// ---- energy (auto) ----
// An in-memory stand-in for the app-side cache: location -> {size, score or skip}. Two thirds of
// the fixture tracks start analysed, so the table shows both numbers and blanks. The mock keeps
// the real command's rules (missing/changed only unless recompute, short tracks without a number,
// a fit needing 20 tracks) but not its maths: a score is a seeded value.
type MockEnergy = { size: number; duration: number; score: number | null; skip: string | null };
const FIXED = { lo: 0.18, hi: 0.78 };
const energyCache = new Map<string, MockEnergy>();
let energySettings = { scale: "fit", fit_low_pct: 5, fit_high_pct: 95, min_secs: 20, threads: 2 };
for (const t of tracks) {
  if (t.track_id % 3 === 0) continue;
  energyCache.set(t.location, { size: t.file_size, duration: t.play_time_secs, score: 0.2 + 0.6 * seeded(`e${t.location}`)(), skip: null });
}

const mockStatus = (t: (typeof tracks)[number]) => {
  const e = energyCache.get(t.location);
  if (!e) return "none";
  if (e.size !== t.file_size) return "stale";
  if (e.score === null) return "skipped";
  return e.duration < energySettings.min_secs ? "skipped" : "ready";
};

function mockEnergyTargets(ids: number[] | null, recompute: boolean) {
  return tracks.filter((t) => (!ids || ids.includes(t.track_id)) && (recompute || ["none", "stale"].includes(mockStatus(t))));
}

function mockEnergyState() {
  const scored = tracks.filter((t) => mockStatus(t) === "ready").map((t) => ({ t, score: energyCache.get(t.location)!.score! }));
  let anchors = FIXED;
  let used = "fixed";
  let note: string | null = null;
  if (energySettings.scale === "fit") {
    const s = scored.map((x) => x.score).sort((a, b) => a - b);
    const at = (p: number) => s[Math.round((p / 100) * (s.length - 1))];
    if (s.length >= 20 && at(energySettings.fit_high_pct) - at(energySettings.fit_low_pct) >= 0.08) {
      anchors = { lo: at(energySettings.fit_low_pct), hi: at(energySettings.fit_high_pct) };
      used = "fit";
    } else {
      note = "The fixed scale is in use: fitting to the library needs at least 20 analysed tracks with a real spread between them.";
    }
  }
  const entries = scored.map(({ t, score }) => {
    const energy = Math.round((1 + 9 * Math.min(1, Math.max(0, (score - anchors.lo) / (anchors.hi - anchors.lo)))) * 10) / 10;
    return { track_id: t.track_id, energy, level: Math.min(10, Math.max(1, Math.round(energy))), score: Math.round(score * 1000) / 1000 };
  });
  const count = (k: string) => tracks.filter((t) => mockStatus(t) === k).length;
  return {
    entries, total: tracks.length, ready: entries.length, skipped: count("skipped"), stale: count("stale"), none: count("none"),
    scale_used: used, anchor_lo: anchors.lo, anchor_hi: anchors.hi, note, settings: { ...energySettings }, problem: null,
  };
}

/** The mock library's Energy (auto) estimate of a track (1.0 to 10.0), or null: what the smart-case twin reads. */
export function mockAutoEnergy(trackId: number): number | null {
  return mockEnergyState().entries.find((e) => e.track_id === trackId)?.energy ?? null;
}

/** The BPM range the mock saves (low end); `foldBpm` is what the mock's analysis does with it. */
export let bpmRangeLo = 88;
const RANGE_LOS = [68, 78, 88, 98];

/** The mock's analysis: the track's tempo folded into the saved window, as the real analyser does. */
export function foldBpm(bpm: number): number {
  let b = bpm;
  while (b < bpmRangeLo) b *= 2;
  while (b >= 2 * bpmRangeLo) b /= 2;
  return b;
}

export const handlers: Handlers = {
  analysis_bpm_range_get: () => ({ lo: bpmRangeLo }),
  analysis_bpm_range_set: (a) => {
    const lo = a.settings?.lo;
    if (!RANGE_LOS.includes(lo)) throw "The BPM range must be one of 68-136, 78-156, 88-176, 98-196.";
    bpmRangeLo = lo;
    return { lo };
  },
  // like the real quick write: every entry is checked first, then all are written or none; a
  // Camelot code or free text is refused, flats are stored as the detector spells them (sharps)
  analysis_set_keys: (a) => {
    const entries = a.entries as { track_id: number; key: string }[];
    if (entries.length === 0) throw "No tracks to change.";
    const wanted = entries.map((e) => {
      const name = canonicalKeyName(e.key);
      if (!name) throw `"${String(e.key).trim()}" is not a musical key (for example Am, F#m, Bb or C).`;
      const t = tracks.find((x) => x.track_id === e.track_id);
      if (!t) throw `invalid: no track ${e.track_id}`;
      return { t, name };
    });
    let changed = 0;
    for (const { t, name } of wanted) {
      if (t.key !== name) {
        t.key = name;
        changed++;
      }
    }
    return changed;
  },
  analysis_energy_state: () => mockEnergyState(),
  analysis_energy_settings: () => ({ ...energySettings }),
  analysis_energy_save_settings: (a) => {
    const s = a.settings;
    const inRange = (v: number, lo: number, hi: number) => Number.isFinite(v) && v >= lo && v <= hi;
    if (!["fit", "fixed"].includes(s.scale)) throw "Unknown scale.";
    if (!inRange(s.fit_low_pct, 0, 40)) throw "The low percentile must be between 0 and 40.";
    if (!inRange(s.fit_high_pct, 60, 100)) throw "The high percentile must be between 60 and 100.";
    if (!inRange(s.min_secs, 5, 600)) throw "The shortest track must be between 5 and 600 seconds.";
    if (!inRange(s.threads, 1, 8)) throw "Tracks analysed at once must be between 1 and 8.";
    energySettings = { ...s };
    return { ...energySettings };
  },
  analysis_energy_plan: (a) => mockEnergyTargets((a.trackIds as number[] | null) ?? null, !!a.recompute).map((t) => t.track_id),
  analysis_energy_clear: () => {
    if (jobLock.running) throw "A job is still running.";
    const n = energyCache.size;
    energyCache.clear();
    return n;
  },
  // like the real job: reads only, one progress event each, every 7th target is too short and
  // the 9th fails, so the skipped and failed states show up
  analysis_energy_analyse: (a) => {
    if (jobLock.running) throw "Another job is still running.";
    const ids = (a.trackIds as number[] | null) ?? null;
    if (ids && ids.length === 0) throw "No tracks to analyse.";
    const targets = mockEnergyTargets(ids, !!a.recompute);
    if (targets.length === 0) throw "Nothing to analyse: those tracks were already analysed. Choose Analyse again to measure them anew.";
    jobLock.running = true;
    let ok = 0;
    let failed = 0;
    let skipped = 0;
    targets.forEach((t, i) =>
      setTimeout(async () => {
        let status = "analysed";
        let detail: string | null = null;
        let error: string | null = null;
        if (i % 9 === 8) {
          status = "failed";
          error = "could not decode the file";
          failed++;
        } else if (i % 7 === 6) {
          status = "skipped";
          detail = "too short for an energy estimate";
          energyCache.set(t.location, { size: t.file_size, duration: 3, score: null, skip: detail });
          skipped++;
        } else {
          const score = 0.2 + 0.6 * seeded(`e${t.location}`)();
          energyCache.set(t.location, { size: t.file_size, duration: t.play_time_secs, score, skip: null });
          detail = `energy about ${(1 + 9 * Math.min(1, Math.max(0, (score - FIXED.lo) / (FIXED.hi - FIXED.lo)))).toFixed(1)}`;
          ok++;
        }
        await emit("job-progress", { job: "energy", label: `${t.artist} - ${t.title}`, done: i + 1, total: targets.length, status, bpm: null, detail, error });
        if (i === targets.length - 1) {
          jobLock.running = false;
          const parts = [ok ? `${ok} analysed` : "", skipped ? `${skipped} with no estimate (too short or silent)` : "", failed ? `${failed} failed` : ""].filter(Boolean);
          await emit("job-done", { job: "energy", ok: ok + skipped, failed, summary: parts.join(", ") || "nothing to do", error: null });
        }
      }, 350 * (i + 1)),
    );
    return null;
  },

  // synthetic but stable per track: a kick-heavy body with quieter intro and outro
  analysis_peaks: (a) => {
    const r = seeded(String(a.location));
    const width = Math.max(1, Math.min(16384, Number(a.width) || 100));
    return Array.from({ length: width }, (_, i) => {
      const t = i / width;
      const env = t < 0.08 || t > 0.92 ? 0.35 : 0.8 + 0.2 * Math.sin(t * 9);
      return [Math.round(255 * env * (0.55 + 0.45 * r())), Math.round(170 * env * r()), Math.round(110 * env * r())];
    });
  },

  // like the real job: only keyless tracks (unless overwrite, which needs named tracks), one
  // progress event each, then done. Every 4th target is skipped as too short and the 6th fails,
  // and results under minConfidence are skipped, so the UI's skipped and failed states show up.
  analysis_detect_keys: (a) => {
    if (jobLock.running) throw "Another job is still running.";
    const ids = a.trackIds as number[] | null;
    if (ids && ids.length === 0) throw "No tracks to analyse.";
    if (a.overwrite && !ids) throw "Overwriting existing keys needs the tracks to be named.";
    const floor = Number(a.minConfidence) || 0;
    const targets = tracks.filter((t) => (!ids || ids.includes(t.track_id)) && (a.overwrite || !t.key.trim()));
    jobLock.running = true;
    let ok = 0;
    let failed = 0;
    let skipped = 0;
    const finish = async () => {
      jobLock.running = false;
      const parts = [
        ok ? `${ok} key${ok === 1 ? "" : "s"} written` : "",
        skipped ? `${skipped} skipped` : "",
        failed ? `${failed} failed` : "",
      ].filter(Boolean);
      await emit("job-done", { job: "keys", ok, failed, summary: parts.join(", ") || "nothing to do", error: null });
    };
    if (!targets.length) setTimeout(finish, 300);
    targets.forEach((t, i) =>
      setTimeout(async () => {
        const r = seeded(t.location);
        const key = KEYS[Math.floor(r() * 12)] + (r() < 0.5 ? "m" : "");
        const confidence = (60 + Math.floor(r() * 35)) / 100;
        let status = "detected";
        let detail: string | null = `${key} (${Math.round(confidence * 100)}%)`;
        let error: string | null = null;
        if (i % 6 === 5) {
          status = "failed";
          detail = null;
          error = "could not decode the file";
          failed++;
        } else if (i % 4 === 3) {
          status = "skipped";
          detail = "no clear key (too short, silent or atonal)";
          skipped++;
        } else if (confidence < floor) {
          status = "skipped";
          detail = `low confidence: ${key} (${Math.round(confidence * 100)}%)`;
          skipped++;
        } else {
          t.key = key;
          ok++;
        }
        await emit("job-progress", {
          job: "keys",
          label: `${t.artist} - ${t.title}`,
          done: i + 1,
          total: targets.length,
          status,
          bpm: null,
          detail,
          error,
        });
        if (i === targets.length - 1) await finish();
      }, 500 * (i + 1)),
    );
    return null;
  },
};
