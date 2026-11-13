// Mock of src-tauri/src/commands/analysis/sections.rs (section detection). Picked up
// automatically by devmock/index.ts. An in-memory stand-in for the app-side cache: location ->
// {size, duration, bars, kind}. The mock keeps the real commands' rules (new or changed tracks
// only unless recompute, short tracks without sections, settings checked in range) but not their
// maths: the sections of a track come from a fixed template cut to its length. Invented data only.
import { emit } from "@tauri-apps/api/event";
import { tracks } from "./db";
import { jobLock } from "./jobs";
import type { Handlers } from "./types";

type SectionKind = "intro" | "build" | "drop" | "breakdown" | "outro" | "section";
type MockEntry = { size: number; duration: number; bars: number; outcome: "ok" | "flat" | "skip"; skip: string | null };

let settings = {
  sensitivity: 3, min_section_bars: 8, phrase_bars: 0, origin: "first_downbeat", snap_tolerance_bars: 2, min_bars: 32, threads: 2, show_markers: true,
};
const cache = new Map<string, MockEntry>();

const barSecs = (t: (typeof tracks)[number]) => 240 / t.bpm;
const barsOf = (t: (typeof tracks)[number]) => Math.max(0, Math.floor((t.play_time_secs - 0.4) / barSecs(t)));

/** What the job would store for a track: some have no downbeat, some are flat, most have sections. */
function measureMock(t: (typeof tracks)[number]): MockEntry {
  const base = { size: t.file_size, duration: t.play_time_secs, bars: barsOf(t) };
  if (t.track_id % 8 === 5) return { ...base, outcome: "skip", skip: "the beat grid has no first downbeat set" };
  if (t.track_id % 11 === 0) return { ...base, outcome: "flat", skip: null };
  return { ...base, outcome: "ok", skip: null };
}

// two thirds of the fixture tracks start analysed, so the table shows both markers and blanks
for (const t of tracks) if (t.track_id % 3 !== 0) cache.set(t.location, measureMock(t));

/** intro / build / drop / breakdown / build / drop / outro, cut to the track's bars. */
const TEMPLATE: [SectionKind, number][] = [["intro", 16], ["build", 8], ["drop", 32], ["breakdown", 16], ["build", 8], ["drop", 24], ["outro", 1000]];

function sectionsFor(t: (typeof tracks)[number], e: MockEntry) {
  const secsPerBar = barSecs(t);
  const edge = (bar: number) => Math.round((0.2 + bar * secsPerBar) * 1000) / 1000;
  const phrase = settings.phrase_bars || (t.track_id % 4 === 0 ? 16 : 8);
  const out: { kind: SectionKind; start: number; end: number }[] = [];
  let bar = 0;
  for (const [kind, len] of TEMPLATE) {
    if (bar >= e.bars) break;
    const end = kind === "outro" || bar + len > e.bars - 4 ? e.bars : bar + len;
    out.push({ kind, start: bar, end });
    bar = end;
  }
  // low sensitivity keeps only the clear changes: builds are the first to go
  const kept = settings.sensitivity <= 2 ? out.filter((s) => s.kind !== "build") : out;
  const merged = kept.map((s, i) => ({ ...s, start: i === 0 ? 0 : kept[i].start, end: i === kept.length - 1 ? e.bars : kept[i + 1].start }));
  return merged.map((s, i) => {
    const conf = Math.round((0.55 + 0.45 * Math.abs(Math.sin(t.track_id * 7 + i * 3))) * 100) / 100;
    return {
      kind: s.kind,
      start_secs: s.start === 0 ? 0 : edge(s.start),
      end_secs: s.end === e.bars ? e.duration : edge(s.end),
      start_bar: s.start,
      end_bar: s.end,
      confidence: s.kind === "build" ? Math.min(conf, 0.62) : conf,
      start_confidence: s.start === 0 ? 1 : Math.min(1, conf + 0.05),
      on_phrase: s.start % phrase === 0,
    };
  });
}

function status(t: (typeof tracks)[number]): "ready" | "flat" | "skipped" | "stale" | "none" {
  const e = cache.get(t.location);
  if (!e) return "none";
  if (e.size !== t.file_size) return "stale";
  if (e.outcome === "skip") return "skipped";
  if (e.bars < settings.min_bars) return "skipped";
  return e.outcome === "flat" ? "flat" : "ready";
}

function mockState() {
  const entries = tracks
    .filter((t) => status(t) === "ready")
    .map((t) => {
      const e = cache.get(t.location)!;
      const secs = sectionsFor(t, e);
      return { track_id: t.track_id, bars: e.bars, phrase_bars: settings.phrase_bars || (t.track_id % 4 === 0 ? 16 : 8), sections: secs };
    });
  const count = (k: string) => tracks.filter((t) => status(t) === k).length;
  const reasons = new Map<string, number>();
  for (const t of tracks) {
    if (status(t) !== "skipped") continue;
    const e = cache.get(t.location)!;
    const why = e.skip ?? `fewer than ${settings.min_bars} bars`;
    reasons.set(why, (reasons.get(why) ?? 0) + 1);
  }
  return {
    entries, total: tracks.length, ready: entries.length, flat: count("flat"), skipped: count("skipped"),
    skip_reasons: [...reasons].map(([reason, n]) => ({ reason, count: n })).sort((a, b) => b.count - a.count),
    stale: count("stale"), none: count("none"), settings: { ...settings }, problem: null,
  };
}

function targets(ids: number[] | null, recompute: boolean) {
  return tracks.filter((t) => (!ids || ids.includes(t.track_id)) && (recompute || ["none", "stale"].includes(status(t))));
}

export const handlers: Handlers = {
  analysis_sections_state: () => mockState(),
  analysis_sections_settings: () => ({ ...settings }),
  analysis_sections_save_settings: (a) => {
    const s = a.settings;
    const inRange = (v: number, lo: number, hi: number) => Number.isInteger(v) && v >= lo && v <= hi;
    if (!inRange(s.sensitivity, 1, 5)) throw "Sensitivity must be between 1 and 5.";
    if (!inRange(s.min_section_bars, 4, 32)) throw "The shortest section must be between 4 and 32 bars.";
    if (![0, 4, 8, 16, 32].includes(s.phrase_bars)) throw "The phrase length must be automatic, 4, 8, 16 or 32 bars.";
    if (!["first_downbeat", "best_fit"].includes(s.origin)) throw "Unknown phrase origin.";
    if (!inRange(s.snap_tolerance_bars, 0, 8)) throw "The snap distance must be between 0 and 8 bars.";
    if (!inRange(s.min_bars, 16, 512)) throw "The shortest track must be between 16 and 512 bars.";
    if (!inRange(s.threads, 1, 8)) throw "Tracks analysed at once must be between 1 and 8.";
    settings = { ...s };
    return { ...settings };
  },
  analysis_sections_plan: (a) => targets((a.trackIds as number[] | null) ?? null, !!a.recompute).map((t) => t.track_id),
  analysis_sections_clear: () => {
    if (jobLock.running) throw "A job is still running.";
    const n = cache.size;
    cache.clear();
    return n;
  },
  // like the real job: reads only, one progress event each; every 6th target has no downbeat and
  // the 9th fails, so the skipped and failed states show up
  analysis_sections_analyse: (a) => {
    if (jobLock.running) throw "Another job is still running.";
    const ids = (a.trackIds as number[] | null) ?? null;
    if (ids && ids.length === 0) throw "No tracks to analyse.";
    const list = targets(ids, !!a.recompute);
    if (list.length === 0) throw "Nothing to analyse: those tracks were already analysed. Choose Analyse again to measure them anew.";
    jobLock.running = true;
    let ok = 0;
    let failed = 0;
    let skipped = 0;
    list.forEach((t, i) =>
      setTimeout(async () => {
        let st = "analysed";
        let detail: string | null = null;
        let error: string | null = null;
        if (i % 9 === 8) {
          st = "failed";
          error = "could not decode the file";
          failed++;
        } else {
          const e = i % 6 === 5 ? ({ ...measureMock(t), outcome: "skip", skip: "the beat grid has no first downbeat set" } as MockEntry) : measureMock(t);
          cache.set(t.location, e);
          if (e.outcome === "skip") {
            st = "skipped";
            detail = e.skip;
            skipped++;
          } else {
            detail = e.outcome === "flat" ? "no clear sections at this sensitivity" : `${sectionsFor(t, e).length} sections in ${e.bars} bars`;
            ok++;
          }
        }
        await emit("job-progress", { job: "sections", label: `${t.artist} - ${t.title}`, done: i + 1, total: list.length, status: st, bpm: null, detail, error });
        if (i === list.length - 1) {
          jobLock.running = false;
          const parts = [
            ok ? `${ok} analysed` : "",
            skipped ? `${skipped} with nothing to measure (no usable grid or first downbeat, too short, or silent)` : "",
            failed ? `${failed} failed` : "",
          ].filter(Boolean);
          await emit("job-done", { job: "sections", ok: ok + skipped, failed, summary: parts.join(", ") || "nothing to do", error: null });
        }
      }, 350 * (i + 1)),
    );
    return null;
  },
};
