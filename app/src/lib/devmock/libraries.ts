// Mock of the Libraries view's commands (src-tauri/src/commands/library/pane.rs and the selection
// half of commands/device/sync.rs). Invented data only.
//
// Two libraries live in memory: the one on this computer (the mock library every other area uses:
// db.ts tracks, mixes.ts mixes, db.ts cases) and the Pacemaker at P:\ (the first ten of those
// tracks, so some are "on both", plus thirty of its own, and its own mixes and cases). A copy
// between them really adds the items to the target, so both panes and the open library show it.
// The plan follows the engine's rules in small: what the target has is "already the same", a mix or
// case pulls its tracks (marked needed-by), tracks dropped on a case go into it, and a track whose
// .str is missing is skipped with the reason (the mock's "needs analysis" tracks 11 and 18).
//
// Opening P:\ (library.ts) gives this separate Pacemaker library, which has no needs-analysis
// tracks, so the preview's Pacemaker cannot show the re-analyse states the computer library does.
//
// `?big=1` makes the Pacemaker 30 000 tracks long.
// Picked up automatically by devmock/index.ts; library.ts reads `libraryFor` for open_library.
import { libraryKind } from "../state/place";
import { cases as demoCases, needs, tracks as demoTracks } from "./db";
import { runSteps } from "./device";
import { mixes as demoMixes } from "./mixes";
import type { Handlers } from "./types";

const MB = 1024 * 1024;
const SAME = "already the same";
const GONE = "it is not in the sending library any more";

type MTrack = (typeof demoTracks)[number];
type Coll = { id: number; name: string; play_time_secs: number; track_count: number };
type MColl = Coll & { ids: number[] };
export type MockLib = { tracks: MTrack[]; mixes: MColl[]; cases: MColl[] };

const norm = (s: string) => s.trim().toLowerCase();
const label = (t: MTrack) => [t.artist, t.title].filter(Boolean).join(" - ");

// the computer: the shared mock library. Mixes carry no track list there, so they get a stable one.
const computer: MockLib = {
  tracks: demoTracks,
  get mixes() {
    return demoMixes.map((m) => ({ ...m, ids: [(m.id * 3) % 24 + 1, (m.id * 3 + 1) % 24 + 1, (m.id * 3 + 2) % 24 + 1] }));
  },
  cases: demoCases as MColl[],
};

// the Pacemaker (`?big=1` in the address gives it 30 000 tracks, to see the panes at that size)
const BIG = typeof location !== "undefined" && new URLSearchParams(location.search).get("big") === "1";
const pmOnly: MTrack[] = Array.from({ length: BIG ? 30_000 : 30 }, (_, i) => ({
  ...demoTracks[i % 24],
  track_id: 101 + i,
  title: `Pacemaker Track ${String(i + 1).padStart(2, "0")}`,
  artist: `Sample Band ${String.fromCharCode(65 + (i % 5))}`,
  location: `/pmdb_tracks/${i % 10}/${(i * 5) % 16}/pm${i}.mp3`,
  analysed: true,
}));
const pm: MockLib = {
  tracks: [...demoTracks.slice(0, 10).map((t) => ({ ...t })), ...pmOnly],
  mixes: [
    { id: 1, name: "Example Mix", play_time_secs: 1800, track_count: 3, ids: [1, 2, 3] },
    { id: 2, name: "Saturday set", play_time_secs: 2700, track_count: 4, ids: [101, 102, 103, 5] },
    { id: 3, name: "Cooldown", play_time_secs: 900, track_count: 2, ids: [104, 105] },
  ],
  cases: [
    { id: 1, name: "Warm-up", play_time_secs: 3600, track_count: 4, ids: [1, 2, 3, 101] },
    { id: 2, name: "Pacemaker faves", play_time_secs: 1500, track_count: 5, ids: [101, 102, 106, 107, 4] },
    { id: 3, name: "Crate A", play_time_secs: 0, track_count: 0, ids: [] },
  ],
};
// tracks the Pacemaker has with other details: a drag leaves them as they are and says so
const UPDATES = new Set([demoTracks[4].location, demoTracks[5].location]);

// tracks the Pacemaker holds a row for but not all of the files: a drag from the computer repairs them
const REPAIRS = new Map<string, "audio" | "side">([[demoTracks[2].location, "audio"], [demoTracks[7].location, "side"]]);
const REPAIR_WORDS = { audio: "the audio file is missing on the target", side: "the beat grid file (.str) is missing on the target" };

export const libraryFor = (root: string): MockLib => (libraryKind(root) === "pacemaker" ? pm : computer);
export const stripIds = (c: MColl): Coll => ({ id: c.id, name: c.name, play_time_secs: c.play_time_secs, track_count: c.track_count });

// ---- the plan ----------------------------------------------------------------------------------

type Action = { type: string; name?: string; reason?: string };
type Need = { location: string; label: string };
type Item = {
  key: string; kind: "track" | "mix" | "case"; action: Action; label: string; bytes: number; needs: Need[]; detail: string;
  global_id: string | null; location: string | null; source_id: number | null; target_id: number | null;
  blockers: { key: string; label: string }[]; needed_by: string[]; into_case: string | null;
};
type Sel = { tracks: string[]; mixes: string[]; cases: string[]; into_case: { name: string; create: boolean } | null };

const blank = (key: string, kind: Item["kind"], lbl: string, action: Action): Item => ({
  key, kind, action, label: lbl, bytes: 0, needs: [], detail: "", global_id: null, location: null, source_id: null, target_id: null,
  blockers: [], needed_by: [], into_case: null,
});
const skip = (reason: string): Action => ({ type: "skip", reason });
const plural = (n: number, one: string) => `${n} ${one}${n === 1 ? "" : "s"}`;
const hash = (s: string) => {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 16777619);
  return (h >>> 0).toString(16);
};

function planOf(a: Record<string, any>) {
  const S = libraryFor(String(a.source));
  const T = libraryFor(String(a.target));
  if (S === T) throw "invalid: the source and the target are the same library";
  const sel = a.selection as Sel;
  const sByLoc = new Map(S.tracks.map((t) => [t.location, t]));
  const tByLoc = new Map(T.tracks.map((t) => [t.location, t]));
  const sById = new Map(S.tracks.map((t) => [t.track_id, t]));
  const items: Item[] = [];

  const picked = [...new Set(sel.tracks)];
  const needed = new Map<string, string[]>();
  const all = [...picked];
  const pull = (ids: number[], by: string) => {
    for (const id of ids) {
      const t = sById.get(id);
      if (!t) continue;
      if (!all.includes(t.location)) all.push(t.location);
      const list = needed.get(t.location) ?? [];
      if (!list.includes(by)) list.push(by);
      needed.set(t.location, list);
    }
  };
  const mixNames = [...new Set(sel.mixes.map(norm))];
  const caseNames = [...new Set(sel.cases.map(norm))];
  for (const n of mixNames) {
    const m = S.mixes.find((x) => norm(x.name) === n);
    if (m) pull(m.ids, `mix "${m.name}"`);
  }
  for (const n of caseNames) {
    const c = S.cases.find((x) => norm(x.name) === n);
    if (c) pull(c.ids, `case "${c.name}"`);
  }

  // tracks
  const byKey = new Map<string, Item>();
  for (const loc of all) {
    const s = sByLoc.get(loc);
    const key = `track:${loc}`;
    let it: Item;
    if (!s) {
      it = blank(key, "track", loc.split("/").pop() ?? loc, skip(GONE));
    } else {
      it = blank(key, "track", label(s), { type: "copy" });
      it.location = loc;
      it.source_id = s.track_id;
      it.global_id = `mock-global-${s.track_id}`;
      const t = tByLoc.get(loc);
      const reason = S === computer ? needs.get(s.track_id) : undefined;
      if (!t) {
        if (reason && reason !== "never analysed") it.action = skip(reason.startsWith("no .str") ? "its analysis file (.str) is missing" : "its analysis file (.str) is empty");
        else it.bytes = s.file_size + 4096;
      } else {
        it.target_id = t.track_id;
        // the engine never changes a track the target has, even with other details (drag_settings)
        it.action = UPDATES.has(loc) && S === computer ? skip("it is already there, and a drag never changes a track on the target") : skip(SAME);
        const lack = S === computer && T === pm ? REPAIRS.get(loc) : undefined;
        if (lack) {
          // only the missing file is copied: the row, its grid and its plays stay as they are
          it.action = { type: "repair" };
          it.detail = REPAIR_WORDS[lack];
          it.bytes = lack === "audio" ? s.file_size : 4096;
        }
      }
      if (!picked.includes(loc)) it.needed_by = needed.get(loc) ?? [];
    }
    items.push(it);
    byKey.set(loc, it);
  }
  const willExist = (loc: string) => tByLoc.has(loc) || byKey.get(loc)?.action.type === "copy";
  const needsOf = (ids: number[]): Need[] => {
    const out: Need[] = [];
    for (const id of ids) {
      const t = sById.get(id);
      if (t && !out.some((n) => n.location === t.location)) out.push({ location: t.location, label: label(t) });
    }
    return out;
  };

  // mixes
  for (const n of mixNames) {
    const m = S.mixes.find((x) => norm(x.name) === n);
    if (!m) {
      items.push(blank(`mix:${n}`, "mix", n, skip(GONE)));
      continue;
    }
    const it = blank(`mix:${n}`, "mix", m.name, { type: "copy" });
    it.source_id = m.id;
    it.needs = needsOf(m.ids);
    const t = T.mixes.find((x) => norm(x.name) === n);
    if (t) {
      it.target_id = t.id;
      it.action = skip(SAME);
    }
    const missing = it.needs.filter((x) => !willExist(x.location)).map((x) => x.label);
    if (missing.length && it.action.type === "copy") it.action = skip(`it needs ${missing.slice(0, 3).map((x) => `"${x}"`).join(", ")}${missing.length > 3 ? ` and ${missing.length - 3} more` : ""}, which will not be on the target`);
    items.push(it);
  }

  // cases
  for (const n of caseNames) {
    const c = S.cases.find((x) => norm(x.name) === n);
    if (!c) {
      items.push(blank(`case:${n}`, "case", n, skip(GONE)));
      continue;
    }
    const it = blank(`case:${n}`, "case", c.name, { type: "copy" });
    it.source_id = c.id;
    it.needs = needsOf(c.ids);
    const avail = it.needs.filter((x) => willExist(x.location));
    if (avail.length < it.needs.length) it.detail = `${it.needs.length - avail.length} of ${it.needs.length} tracks will not be there and are left out`;
    const t = T.cases.find((x) => norm(x.name) === n);
    if (t) {
      it.target_id = t.id;
      const have = new Set(t.ids.map((id) => T.tracks.find((x) => x.track_id === id)?.location));
      it.action = avail.every((x) => have.has(x.location)) ? skip(SAME) : { type: "merge" };
    }
    items.push(it);
  }

  // the case the picked tracks go into
  if (sel.into_case) {
    const name = sel.into_case.name.trim();
    const needsHere: Need[] = picked.flatMap((loc) => {
      const s = sByLoc.get(loc);
      return s ? [{ location: loc, label: label(s) }] : [];
    });
    const avail = needsHere.filter((x) => willExist(x.location));
    const t = T.cases.find((x) => norm(x.name) === norm(name));
    let it: Item;
    if (t) {
      const have = new Set(t.ids.map((id) => T.tracks.find((x) => x.track_id === id)?.location));
      const fresh = avail.filter((x) => !have.has(x.location)).length;
      it = blank(`into-case:${norm(name)}`, "case", `Add ${plural(fresh || avail.length, "track")} to case ${name}`, fresh ? { type: "merge" } : skip(SAME));
      it.target_id = t.id;
    } else if (sel.into_case.create) {
      it = blank(`into-case:${norm(name)}`, "case", `New case ${name} with ${plural(avail.length, "track")}`, { type: "copy" });
    } else {
      it = blank(`into-case:${norm(name)}`, "case", `Add ${plural(avail.length, "track")} to case ${name}`, skip("the case is not on the target any more"));
    }
    if (avail.length === 0 && it.action.type !== "skip") it.action = skip("none of the tracks will be on the target");
    it.into_case = name;
    it.needs = needsHere;
    if (avail.length < needsHere.length) it.detail = `${needsHere.length - avail.length} of ${needsHere.length} tracks will not be there and are left out`;
    items.push(it);
  }

  const bytes = items.filter((i) => i.kind === "track" && i.action.type !== "skip").reduce((n, i) => n + i.bytes, 0);
  const full = String(a.target).toLowerCase().includes("full");
  const free = full ? 5 * MB : T === computer ? 212_000 * MB : 11_400 * MB;
  const digest = `mock-sel-${hash(JSON.stringify([sel, items.map((i) => [i.key, i.action, i.bytes, i.source_id, i.target_id])]))}`;
  return {
    items, settings: { tracks_on_both: "newer", mixes_same_name: "newer", cases_same_name: "merge", remove_missing: false, play_counts: "higher" },
    source_label: S === computer ? "computer" : "Pacemaker", bytes_to_copy: bytes, free_bytes: free, fits: bytes === 0 || bytes + MB <= free,
    digest, notes: ["Smart cases are kept by the app on this computer and are not synced."], S, T, sel,
  };
}

// ---- the run -----------------------------------------------------------------------------------

function apply(p: ReturnType<typeof planOf>) {
  const { S, T, items } = p;
  const sById = new Map(S.tracks.map((t) => [t.track_id, t]));
  const nextId = (list: { track_id?: number; id?: number }[], key: "track_id" | "id") => Math.max(0, ...list.map((x) => x[key] ?? 0)) + 1;
  const tIdAt = (loc: string) => T.tracks.find((t) => t.location === loc)?.track_id;
  const sIds = (ids: number[]) =>
    ids.flatMap((id) => {
      const loc = sById.get(id)?.location;
      const t = loc ? tIdAt(loc) : undefined;
      return t === undefined ? [] : [t];
    });
  for (const it of items) {
    if (it.action.type === "skip") continue;
    if (it.kind === "track") {
      const s = S.tracks.find((t) => t.track_id === it.source_id);
      if (s && it.action.type === "copy") T.tracks.push({ ...s, track_id: nextId(T.tracks, "track_id") });
      if (s && it.action.type === "repair") REPAIRS.delete(s.location);
    } else if (it.into_case !== null) {
      const ids = it.needs.map((n) => tIdAt(n.location)).filter((x): x is number => x !== undefined);
      const t = T.cases.find((c) => norm(c.name) === norm(it.into_case!));
      if (t) {
        t.ids = [...t.ids, ...ids.filter((id) => !t.ids.includes(id))];
        t.track_count = t.ids.length;
      } else T.cases.push({ id: nextId(T.cases as any, "id"), name: it.into_case, play_time_secs: 0, track_count: ids.length, ids });
    } else if (it.kind === "mix") {
      const m = S.mixes.find((x) => x.id === it.source_id);
      if (m) {
        const ids = sIds(m.ids);
        addMix(T, { id: nextId(T.mixes as any, "id"), name: m.name, play_time_secs: m.play_time_secs, track_count: ids.length, ids });
      }
    } else {
      const c = S.cases.find((x) => x.id === it.source_id);
      if (c) {
        const ids = sIds(c.ids);
        const t = T.cases.find((x) => norm(x.name) === norm(c.name));
        if (t) {
          t.ids = [...t.ids, ...ids.filter((id) => !t.ids.includes(id))];
          t.track_count = t.ids.length;
        } else T.cases.push({ id: nextId(T.cases as any, "id"), name: c.name, play_time_secs: c.play_time_secs, track_count: ids.length, ids });
      }
    }
  }
}

/** The computer's mix list is the shared mock array (and `computer.mixes` is a fresh read of it each time); the Pacemaker's is ours. */
function addMix(T: MockLib, m: MColl) {
  if (T === computer) demoMixes.push(stripIds(m));
  else T.mixes.push(m);
}

function summary(p: ReturnType<typeof planOf>) {
  const run = p.items.filter((i) => i.action.type !== "skip");
  const bytes = run.filter((i) => i.kind === "track").reduce((n, i) => n + i.bytes, 0);
  const count = (kind: Item["kind"], type: string) => run.filter((i) => i.kind === kind && i.into_case === null && i.action.type === type).length;
  const parts: string[] = [];
  const tc = count("track", "copy"), mc = count("mix", "copy"), cc = count("case", "copy");
  if (tc + mc + cc > 0) {
    const what = [tc ? `${plural(tc, "track")} (${(bytes / MB).toFixed(1)} MB)` : "", mc ? plural(mc, "mix").replace("mixs", "mixes") : "", cc ? plural(cc, "case") : ""].filter(Boolean);
    parts.push(`Copied ${what.join(", ")}`);
  }
  if (count("track", "update")) parts.push(`Updated ${plural(count("track", "update"), "track")}`);
  if (count("track", "repair")) parts.push(`Repaired the missing files of ${plural(count("track", "repair"), "track")}`);
  if (count("case", "merge")) parts.push(`Merged ${plural(count("case", "merge"), "case")}`);
  for (const i of run.filter((x) => x.into_case !== null)) parts.push(i.action.type === "copy" ? `Made new case ${i.label.slice("New case ".length)}` : `Added ${i.label.slice("Add ".length)}`);
  return parts.join(". ") || "Nothing to copy, all of it is there already";
}

export const handlers: Handlers = {
  library_pane: (a) => {
    const root = String(a.root);
    const lib = libraryFor(root);
    return { root, kind: libraryKind(root), tracks: lib.tracks, mixes: lib.mixes.map(stripIds), cases: lib.cases.map(stripIds) };
  },

  sync_plan_selection: (a) => {
    const { S: _s, T: _t, sel: _sel, ...plan } = planOf(a);
    return plan;
  },

  sync_apply_selection: (a) => {
    const p = planOf(a);
    if (p.digest !== a.digest) throw "stale plan: the libraries changed since the preview, so nothing was synced. Look at the preview again.";
    const run = p.items.filter((i) => i.action.type !== "skip");
    const bytes = run.filter((i) => i.kind === "track").reduce((n, i) => n + i.bytes, 0);
    runSteps("device-sync", run.length ? run.map((i) => i.label) : ["Checking"], "syncing", summary(p), () => apply(p), (d, t) => `${((bytes * d) / Math.max(t, 1) / MB).toFixed(1)} of ${(bytes / MB).toFixed(1)} MB`);
  },
};
