// Mock of src-tauri/src/commands/cases/dupes.rs (duplicate finder and "which cases hold this
// track"). Invented placeholder data only. The groups, the plan and the jobs behave like the
// backend's: the plan is one function used by the preview and the start, a merge removes what it
// says, and the fingerprint job fills the "cache" the sound check reads.
import { emit } from "@tauri-apps/api/event";
import type { DupSettings, FindResult, Group, GroupPlan, Member, MergePlan, MergeRequest } from "../api/duplicates";
import type { TrackHolding } from "../api/cases";
import { cases } from "./db";
import type { Handlers } from "./types";

const DAY = 86_400_000;
const NOW = Date.now();

let settings: DupSettings = {
  by_tags: true,
  by_size: true,
  by_audio: false,
  strip_feat: true,
  ignore_brackets: false,
  ignore_the: true,
  tag_duration_secs: 10,
  size_duration_secs: 1,
  audio_threshold: 0.9,
  audio_duration_secs: 3,
  audio_offset_secs: 5,
  audio_min_overlap: 0.8,
  fingerprint_threads: 0,
  keeper_rule: "bitrate",
  move_cases: true,
  rating_rule: "max",
  plays_rule: "max",
  allow_row_removal: false,
};

function row(id: number, over: Partial<Member>): Member {
  return {
    track_id: id, title: "Demo Track", artist: "Example Artist", album: "Sample Album", album_artist: "", composer: "", genre: "House", label: "", key: "8A", comments: "",
    producer: "", remixer: "", year: 2020, track_number: 0, disc_number: 0, format: "MP3", bit_rate_kbps: 320, sample_rate: 44100, file_size: 8_000_000, play_time_secs: 200,
    bpm: 128, rating: 0, times_played: 0, last_played: -1, date_added: NOW - 30 * DAY, location: `/pmdb_tracks/${id % 16}/${(id * 3) % 16}/mock${id}.mp3`, analysed: true,
    cases: [], mixes: [], file: "present", ...over,
  };
}

/** The synthetic copies. `alive` shrinks when a mock merge removes rows. */
const fixture: Member[] = [
  row(201, { title: "Demo Track 03", artist: "Example Artist C", bit_rate_kbps: 128, file_size: 3_200_000, rating: 2, times_played: 3, cases: [{ case_id: 1, name: "Warm-up", position: 3, of: 6 }] }),
  row(202, { title: "Demo Track 03 (feat. Guest)", artist: "example artist c", bit_rate_kbps: 320, file_size: 8_100_000, play_time_secs: 202, rating: 4, times_played: 9, last_played: NOW - 2 * DAY, date_added: NOW - 3 * DAY, cases: [{ case_id: 2, name: "Peak time", position: 1, of: 4 }] }),
  row(203, { title: "Demo Track 03", artist: "Example Artist C", bit_rate_kbps: 0, file_size: 0, file: "missing", format: "FLAC", mixes: ["Sample Mix"], cases: [{ case_id: 2, name: "Peak time", position: 3, of: 4 }] }),
  row(204, { title: "Sample Intro", artist: "Example Artist A", file_size: 5_000_000, play_time_secs: 90, key: "", cases: [] }),
  row(205, { title: "Untitled 05", artist: "", file_size: 5_000_000, play_time_secs: 90, bit_rate_kbps: 192, cases: [{ case_id: 1, name: "Warm-up", position: 5, of: 6 }] }),
  row(206, { title: "Demo Track 07", artist: "Example Artist B", file_size: 9_000_000, play_time_secs: 245, times_played: 1 }),
  row(207, { title: "Demo Track 07 Remaster", artist: "Example Artist B", file_size: 9_400_000, play_time_secs: 245, format: "FLAC", bit_rate_kbps: 900, sample_rate: 48000 }),
];
let alive = new Set(fixture.map((m) => m.track_id));
let fingerprinted = false;
let cancel = false;

const byId = (id: number) => fixture.find((m) => m.track_id === id)!;

function groups(s: DupSettings): Group[] {
  const g = (ids: number[], links: Group["links"], keeper: number): Group | null => {
    const live = ids.filter((i) => alive.has(i));
    return live.length > 1 ? { id: live[0], members: live.map(byId), links: links.filter((l) => alive.has(l.a) && alive.has(l.b)), modes: [...new Set(links.map((l) => l.mode))], suggested_keeper: live.includes(keeper) ? keeper : live[0] } : null;
  };
  const out: (Group | null)[] = [];
  if (s.by_tags) out.push(g([201, 202, 203], [{ a: 201, b: 202, mode: "tags", score: null, offset_secs: null }, { a: 202, b: 203, mode: "tags", score: null, offset_secs: null }], 202));
  if (s.by_size) out.push(g([204, 205], [{ a: 204, b: 205, mode: "size", score: null, offset_secs: null }], 205));
  if (s.by_audio && fingerprinted) out.push(g([206, 207], [{ a: 206, b: 207, mode: "audio", score: 0.973, offset_secs: 0.5 }], 207));
  return out.filter((x): x is Group => x !== null);
}

function find(s: DupSettings): FindResult {
  return {
    groups: groups(s),
    pending: s.by_audio && !fingerprinted ? [...alive].filter((i) => i >= 206) : [],
    fingerprinted: s.by_audio && fingerprinted ? 2 : 0,
    tracks: 30 + alive.size,
    warnings: s.by_tags ? ["1 track had no artist or no title, so the tag check could not compare it."] : [],
  };
}

const MIXED = new Set([203]);

/** The one function behind both the preview and the start, like the backend's. */
function plan(req: MergeRequest): MergePlan {
  if (req.groups.length === 0) throw "Pick at least one group to merge.";
  if (req.delete_files && !req.remove_rows) throw "Deleting the files needs the copies to be removed from the library as well: a track whose file is gone would stay in the library.";
  if (req.remove_rows && !req.settings.allow_row_removal) throw "Removing copies from the library is switched off in the duplicate settings (deleting rows has not been tried on a real Pacemaker yet).";
  const labels: MergePlan["labels"] = [];
  const files: MergePlan["files"] = [];
  const out: GroupPlan[] = req.groups.map((g) => {
    const k = byId(g.keeper);
    const rows = [k, ...g.others.map(byId)];
    for (const r of rows) labels.push({ track_id: r.track_id, label: [r.artist, r.title].filter(Boolean).join(" - ") || r.location });
    const keeperCases = new Set(k.cases.map((c) => c.case_id));
    const case_edits: GroupPlan["case_edits"] = [];
    const remove = req.remove_rows ? g.others.filter((i) => !MIXED.has(i)) : [];
    const caseIds = [...new Set(g.others.flatMap((i) => byId(i).cases.map((c) => c.case_id)))];
    for (const cid of caseIds) {
      const inCase = g.others.filter((i) => byId(i).cases.some((c) => c.case_id === cid));
      const name = cases.find((c) => c.id === cid)?.name ?? `case ${cid}`;
      if (req.settings.move_cases) {
        keeperCases.has(cid) ? case_edits.push({ case_id: cid, case_name: name, kind: "already_there", others: inCase }) : (case_edits.push({ case_id: cid, case_name: name, kind: "replace", others: inCase }), keeperCases.add(cid));
      } else if (inCase.some((i) => remove.includes(i))) {
        case_edits.push({ case_id: cid, case_name: name, kind: "drop", others: inCase.filter((i) => remove.includes(i)) });
      }
    }
    const ratingAfter = req.settings.rating_rule === "max" ? Math.max(...rows.map((r) => r.rating)) : k.rating;
    const playsAfter = req.settings.plays_rule === "keeper" ? k.times_played : req.settings.plays_rule === "sum" ? rows.reduce((s, r) => s + r.times_played, 0) : Math.max(...rows.map((r) => r.times_played));
    const lastAfter = req.settings.plays_rule === "keeper" ? k.last_played : Math.max(...rows.map((r) => r.last_played));
    if (req.delete_files) {
      for (const id of remove) {
        const rel = byId(id).location.replace("/pmdb_tracks/", "");
        const present = byId(id).file === "present";
        files.push({ track_id: id, path: rel, kind: "audio", size: byId(id).file_size, exists: present, kept_because: null });
        files.push({ track_id: id, path: rel.replace(/\.[^.]+$/, ".str"), kind: "grid", size: 2048, exists: present, kept_because: null });
      }
    }
    return {
      keeper: g.keeper,
      others: g.others,
      case_edits,
      stats: { rating: { before: k.rating, after: ratingAfter }, times_played: { before: k.times_played, after: playsAfter }, last_played: { before: k.last_played, after: lastAfter } },
      remove: remove.map((track_id) => ({ track_id, location: byId(track_id).location })),
      blocked: req.remove_rows ? g.others.filter((i) => MIXED.has(i)).map((track_id) => ({ track_id, mixes: byId(track_id).mixes })) : [],
    };
  });
  const gone = files.filter((f) => f.exists && !f.kept_because);
  const problems = req.delete_files
    ? out.flatMap((g) => {
        const keeperGone = byId(g.keeper).file !== "present";
        const loses = g.remove.some((r) => files.some((f) => f.track_id === r.track_id && f.kind === "audio" && f.exists && !f.kept_because));
        const name = labels.find((l) => l.track_id === g.keeper)?.label;
        return keeperGone && loses ? [`“${name}”, the track you keep, has no audio file on the Pacemaker, but a copy you are deleting does: after this there would be no playable file.`] : [];
      })
    : [];
  const p: MergePlan = {
    groups: out,
    labels,
    files,
    notes: out.flatMap((g) => g.blocked.map((b) => `“${labels.find((l) => l.track_id === b.track_id)?.label}” stays in the library: the mix ${b.mixes.map((m) => `“${m}”`).join(", ")} uses it. Its case places still move.`)),
    problems,
    totals: {
      groups: out.length,
      case_entries: out.reduce((s, g) => s + g.case_edits.reduce((n, e) => n + e.others.length, 0), 0),
      rows_removed: out.reduce((s, g) => s + g.remove.length, 0),
      files_deleted: gone.length,
      bytes: gone.reduce((s, f) => s + f.size, 0),
    },
    digest: "",
  };
  p.digest = String([...JSON.stringify([out, files, problems])].reduce((h, c) => (h * 31 + c.charCodeAt(0)) >>> 0, 7));
  return p;
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export const handlers: Handlers = {
  dupes_settings_get: () => ({ settings, warning: null }),
  dupes_settings_set: (a) => {
    settings = { ...(a.settings as DupSettings) };
    return { settings, warning: null };
  },
  dupes_find: (a) => find(a.settings as DupSettings),
  dupes_fingerprint_start: () => {
    const todo = find({ ...settings, by_audio: true }).pending;
    if (todo.length === 0) throw "Every track that could be compared already has its fingerprint.";
    cancel = false;
    void (async () => {
      let done = 0;
      for (const id of todo) {
        await sleep(900);
        if (cancel) break;
        done++;
        await emit("job-progress", { job: "dupes-fingerprint", label: `${byId(id).artist} - ${byId(id).title}`, done, total: todo.length, status: "fingerprinted", bpm: null, detail: null, error: null });
      }
      fingerprinted = done === todo.length;
      await emit("job-done", { job: "dupes-fingerprint", ok: done, failed: 0, summary: cancel ? `Fingerprinted ${done} of ${todo.length} tracks, stopped at your request; what was done is kept` : `Fingerprinted ${done} of ${todo.length} tracks`, error: null });
    })();
    return todo.length;
  },
  dupes_cancel: () => {
    cancel = true;
  },
  dupes_merge_plan: (a) => plan(a.request as MergeRequest),
  dupes_merge_start: (a) => {
    const req = a.request as MergeRequest;
    const p = plan(req);
    if (p.digest !== a.digest) throw "The library changed after the preview was made, so nothing was written. The preview has been refreshed: check it and press Merge again.";
    void (async () => {
      let i = 0;
      for (const g of p.groups) {
        await sleep(700);
        i++;
        await emit("job-progress", { job: "dupes-merge", label: `Merging ${p.labels.find((l) => l.track_id === g.keeper)?.label}`, done: i, total: p.groups.length, status: "merged", bpm: null, detail: null, error: null });
        for (const r of g.remove) alive.delete(r.track_id);
      }
      await emit("job-done", {
        job: "dupes-merge",
        ok: p.groups.length,
        failed: 0,
        summary: `Merged ${p.groups.length} group${p.groups.length === 1 ? "" : "s"}, ${p.totals.case_entries} case places moved to the kept track${p.totals.rows_removed ? `, ${p.totals.rows_removed} copies removed from the library` : ""}${p.totals.files_deleted ? `, ${p.totals.files_deleted} files deleted` : ""}`,
        error: null,
      });
    })();
  },
  cases_holding: (a): TrackHolding[] =>
    (a.trackIds as number[]).map((track_id) => ({
      track_id,
      cases: cases
        .filter((c) => c.ids.includes(track_id))
        .map((c) => ({ case_id: c.id, name: c.name, position: c.ids.indexOf(track_id) + 1, of: c.ids.length, smart_copy_of: null }))
        .sort((x, y) => x.name.localeCompare(y.name)),
      smart: track_id % 5 === 0 ? [{ id: 1, name: "Rated 4+", on_device: false, device_case_id: null }] : [],
    })),
};
