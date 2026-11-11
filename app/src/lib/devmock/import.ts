// Mock of src-tauri/src/commands/import.rs.
import { emit } from "@tauri-apps/api/event";
import type { CasePlan, ImportOptions, ImportPlan, InteropSource } from "$lib/api/import";
import { cueApplies } from "$lib/views/import/importLogic";
import { baseName } from "$lib/format";
import { AUDIO, tracks } from "./db";
import { jobLock, runJob } from "./jobs";
import type { Handlers } from "./types";

/** The mock dialog answers every pick with the same fake path, so the source of a preview is
 *  read from the path when it names one, else from `globalThis.__mockImportSource` (set it in the
 *  browser console to see the Serato / Traktor / M3U previews), else rekordbox. */
function mockSource(path: string): InteropSource {
  const forced = (globalThis as { __mockImportSource?: InteropSource }).__mockImportSource;
  if (forced) return forced;
  if (/itunes|library\.xml/i.test(path)) return "itunes";
  if (/\.nml$|traktor/i.test(path)) return "traktor";
  if (/\.crate$|serato/i.test(path)) return "serato";
  if (/\.m3u8?$/i.test(path)) return "m3u";
  return "rekordbox";
}

/** An invented import plan over the shared fixture: some rating changes, a new case, an
 *  extended case, one already complete, cue points, and unmatched files (one on disk). */
function mockPlan(path: string, options: ImportOptions): ImportPlan {
  const source = mockSource(path);
  const hasTags = source !== "serato"; // a crate holds paths only: no ratings, no cues
  const pick = (ids: number[]) =>
    ids.map((id) => tracks[id - 1]).map((t) => ({ track_id: t.track_id, title: t.title, artist: t.artist }));
  const ratings = [1, 2, 3, 6, 9].map((id) => {
    const t = tracks[id - 1];
    return { track_id: id, title: t.title, artist: t.artist, from: t.rating, to: (t.rating + 2) % 6 || 5 };
  });
  const cases: CasePlan[] = [
    { index: 0, name: "Warm up", action: "create", case_id: null, tracks: pick([2, 4, 6, 8]), already_in: 0, unmatched: 1, selected: true },
    { index: 1, name: "Sets / Friday", action: "extend", case_id: 1, tracks: pick([14, 15]), already_in: 4, unmatched: 0, selected: true },
    { index: 2, name: "Peak time", action: "unchanged", case_id: 2, tracks: [], already_in: 3, unmatched: 0, selected: true },
    { index: 3, name: "Old favourites", action: "skip", case_id: null, tracks: [], already_in: 0, unmatched: 12, selected: true },
  ];
  const sel = options.playlist_indexes;
  for (const c of cases) c.selected = options.playlists && (sel === null || sel.includes(c.index));
  const unmatched = [
    { title: "Mock Tune One", artist: "Example Artist Z", path: "C:\\Music\\Mock Tune One.mp3", reason: "not on the device", on_disk: true, in_playlist: true },
    { title: "Mock Tune Two", artist: "Example Artist Z", path: "C:\\Music\\Mock Tune Two.flac", reason: "not on the device", on_disk: true, in_playlist: true },
    { title: "Mock Tune Three", artist: "Example Artist Y", path: "D:\\Gone\\Mock Tune Three.mp3", reason: "not on the device", on_disk: false, in_playlist: false },
    { title: "Mock Tune Four", artist: "Example Artist Y", path: "", reason: "2 device tracks share this title and artist", on_disk: false, in_playlist: false },
    { title: "Mock Tune Five", artist: "", path: "//example-host/share/Mock Tune Five.mp3", reason: "not on the device; the file location was not checked because it points at a network or device location", on_disk: false, in_playlist: true },
  ].map((u) => ({ path_checked: !u.reason.includes("not checked"), ...u }));
  return {
    source,
    foreign_tracks: 20,
    matched: 16,
    matched_by_file: 12,
    matched_by_tags: 4,
    ratings: options.ratings && hasTags ? ratings : [],
    ratings_unchanged: 3,
    cases,
    cues: hasTags
      ? [2, 4, 6].map((id, i) => {
          const secs = 12.25 * id;
          const existing = i === 1 ? 5 * 44100 : i === 2 ? Math.round(secs * 44100) : null;
          return {
            track_id: id,
            title: tracks[id - 1].title,
            artist: tracks[id - 1].artist,
            cue_secs: secs,
            cue_frames: Math.round(secs * 44100),
            existing_frames: existing,
            action: existing === null ? ("set" as const) : existing === Math.round(secs * 44100) ? ("same" as const) : ("replace" as const),
          };
        })
      : [],
    notes: source === "serato" ? ["Skipped Broken.crate: this is not a Serato crate (it does not start with a vrsn record)", "Smart crates (rule-based) are not imported; only ordinary crates are."] : [],
    unmatched_total: unmatched.length,
    unmatched,
    addable_paths: unmatched.filter((u) => u.on_disk).map((u) => u.path),
  };
}

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

export const handlers: Handlers = {
  interop_preview: async (a) => {
    await wait(250);
    return mockPlan(String(a.path), a.options as ImportOptions);
  },
  interop_apply: (a) => {
    if (jobLock.running) throw "Another job is still running.";
    const plan = mockPlan(String(a.path), a.options as ImportOptions);
    const opts = a.options as ImportOptions;
    const steps = [
      ...(opts.ratings && plan.ratings.length ? [`${plan.ratings.length} ratings`] : []),
      ...(plan.cues.some((c) => cueApplies(opts.cues, c.action)) ? [`${plan.cues.filter((c) => cueApplies(opts.cues, c.action)).length} cue points`] : []),
      ...plan.cases.filter((c) => c.selected && (c.action === "create" || c.action === "extend")).map((c) => c.name),
    ];
    jobLock.running = true;
    steps.forEach((label, i) =>
      setTimeout(async () => {
        await emit("job-progress", { job: "interop", label, done: i + 1, total: steps.length, status: "done", bpm: null, detail: null, error: null });
        if (i === steps.length - 1) {
          jobLock.running = false;
          await emit("job-done", { job: "interop", ok: steps.length, failed: 0, summary: `${steps.length} step${steps.length === 1 ? "" : "s"} applied (mock)`, error: null });
        }
      }, 500 * (i + 1)),
    );
    if (steps.length === 0) {
      jobLock.running = false;
      setTimeout(() => emit("job-done", { job: "interop", ok: 0, failed: 0, summary: "nothing to change", error: null }), 300);
    }
    return null;
  },
  interop_export: (a) => ({ path: String(a.dest), tracks: a.scope === "library" || a.format === "rekordbox" ? tracks.length : 6 }),

  expand_audio_paths: (a) => (a.paths as string[]).filter((p) => AUDIO.test(p) || !/\.[a-z0-9]+$/i.test(p)),
  add_tracks: (a) => {
    if (jobLock.running) throw "Another job is still running.";
    const files = (a.files as string[]).map(baseName);
    runJob("add", files, (label, i) => {
      if (/bad/i.test(label)) return { error: 'Decode(Unsupported("no suitable format reader found"))' };
      if (!/skip/i.test(label)) {
        const id = tracks.length + 1;
        tracks.push({
          ...tracks[i % 6],
          track_id: id,
          title: label.replace(/\.[^.]+$/, ""),
          location: `/pmdb_tracks/0/0/new${id}.mp3`,
          analysed: true,
        });
      }
      return { bpm: 120 + i * 3.5 };
    });
    return null;
  },
};
