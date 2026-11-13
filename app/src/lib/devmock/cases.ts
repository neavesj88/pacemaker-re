// Mock of src-tauri/src/commands/cases.rs. Mutates the shared `cases` fixture so the Cases view,
// the sidebar counts and the library reload all agree. Invented placeholder data only.
import type { CaseInfo, CaseMeta, Query, RefreshItem, RefreshReport, SmartCase, SmartSettings } from "../api/cases";
import { planMerge, planTransfer } from "../state/casePlan";
import { evaluateQuery, referencedCases, validateQuery, type RuleRow } from "../state/smartRules";
import { cases, tracks } from "./db";
import { recordMockUndo } from "./device-trust";
import { mockEnergyLevel, mockLabelOf } from "./tags";
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

const flat = { mode: "all", sort: null, limit: null } as const;
let smartCases: SmartCase[] = [
  { id: 1, name: "Rated 4+", ...flat, rules: [{ kind: "rating_min", stars: 4 }], on_device: false, device_case_id: null },
  { id: 2, name: "Never played", ...flat, rules: [{ kind: "plays_zero" }], on_device: true, device_case_id: null },
  { id: 3, name: "Warm-up keys", ...flat, rules: [{ kind: "bpm_range", min: 120, max: 130 }, { kind: "key_compatible", key: "8A" }], on_device: false, device_case_id: null },
  {
    id: 4,
    name: "Fresh and stale",
    mode: "any",
    rules: [
      { kind: "added_within", days: 60 },
      { kind: "group", mode: "all", items: [{ kind: "last_played_before", days: 10, include_never: false }, { kind: "group", mode: "none", items: [{ kind: "favourite", value: true }, { kind: "text", field: "genre", op: "is", text: "Trance" }] }] },
    ],
    sort: { field: "date_added", descending: true },
    limit: 12,
    on_device: false,
    device_case_id: null,
  },
];

// columns the shared fixture lacks but the rules read (invented, deterministic)
const LABELS = ["Example Label", "Sample Records", ""];
function ruleRows(): RuleRow[] {
  const now = Date.now();
  return tracks.map((t) => ({
    ...t,
    last_played: t.times_played > 0 ? now - (t.track_id % 20) * DAY : -1,
    year: t.track_id % 7 === 0 ? 0 : 1995 + ((t.track_id * 3) % 30),
    date_added: now - t.track_id * 9 * DAY,
    label: LABELS[t.track_id % 3],
    comments: t.track_id % 4 === 0 ? "synthetic note" : "",
    remixer: t.track_id % 5 === 0 ? "Example Remixer" : "",
  }));
}

function checkSmart(name: string, q: Query) {
  if (!name.trim()) throw "The smart case needs a name.";
  const e = validateQuery(q);
  if (e) throw e;
}

/** What the query matches right now (the Rust `evaluate` is the real one). */
function matches(q: Query) {
  const caseSets = new Map<number, Set<number> | null>();
  for (const id of referencedCases(q.rules)) {
    const c = cases.find((x) => x.id === id);
    caseSets.set(id, c ? new Set(c.ids) : null);
  }
  return evaluateQuery(q, ruleRows(), { favourites: new Set(favouriteCase()?.ids ?? []), labelOf: mockLabelOf, energyOf: mockEnergyLevel, cases: caseSets, now: Date.now() });
}

const queryOf = (sc: Query): Query => ({ mode: sc.mode, rules: structuredClone(sc.rules), sort: sc.sort ? { ...sc.sort } : null, limit: sc.limit });

function putOnDevice(sc: SmartCase): number {
  const ids = matches(sc).ids;
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

function selfCheck(sc: SmartCase) {
  if (sc.device_case_id != null && referencedCases(sc.rules).has(sc.device_case_id)) {
    throw "This smart case has a rule that reads its own Pacemaker copy, which is rebuilt from the rules. Remove that rule first.";
  }
}

function takeOffDevice(sc: SmartCase) {
  const i = cases.findIndex((x) => x.id === sc.device_case_id);
  if (i >= 0) cases.splice(i, 1);
  sc.on_device = false;
  sc.device_case_id = null;
}

// the one smart case that starts "on the device" gets its case so the demo is consistent
putOnDevice(smartCases[1]);
// Add ?smartstale to the dev URL to start with that copy out of date, which shows the refresh question.
if (typeof location !== "undefined" && new URLSearchParams(location.search).has("smartstale")) {
  const copy = cases.find((c) => c.id === smartCases[1].device_case_id);
  copy?.ids.pop();
}
for (const c of cases) refresh(c);

// ---- keeping the device copies current, settings, export ------------------------------------------

let settings: SmartSettings = { refresh_mode: "ask", export_smart: true, export_folder: true };

const ref = (id: number) => {
  const t = trackById.get(id);
  return t ? { track_id: id, title: t.title, artist: t.artist } : { track_id: id, title: "(no longer in the library)", artist: "" };
};

/** The Rust `smart_sync::plan`, in TypeScript: what each switched-on smart case's copy should hold. */
function planOne(s: SmartCase): { item: RefreshItem; ids: number[] } {
  const item: RefreshItem = {
    id: s.id, name: s.name, device_name: PREFIX + s.name.trim(), action: "create", before: 0, after: 0,
    added: [], removed: [], reordered: false, renamed_from: null, recreated: s.device_case_id != null, error: null,
  };
  let ids: number[];
  try {
    checkSmart(s.name, s);
    selfCheck(s);
    ids = matches(s).ids;
  } catch (e) {
    return { item: { ...item, action: "skip", recreated: false, error: String(e) }, ids: [] };
  }
  item.after = ids.length;
  const copy = s.device_case_id != null ? cases.find((x) => x.id === s.device_case_id && x.name.startsWith(PREFIX)) : undefined;
  if (!copy) return { item: { ...item, added: ids.map(ref) }, ids };
  const now = copy.ids;
  const inNew = new Set(ids);
  const inNow = new Set(now);
  item.recreated = false;
  item.before = now.length;
  item.added = ids.filter((i) => !inNow.has(i)).map(ref);
  item.removed = now.filter((i) => !inNew.has(i)).map(ref);
  item.reordered = now.filter((i) => inNew.has(i)).join() !== ids.filter((i) => inNow.has(i)).join();
  if (copy.name !== item.device_name) item.renamed_from = copy.name;
  item.action = now.join() === ids.join() && item.renamed_from === null ? "unchanged" : "update";
  return { item, ids };
}

const report = (items: RefreshItem[]): RefreshReport => ({ items, changes: items.filter((i) => i.action === "create" || i.action === "update").length });

/** What the library-wide rekordbox export says about smart cases (the Rust `smart_export::library_xml` notes). */
export function smartLibraryNotes(): string[] {
  if (!settings.export_smart || smartCases.length === 0) return [];
  const names = smartCases.map((s) => `“${s.name}”`).join(", ");
  const copies = smartCases.filter((s) => s.on_device && s.device_case_id != null).map((s) => `“${PREFIX}${s.name}”`);
  return [`Smart cases written as playlists of what they match now: ${names}.` + (copies.length ? ` Their Pacemaker copies (${copies.join(", ")}) were not written a second time.` : "")];
}

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
    // the device area's undo history (devmock/device-trust.ts) is fed by every edit
    const t = tracks.find((x) => x.track_id === a.trackId);
    const who = t ? `${t.artist} - ${t.title}` : `track ${a.trackId}`;
    const show = (b: boolean) => (b ? "in Favourites" : "not in Favourites");
    recordMockUndo("favourite", now ? `Added ${who} to Favourites` : `Removed ${who} from Favourites`,
      [{ track_id: a.trackId, label: who, what: "Favourites", current: show(now), becomes: show(!now), skip: null }],
      () => { c.ids = now ? c.ids.filter((id) => id !== a.trackId) : [...c.ids, a.trackId]; refresh(c); });
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
  cases_transfer_new: (a) => {
    checkName(a.newName);
    if (a.removeFromSource && a.from == null) throw "invalid: a move needs the case the tracks come from";
    if (a.from != null) find(a.from);
    const ids = [...new Set<number>(a.trackIds)].filter(known);
    const dest = create(a.newName, ids);
    let removed = 0;
    if (a.from != null && a.removeFromSource) {
      const src = find(a.from);
      const gone = new Set(ids);
      const before = src.ids.length;
      src.ids = src.ids.filter((id) => !gone.has(id));
      removed = before - src.ids.length;
      refresh(src);
    }
    return { case_id: dest.id, added: ids.length, removed };
  },
  cases_delete_many: (a) => {
    let n = 0;
    const kept: { id: number; name: string; error: string }[] = [];
    for (const it of a.cases as { id: number; name: string }[]) {
      const i = cases.findIndex((c) => c.id === it.id);
      if (i < 0) kept.push({ id: it.id, name: it.name, error: "the case is no longer in the library" });
      else if (cases[i].name !== it.name) kept.push({ id: it.id, name: it.name, error: "kept: the case with this id is now another case" });
      else (cases.splice(i, 1), n++);
    }
    return { deleted: n, kept };
  },
  cases_export: (a) => {
    const c = find(a.caseId);
    if (c.ids.length === 0) throw "This case has no tracks to export.";
    return { path: String(a.dest), tracks: c.ids.length };
  },

  smart_cases_list: () => structuredClone(smartCases),
  smart_cases_counts: () =>
    smartCases.map((s) => {
      try {
        return { id: s.id, count: matches(s).ids.length, error: null };
      } catch (e) {
        return { id: s.id, count: 0, error: String(e) };
      }
    }),
  smart_cases_evaluate: (a) => (checkSmart("-", a.query), matches(a.query)),
  smart_cases_save: (a) => {
    const inc = a.smartCase as SmartCase;
    checkSmart(inc.name, inc);
    const cur = smartCases.find((s) => s.id === inc.id && inc.id !== 0);
    if (cur) {
      Object.assign(cur, queryOf(inc), { name: inc.name.trim() });
      return structuredClone(cur);
    }
    const fresh: SmartCase = { id: Math.max(0, ...smartCases.map((s) => s.id)) + 1, name: inc.name.trim(), ...queryOf(inc), on_device: false, device_case_id: null };
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
    checkSmart(sc.name, sc);
    if (a.on) selfCheck(sc);
    const count = a.on ? putOnDevice(sc) : (takeOffDevice(sc), 0);
    return { smart_case: structuredClone(sc), count, error: null };
  },
  smart_cases_refresh_plan: () => report(smartCases.filter((s) => s.on_device).map((s) => planOne(s).item)),
  smart_cases_refresh_apply: () => {
    const planned = smartCases.filter((s) => s.on_device).map((s) => ({ s, ...planOne(s) }));
    if (!planned.some((p) => p.item.action === "create" || p.item.action === "update")) return report(planned.map((p) => p.item));
    for (const p of planned) if (p.item.action === "create" || p.item.action === "update") putOnDevice(p.s);
    return report(planned.map((p) => p.item));
  },
  smart_cases_export: (a) => {
    const s = smartCases.find((x) => x.id === a.id);
    if (!s) throw `no smart case ${a.id}`;
    const n = matches(s).ids.length;
    if (n === 0) throw `“${s.name}” matches no tracks right now, so there is nothing to export.`;
    return { path: String(a.dest), tracks: n };
  },
  smart_settings_get: () => ({ settings: { ...settings }, warning: null }),
  smart_settings_set: (a) => {
    settings = { ...(a.settings as SmartSettings) };
    return { settings: { ...settings }, warning: null };
  },
};

/** For devmock/casesExport.ts: a saved smart case, what it matches now and the Pacemaker copy it owns. Throws the
 *  sentence the real evaluation gives when a rule cannot run. */
export function smartForExport(id: number): { name: string; ids: number[]; copy: number | null } | undefined {
  const s = smartCases.find((x) => x.id === id);
  if (!s) return undefined;
  checkSmart(s.name, s);
  const copy = s.device_case_id != null && cases.some((c) => c.id === s.device_case_id && c.name.startsWith(PREFIX)) ? s.device_case_id : null;
  return { name: s.name.trim(), ids: matches(s).ids.filter(known), copy };
}
