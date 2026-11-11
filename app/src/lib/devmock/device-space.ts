// Mock of the free-space planner commands in src-tauri/src/commands/device/space.rs. Invented data
// only. The plan is a simplified copy of pm_formats::ops_space::plan over a small fixed pool of
// fake tracks, so the browser preview reacts to the rules and the target the way the app does.
// Picked up automatically by devmock/index.ts.
//
// Switches in the archive folder's name: "nowhere" is not a folder, "full" has too little room.
import { runSteps } from "./device";
import type { Handlers } from "./types";

const MB = 1024 * 1024;
const GB = 1024 * MB;
const DAY = 86_400_000;

type Settings = {
  rules: {
    never_played: boolean; stale_months: number; low_rating_max: number; unrated_is_low: boolean; duplicates: boolean;
    min_reasons: number; only_no_case: boolean; only_no_mix: boolean; protect_favourites: boolean; protect_rating_min: number;
    protect_new_days: number; order: "certain" | "largest";
  };
  goal_kind: "free" | "card"; free_gb: number; card_gb: number; reserve_percent: number;
  archive_dir: string; include_grids: boolean; require_archive: boolean;
};

let settings: Settings = {
  rules: {
    never_played: true, stale_months: 6, low_rating_max: 2, unrated_is_low: false, duplicates: true, min_reasons: 1,
    only_no_case: true, only_no_mix: true, protect_favourites: true, protect_rating_min: 4, protect_new_days: 30, order: "certain",
  },
  goal_kind: "free", free_gb: 10, card_gb: 64, reserve_percent: 5, archive_dir: "", include_grids: true, require_archive: true,
};

type Base = {
  id: number; n: number; artist: string; mb: number; plays: number; daysAgo: number | null; rating: number;
  cases: string[]; mixes: string[]; dupOf?: number; addedDaysAgo: number;
};
const base = (id: number, n: number, artist: string, mb: number, plays: number, daysAgo: number | null, rating: number, extra: Partial<Base> = {}): Base => ({
  id, n, artist, mb, plays, daysAgo, rating, cases: [], mixes: [], addedDaysAgo: 400, ...extra,
});

let pool: Base[] = [
  base(101, 31, "Example Artist A", 612, 0, null, 0),
  base(102, 32, "Example Artist B", 540, 0, null, 0),
  base(103, 33, "Example Artist C", 480, 4, 300, 1),
  base(104, 34, "Example Artist D", 455, 2, 520, 2),
  base(105, 35, "Example Artist A", 390, 9, 20, 5, { cases: ["Warm-up"] }),
  base(106, 36, "Example Artist E", 372, 0, null, 0, { mixes: ["Demo Mix 1"] }),
  base(107, 37, "Example Artist F", 340, 1, 230, 3),
  base(108, 38, "Example Artist B", 318, 0, null, 0, { dupOf: 101 }),
  base(109, 39, "Example Artist G", 296, 6, 700, 1, { mixes: ["Demo Mix 2"] }),
  base(110, 40, "Example Artist H", 270, 0, null, 0, { addedDaysAgo: 5 }),
  base(111, 41, "Example Artist C", 244, 3, 200, 2, { cases: ["Peak time"] }),
  base(112, 42, "Example Artist I", 210, 0, null, 0),
  base(113, 43, "Example Artist J", 188, 12, 3, 4),
  base(114, 44, "Example Artist D", 160, 0, null, 0, { dupOf: 104 }),
  base(115, 45, "Example Artist K", 142, 1, 410, 0),
  base(116, 46, "Example Artist L", 120, 0, null, 0),
];
const favourites = new Set([113]);

const labelOf = (b: Base) => `${b.artist} - Demo Track ${b.n}`;
const bytesOf = (b: Base) => b.mb * MB + 2_200;
const STORAGE = { total: 63_900 * MB, free: 8 * GB, library: 51 * GB };

function usable() {
  return Math.floor(settings.card_gb * 1e9 / 100) * (100 - Math.min(settings.reserve_percent, 90));
}

function needBytes(): number {
  return settings.goal_kind === "free"
    ? Math.max(0, Math.floor(settings.free_gb * GB) - STORAGE.free)
    : Math.max(0, STORAGE.library - usable());
}

function plan() {
  const r = settings.rules;
  const now = Date.now();
  const need = needBytes();
  const hidden = { favourites: 0, high_rated: 0, new_tracks: 0 };
  const slots = Number(r.never_played || r.stale_months > 0) + Number(r.low_rating_max > 0) + Number(r.duplicates);
  const minReasons = Math.min(r.min_reasons, Math.max(1, slots));
  const reasonsOn = r.never_played || r.stale_months > 0 || r.low_rating_max > 0 || r.duplicates;
  const rulesOn = reasonsOn || r.only_no_case || r.only_no_mix;
  const items: Record<string, any>[] = [];
  for (const b of rulesOn ? pool : []) {
    if ((r.only_no_case && b.cases.length) || (r.only_no_mix && b.mixes.length)) continue;
    const why: { kind: string; text: string }[] = [];
    if (r.never_played && b.plays === 0 && b.daysAgo === null) why.push({ kind: "never_played", text: "Never played" });
    if (r.stale_months > 0 && b.daysAgo !== null && b.daysAgo > r.stale_months * 30.4375) {
      const m = Math.floor(b.daysAgo / 30.4375);
      why.push({ kind: "not_played_recently", text: `Not played for ${m} month${m === 1 ? "" : "s"}` });
    }
    if (r.low_rating_max > 0) {
      if (b.rating >= 1 && b.rating <= r.low_rating_max) why.push({ kind: "low_rating", text: `Rated ${b.rating} star${b.rating === 1 ? "" : "s"}` });
      else if (r.unrated_is_low && b.rating === 0) why.push({ kind: "low_rating", text: "Unrated" });
    }
    const keeper = b.dupOf ? pool.find((x) => x.id === b.dupOf) : undefined;
    if (r.duplicates && keeper) why.push({ kind: "duplicate", text: `Same audio as ${labelOf(keeper)}` });
    if (reasonsOn && why.length < minReasons) continue;
    const reasons = [...why];
    if (r.only_no_case) reasons.push({ kind: "no_case", text: "In no case" });
    if (r.only_no_mix) reasons.push({ kind: "no_mix", text: "In no mix" });
    if (r.protect_favourites && favourites.has(b.id)) { hidden.favourites++; continue; }
    if (r.protect_rating_min > 0 && b.rating >= r.protect_rating_min) { hidden.high_rated++; continue; }
    if (r.protect_new_days > 0 && b.addedDaysAgo < r.protect_new_days) { hidden.new_tracks++; continue; }
    items.push({
      track_id: b.id, label: labelOf(b), format: "FLAC", bytes: bytesOf(b), play_time_secs: 200 + b.n, rating: b.rating,
      times_played: b.plays, last_played_ms: b.daysAgo === null ? null : now - b.daysAgo * DAY, added_ms: now - b.addedDaysAgo * DAY,
      in_cases: b.cases, in_mixes: b.mixes, reasons,
      duplicate_of: keeper && r.duplicates ? keeper.id : null, duplicate_of_label: keeper && r.duplicates ? labelOf(keeper) : null,
      blocked: b.mixes.length ? `Used by the mix ${b.mixes.join(", ")}: it can be archived but not removed` : null,
      suggested: false,
    });
  }
  const certain = (i: Record<string, any>) => i.reasons.filter((x: any) => x.kind !== "no_case" && x.kind !== "no_mix").length;
  items.sort((a, b) => {
    if (!!a.blocked !== !!b.blocked) return a.blocked ? 1 : -1;
    if (r.order === "largest") return b.bytes - a.bytes || a.track_id - b.track_id;
    return certain(b) - certain(a) || (a.last_played_ms ?? -1) - (b.last_played_ms ?? -1) || a.rating - b.rating || b.bytes - a.bytes || a.track_id - b.track_id;
  });
  // a track and the copy that repeats it are never both suggested
  let got = 0, count = 0;
  const picked = new Set<number>(), keepersWithCopy = new Set<number>();
  for (const i of items.filter((x) => !x.blocked)) {
    if (got >= need) break;
    if ((i.duplicate_of !== null && picked.has(i.duplicate_of)) || keepersWithCopy.has(i.track_id)) continue;
    i.suggested = true; got += i.bytes; count++;
    picked.add(i.track_id);
    if (i.duplicate_of !== null) keepersWithCopy.add(i.duplicate_of);
  }
  const removable = items.filter((i) => !i.blocked).reduce((n, i) => n + i.bytes, 0);
  return {
    rules_on: rulesOn, total_tracks: 24 + pool.length, need_bytes: need, free_bytes: STORAGE.free, total_bytes: STORAGE.total,
    library_bytes: STORAGE.library, usable_bytes: settings.goal_kind === "card" ? usable() : null, items,
    candidate_bytes: items.reduce((n, i) => n + i.bytes, 0), removable_bytes: removable, suggested_count: count, suggested_bytes: got,
    reachable: removable >= need, hidden, missing_audio: 0, unchecked: 0, warnings: [] as string[],
  };
}

function preview(ids: number[], dir: string | null, grids: boolean, remove: boolean) {
  const problems: string[] = [];
  const skipped: string[] = [];
  if (!remove && !dir) problems.push("nothing to do: choose an archive folder, remove, or both");
  if (ids.length === 0) problems.push("no tracks are ticked");
  if (dir && dir.toLowerCase().includes("nowhere")) problems.push(`${dir} is not a folder`);
  const items: Record<string, any>[] = [];
  for (const id of [...new Set(ids)]) {
    const b = pool.find((x) => x.id === id);
    if (!b) { skipped.push(`track ${id}: not in the library any more`); continue; }
    const stays = remove && b.mixes.length > 0;
    items.push({
      track_id: id, label: labelOf(b), bytes: bytesOf(b), archive_name: dir ? `${labelOf(b)}.flac` : null,
      remove: remove && !stays, note: stays ? `stays on the card: the mix ${b.mixes.join(", ")} uses it` : null,
    });
  }
  const archiveBytes = dir ? items.reduce((n, i) => n + i.bytes - (grids ? 0 : 2_200), 0) : 0;
  const free = dir ? (dir.toLowerCase().includes("full") ? 200 * MB : 480 * GB) : null;
  if (free !== null && free < archiveBytes + 16 * MB) problems.push(`the archive drive has ${(free / MB).toFixed(1)} MB free and the archive needs ${((archiveBytes + 16 * MB) / MB).toFixed(1)} MB`);
  return {
    items, skipped, archive_bytes: archiveBytes, remove_bytes: items.filter((i) => i.remove).reduce((n, i) => n + i.bytes, 0),
    archive_free_bytes: free, problems,
  };
}

export const handlers: Handlers = {
  device_space_settings: () => ({ settings: structuredClone(settings), warning: null }),

  device_space_settings_set: (a) => {
    settings = structuredClone(a.settings as Settings);
    return { settings: structuredClone(settings), warning: null };
  },

  device_space_plan: (a) => {
    settings = structuredClone(a.settings as Settings);
    if (settings.goal_kind === "free" && String(a.root).toLowerCase().includes("nofree")) {
      throw "invalid: the free space of this drive could not be read, so a free-space target cannot be planned; plan for a card size instead";
    }
    return plan();
  },

  device_space_run_preview: (a) => preview(a.trackIds, a.archiveDir, a.includeGrids, a.remove),

  device_space_run: (call) => {
    const ids = call.trackIds as number[];
    const a = call.args as { archive_dir: string | null; include_grids: boolean; remove: boolean; require_archive: boolean };
    if (ids.length === 0) throw "No tracks are ticked.";
    if (!a.archive_dir && !a.remove) throw "Choose an archive folder, remove from the card, or both.";
    if (a.remove && !a.archive_dir && a.require_archive) {
      throw 'Removing needs an archive copy first. Choose an archive folder, or turn off "Archive before removing" in the planner\'s settings.';
    }
    const p = preview(ids, a.archive_dir, a.include_grids, a.remove);
    if (p.problems.length) throw `invalid: ${p.problems.join("; ")}`;
    const archived = p.items.filter((i) => i.archive_name);
    const removed = p.items.filter((i) => i.remove);
    const labels = [...archived.map((i) => `Archiving ${i.label}`), ...removed.map((i) => `Removing ${i.label}`)];
    const parts: string[] = [];
    if (archived.length) parts.push(`Archived ${archived.length} track${archived.length === 1 ? "" : "s"} (${(p.archive_bytes / MB).toFixed(1)} MB) to ${a.archive_dir}\\pacemaker-archive-20261111T101320Z. Every file was read back and checked; checksums are in MANIFEST.sha256`);
    if (removed.length) parts.push(`Removed ${removed.length} track${removed.length === 1 ? "" : "s"} from the card, freeing ${(p.remove_bytes / MB).toFixed(1)} MB`);
    const stay = p.items.filter((i) => !i.remove && a.remove);
    if (stay.length) parts.push(`${stay.length} item${stay.length === 1 ? "" : "s"} left alone: ${stay.map((i) => `${i.label}: ${i.note}`).join("; ")}`);
    runSteps("device-space", labels, labels.length && removed.length ? "removing" : "archiving", parts.join(". "), () => {
      pool = pool.filter((b) => !removed.some((r) => r.track_id === b.id));
      STORAGE.free += p.remove_bytes;
      STORAGE.library -= p.remove_bytes;
    });
  },

  device_space_open_folder: () => undefined,
};
