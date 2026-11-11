// Mock of src-tauri/src/commands/cases.rs. Mutates the shared `cases` fixture so the Cases view,
// the sidebar counts and the library reload all agree. Invented placeholder data only.
import type { CaseInfo, CaseMeta, Rule, SmartCase } from "../api/cases";
import { planMerge, planTransfer } from "../state/casePlan";
import { cases, tracks } from "./db";
import type { Handlers } from "./types";

type MockCase = (typeof cases)[number];

const DAY = 86_400_000;
const FAVOURITES = "Favourites";
const PREFIX = "★ ";

// per-case metadata the shared fixture does not carry
const metaById = new Map<number, Omit<CaseMeta, "name"> & { times_played: number; last_played: number }>([
  [1, { genre: "House", year: 2023, rating: 4, producer: "", remixer: "", comments: "Easy start to the night.", times_played: 8, last_played: Date.now() - 20 * DAY }],
  [2, { genre: "Techno", year: 2024, rating: 5, producer: "", remixer: "", comments: "", times_played: 3, last_played: Date.now() - 3 * DAY }],
  [3, { genre: "", year: 2024, rating: 0, producer: "", remixer: "", comments: "", times_played: 0, last_played: 0 }],
]);

const trackById = new Map(tracks.map((t) => [t.track_id, t]));
const known = (id: number) => trackById.has(id);

function refresh(c: MockCase) {
  c.track_count = c.ids.length;
  c.play_time_secs = c.ids.reduce((s, id) => s + (trackById.get(id)?.play_time_secs ?? 0), 0);
}

function info(c: MockCase): CaseInfo {
  const m = metaById.get(c.id) ?? { genre: "", year: 2024, rating: 0, producer: "", remixer: "", comments: "", times_played: 0, last_played: 0 };
  return { id: c.id, name: c.name, ...m, date_created: 1_700_000_000_000, date_modified: Date.now(), play_time_secs: c.play_time_secs, track_count: c.track_count };
}

function find(id: number): MockCase {
  const c = cases.find((x) => x.id === id);
  if (!c) throw `invalid: no case ${id}`;
  return c;
}

function checkName(name: string) {
  if (!name.trim()) throw "invalid: case name is empty";
  if ([...name].length > 255) throw "invalid: case name is longer than 255 characters";
}

function create(name: string, ids: number[]): MockCase {
  checkName(name);
  for (const id of ids) if (!known(id)) throw `invalid: no track ${id}`;
  const c: MockCase = { id: Math.max(0, ...cases.map((x) => x.id)) + 1, name, play_time_secs: 0, track_count: 0, ids: [...new Set(ids)] };
  refresh(c);
  cases.push(c);
  return c;
}

function favouriteCase(): MockCase | undefined {
  return cases.filter((c) => c.name === FAVOURITES).sort((a, b) => a.id - b.id)[0];
}

// ---- organising: the same plans the Rust side makes (state/casePlan.ts is their twin) ---------------

function mergePlan(sourceIds: number[], target: number | null) {
  const ids = [...new Set<number>(sourceIds)];
  if (ids.length === 0) throw "invalid: pick at least one case to merge";
  if (target != null) {
    find(target);
    if (ids.includes(target)) throw "invalid: a case cannot be merged into itself";
  }
  const lists = ids.map((id) => [...new Set(find(id).ids.filter(known))]);
  const plan = planMerge(lists, target != null ? find(target).ids : []);
  plan.stale = ids.reduce((n, id) => n + new Set(find(id).ids).size, 0) - lists.reduce((n, l) => n + l.length, 0);
  return plan;
}

function transferPlan(from: number | null, to: number, trackIds: number[], remove: boolean) {
  const dest = find(to);
  if (remove && from == null) throw "invalid: a move needs the case the tracks come from";
  if (from === to) throw "invalid: the tracks are already in that case";
  const src = from != null ? find(from).ids.filter(known) : null;
  return planTransfer(trackIds, src, dest.ids, remove, known);
}

// ---- smart cases ---------------------------------------------------------------------------------

let smartCases: SmartCase[] = [
  { id: 1, name: "Rated 4+", rules: [{ kind: "rating_min", stars: 4 }], on_device: false, device_case_id: null },
  { id: 2, name: "Never played", rules: [{ kind: "plays_zero" }], on_device: true, device_case_id: null },
  { id: 3, name: "Warm-up keys", rules: [{ kind: "bpm_range", min: 120, max: 130 }, { kind: "key_compatible", key: "8A" }], on_device: false, device_case_id: null },
];

// pitch class -> Camelot number
const MINOR = [5, 12, 7, 2, 9, 4, 11, 6, 1, 8, 3, 10];
const MAJOR = [8, 3, 10, 5, 12, 7, 2, 9, 4, 11, 6, 1];

function camelot(text: string): { n: number; minor: boolean } | null {
  const t = text.trim();
  let m = /^(\d{1,2})([abAB])$/.exec(t);
  if (m) {
    const n = Number(m[1]);
    return n >= 1 && n <= 12 ? { n, minor: m[2].toLowerCase() === "a" } : null;
  }
  m = /^([A-Ga-g])([#b♯♭]?)\s*(maj|major|min|minor|m)?$/i.exec(t);
  if (!m) return null;
  const base = { C: 0, D: 2, E: 4, F: 5, G: 7, A: 9, B: 11 }[m[1].toUpperCase() as "C"];
  const acc = m[2] === "#" || m[2] === "♯" ? 1 : m[2] ? -1 : 0;
  const pc = (((base + acc) % 12) + 12) % 12;
  const minor = !!m[3] && m[3].toLowerCase().startsWith("m") && m[3].toLowerCase() !== "maj" && m[3].toLowerCase() !== "major";
  return { n: minor ? MINOR[pc] : MAJOR[pc], minor };
}

function compatible(a: { n: number; minor: boolean }, b: { n: number; minor: boolean }) {
  if (a.n === b.n) return true;
  const step = (((a.n - b.n) % 12) + 12) % 12;
  return a.minor === b.minor && (step === 1 || step === 11);
}

function validate(name: string, rules: Rule[]) {
  if (!name.trim()) throw "The smart case needs a name.";
  if (rules.length === 0) throw "Add at least one rule.";
  for (const r of rules) {
    if (r.kind === "rating_min" && !(r.stars >= 1 && r.stars <= 5)) throw "A rating rule needs 1 to 5 stars.";
    if (r.kind === "plays_min" && !(r.n >= 1)) throw "A plays rule needs a count of 1 or more.";
    if (r.kind === "played_within" && !(r.days >= 1)) throw "A last-played rule needs 1 day or more.";
    if (r.kind === "bpm_range") {
      if (r.min == null && r.max == null) throw "A BPM rule needs a minimum, a maximum or both.";
      if (r.min != null && r.max != null && r.min > r.max) throw "The BPM minimum is above the maximum.";
    }
    if ((r.kind === "key" || r.kind === "key_compatible") && !camelot(r.key)) throw `"${r.key}" is not a key (try 8A or Am).`;
    if (r.kind === "genre" && !r.text.trim()) throw "A genre rule needs some text.";
  }
}

function matches(rules: Rule[]): number[] {
  const favs = new Set(favouriteCase()?.ids ?? []);
  const now = Date.now();
  const lastPlayed = (t: (typeof tracks)[number]) => (t.times_played > 0 ? now - (t.track_id % 20) * DAY : -1);
  const ok = (r: Rule, t: (typeof tracks)[number]): boolean => {
    switch (r.kind) {
      case "rating_min": return t.rating >= r.stars;
      case "favourite": return favs.has(t.track_id) === r.value;
      case "plays_zero": return t.times_played === 0;
      case "plays_min": return t.times_played >= r.n;
      case "played_within": return lastPlayed(t) > 0 && lastPlayed(t) >= now - r.days * DAY;
      case "bpm_range": return t.bpm > 0 && (r.min == null || t.bpm >= r.min) && (r.max == null || t.bpm <= r.max);
      case "key": { const a = camelot(r.key), b = camelot(t.key); return !!a && !!b && a.n === b.n && a.minor === b.minor; }
      case "key_compatible": { const a = camelot(r.key), b = camelot(t.key); return !!a && !!b && compatible(a, b); }
      case "genre": return t.genre.toLowerCase().includes(r.text.trim().toLowerCase());
      case "analysed": return t.analysed === r.value;
    }
  };
  return tracks
    .filter((t) => rules.every((r) => ok(r, t)))
    .sort((a, b) => a.artist.toLowerCase().localeCompare(b.artist.toLowerCase()) || a.title.toLowerCase().localeCompare(b.title.toLowerCase()) || a.track_id - b.track_id)
    .map((t) => t.track_id);
}

function putOnDevice(sc: SmartCase): number {
  const ids = matches(sc.rules);
  let c = sc.device_case_id != null ? cases.find((x) => x.id === sc.device_case_id) : undefined;
  if (c) {
    c.name = PREFIX + sc.name.trim();
    c.ids = ids;
    refresh(c);
  } else {
    c = create(PREFIX + sc.name.trim(), ids);
  }
  sc.on_device = true;
  sc.device_case_id = c.id;
  return ids.length;
}

function takeOffDevice(sc: SmartCase) {
  const i = cases.findIndex((x) => x.id === sc.device_case_id);
  if (i >= 0) cases.splice(i, 1);
  sc.on_device = false;
  sc.device_case_id = null;
}

// the one smart case that starts "on the device" gets its case so the demo is consistent
putOnDevice(smartCases[1]);
for (const c of cases) refresh(c);

export const handlers: Handlers = {
  case_tracks: (a) => find(a.caseId).ids,
  cases_list: () => [...cases].sort((x, y) => x.name.localeCompare(y.name)).map(info),
  cases_create: (a) => create(a.name, a.trackIds ?? []).id,
  cases_rename: (a) => {
    checkName(a.name);
    const c = find(a.caseId);
    const changed = c.name !== a.name;
    c.name = a.name;
    return changed;
  },
  cases_set_meta: (a) => {
    checkName(a.meta.name);
    if (a.meta.rating < 0 || a.meta.rating > 5) throw `invalid: rating ${a.meta.rating} is above 5`;
    const c = find(a.caseId);
    const { name, ...rest } = a.meta as CaseMeta;
    c.name = name;
    metaById.set(c.id, { ...(metaById.get(c.id) ?? { times_played: 0, last_played: 0 }), ...rest });
    return true;
  },
  cases_delete: (a) => {
    const i = cases.findIndex((c) => c.id === a.caseId);
    if (i >= 0) cases.splice(i, 1);
    return i >= 0;
  },
  cases_add_tracks: (a) => {
    const c = find(a.caseId);
    for (const id of a.trackIds) if (!known(id)) throw `invalid: no track ${id}`;
    const fresh = [...new Set<number>(a.trackIds)].filter((id) => !c.ids.includes(id));
    c.ids.push(...fresh);
    refresh(c);
    return fresh.length;
  },
  cases_remove_tracks: (a) => {
    const c = find(a.caseId);
    const before = c.ids.length;
    c.ids = c.ids.filter((id) => !a.trackIds.includes(id));
    refresh(c);
    return before - c.ids.length;
  },
  cases_set_tracks: (a) => {
    const c = find(a.caseId);
    for (const id of a.trackIds) if (!known(id)) throw `invalid: no track ${id}`;
    c.ids = [...new Set<number>(a.trackIds)];
    refresh(c);
  },
  cases_toggle_favourite: (a) => {
    if (!known(a.trackId)) throw `invalid: no track ${a.trackId}`;
    const c = favouriteCase() ?? create(FAVOURITES, []);
    const now = !c.ids.includes(a.trackId);
    c.ids = now ? [...c.ids, a.trackId] : c.ids.filter((id) => id !== a.trackId);
    refresh(c);
    return now;
  },
  cases_favourite_ids: () => [...(favouriteCase()?.ids ?? [])],

  // ---- organising ----
  cases_duplicate: (a) => {
    checkName(a.name);
    const src = find(a.caseId);
    const copy = create(a.name, src.ids.filter(known)); // like the real one, a row for a deleted track is not copied
    const m = metaById.get(src.id);
    if (m) metaById.set(copy.id, { ...m, times_played: 0, last_played: 0 });
    return copy.id;
  },
  cases_merge_preview: (a) => mergePlan(a.sourceIds, a.target),
  cases_merge: (a) => {
    const plan = mergePlan(a.sourceIds, a.target);
    let dest: MockCase;
    if (a.target != null) dest = find(a.target);
    else if (a.newName != null) dest = create(a.newName, []);
    else throw "Pick either an existing case or a name for a new one.";
    dest.ids.push(...plan.added);
    refresh(dest);
    let deleted = 0;
    if (a.deleteSources) {
      for (const id of new Set<number>(a.sourceIds)) {
        const i = cases.findIndex((c) => c.id === id);
        if (i >= 0) (cases.splice(i, 1), deleted++);
      }
    }
    return { case_id: dest.id, added: plan.added.length, deleted_sources: deleted };
  },
  cases_transfer_preview: (a) => transferPlan(a.from, a.to, a.trackIds, a.removeFromSource),
  cases_transfer: (a) => {
    const plan = transferPlan(a.from, a.to, a.trackIds, a.removeFromSource);
    const dest = find(a.to);
    dest.ids.push(...plan.to_add);
    refresh(dest);
    if (a.from != null && plan.to_remove.length > 0) {
      const src = find(a.from);
      const gone = new Set(plan.to_remove);
      src.ids = src.ids.filter((id) => !gone.has(id));
      refresh(src);
    }
    return { added: plan.to_add.length, removed: plan.to_remove.length };
  },
  cases_delete_many: (a) => {
    let n = 0;
    for (const id of new Set<number>(a.caseIds)) {
      const i = cases.findIndex((c) => c.id === id);
      if (i >= 0) (cases.splice(i, 1), n++);
    }
    return n;
  },
  cases_export: (a) => {
    const c = find(a.caseId);
    if (c.ids.length === 0) throw "This case has no tracks to export.";
    return { path: String(a.dest), tracks: c.ids.length };
  },

  smart_cases_list: () => structuredClone(smartCases),
  smart_cases_counts: () => smartCases.map((s) => ({ id: s.id, count: matches(s.rules).length })),
  smart_cases_evaluate: (a) => (validate("-", a.rules), matches(a.rules)),
  smart_cases_save: (a) => {
    const inc = a.smartCase as SmartCase;
    validate(inc.name, inc.rules);
    const cur = smartCases.find((s) => s.id === inc.id && inc.id !== 0);
    if (cur) {
      cur.name = inc.name.trim();
      cur.rules = structuredClone(inc.rules);
      return structuredClone(cur);
    }
    const fresh: SmartCase = { id: Math.max(0, ...smartCases.map((s) => s.id)) + 1, name: inc.name.trim(), rules: structuredClone(inc.rules), on_device: false, device_case_id: null };
    smartCases.push(fresh);
    return structuredClone(fresh);
  },
  smart_cases_delete: (a) => {
    const sc = smartCases.find((s) => s.id === a.id);
    if (!sc) return false;
    let deleted = false;
    if (a.removeDeviceCase && sc.device_case_id != null) {
      takeOffDevice(sc);
      deleted = true;
    }
    smartCases = smartCases.filter((s) => s.id !== a.id);
    return deleted;
  },
  smart_cases_set_on_device: (a) => {
    const sc = smartCases.find((s) => s.id === a.id);
    if (!sc) throw `no smart case ${a.id}`;
    validate(sc.name, sc.rules);
    const count = a.on ? putOnDevice(sc) : (takeOffDevice(sc), 0);
    return { smart_case: structuredClone(sc), count };
  },
  smart_cases_refresh_all: () => smartCases.filter((s) => s.on_device).map((s) => ({ count: putOnDevice(s), smart_case: structuredClone(s) })),
};
