// Mock of the trust-pack commands in src-tauri/src/commands/device/{trust,undo}.rs: restore from a
// snapshot, Health card fixes, undo history, settings. Invented data only. Picked up
// automatically by devmock/index.ts. (The backup verification job lives in devmock/device.ts next
// to the other device jobs.)
//
// The edit commands of other areas feed the undo history through `recordMockUndo` (devmock/
// metadata.ts and cases.ts call it), so rating a track in the browser preview really shows up
// here and really undoes. Three demo entries are there from the start.
import { emit } from "@tauri-apps/api/event";
import { cases, needs, tracks } from "./db";
import type { Handlers } from "./types";

const KEY = "demo-volume";
const label = (t: { title: string; artist: string }) => `${t.artist} - ${t.title}`;
const trackLabel = (id: number) => {
  const t = tracks.find((x) => x.track_id === id);
  return t ? label(t) : `Demo Track ${id}`;
};

// ---- settings --------------------------------------------------------------------------------

let settings = { undo_limit: 20, dead_row_guard_percent: 50, undo_pill_seconds: 10 };
const MAX_UNDO = 200;
const settingsView = () => ({ settings: { ...settings }, max_undo_limit: MAX_UNDO, warning: null as string | null });

// ---- undo history ----------------------------------------------------------------------------

type PlanItem = {
  entry_id: number; track_id: number; label: string; what: string; current: string; becomes: string; skip: string | null;
};
type MockUndo = {
  id: number; kind: "rating" | "metadata" | "favourite"; at_ms: number; summary: string; tracks: number;
  items: Omit<PlanItem, "entry_id">[];
  revert: () => void;
};
let undoStack: MockUndo[] = []; // oldest first
let nextUndoId = 1;

/** Called by the mock edit commands of other areas. */
export function recordMockUndo(
  kind: MockUndo["kind"],
  summary: string,
  items: Omit<PlanItem, "entry_id">[],
  revert: () => void,
) {
  if (settings.undo_limit === 0) return;
  const id = nextUndoId++;
  undoStack.push({ id, kind, at_ms: Date.now(), summary, tracks: new Set(items.map((i) => i.track_id)).size, items, revert });
  while (undoStack.length > settings.undo_limit) undoStack.shift();
  void emit("undo-changed", { key: KEY, why: "recorded", summary, id });
}

const seed = (minutesAgo: number, kind: MockUndo["kind"], summary: string, items: Omit<PlanItem, "entry_id">[]) => {
  undoStack.push({
    id: nextUndoId++, kind, at_ms: Date.now() - minutesAgo * 60_000, summary,
    tracks: new Set(items.map((i) => i.track_id)).size, items, revert: () => {},
  });
};
const item = (id: number, what: string, current: string, becomes: string): Omit<PlanItem, "entry_id"> => ({
  track_id: id, label: trackLabel(id), what, current, becomes, skip: null,
});
seed(42, "metadata", "Edited 3 tracks: genre", [
  item(3, "Genre", "Techno", "House"), item(5, "Genre", "Techno", "Trance"), item(8, "Genre", "Techno", "House"),
]);
seed(18, "favourite", `Added ${trackLabel(12)} to Favourites`, [item(12, "Favourites", "in Favourites", "not in Favourites")]);
seed(4, "rating", `Rated ${trackLabel(2)} 4 stars (was 1 star)`, [item(2, "Rating", "4 stars", "1 star")]);

const undoView = () => ({
  key: KEY,
  limit: settings.undo_limit,
  entries: [...undoStack].reverse().map(({ id, kind, at_ms, summary, tracks }) => ({ id, kind, at_ms, summary, tracks })),
});

const top = (count: number) => {
  const picked = [...undoStack].reverse().slice(0, count);
  if (count < 1 || picked.length === 0) throw "There is nothing to undo.";
  if (picked.length < count) throw `Only ${picked.length} edit${picked.length === 1 ? "" : "s"} can be undone.`;
  return picked;
};

// ---- snapshots -------------------------------------------------------------------------------

const zeroCounts = {
  tracks_returning: 0, tracks_lost: 0, tracks_changed: 0, cases_returning: 0, cases_lost: 0, cases_changed: 0,
  mixes_returning: 0, mixes_lost: 0, mixes_changed: 0,
};
type Counts = typeof zeroCounts;
type Row = {
  name: string; stamp: string; problem: string | null;
  snapshot: { tracks: number; cases: number; mixes: number } | null; counts: Counts | null;
};
const SNAP_DIR = "C:\\Users\\demo\\AppData\\Local\\pacemaker-revival\\snapshots\\E";

let rows: Row[] = [
  {
    name: "music-db-20261111T094512Z", stamp: "20261111T094512Z", problem: null,
    snapshot: { tracks: 22, cases: 3, mixes: 2 },
    counts: { ...zeroCounts, tracks_lost: 2, tracks_changed: 3, mixes_changed: 1 },
  },
  {
    name: "music-db-20261108T201133Z", stamp: "20261108T201133Z", problem: null,
    snapshot: { tracks: 20, cases: 2, mixes: 1 },
    counts: { ...zeroCounts, tracks_lost: 4, tracks_returning: 1, tracks_changed: 6, cases_lost: 1, mixes_lost: 1 },
  },
  {
    name: "music-db-20261030T081500Z", stamp: "20261030T081500Z", snapshot: null, counts: null,
    problem: "this snapshot was taken while the card had an unfinished write (it holds a music.db-journal), so it cannot be restored safely",
  },
];

const listed = (items: { id: number; label: string; note?: string | null }[]) => ({
  total: items.length, items: items.map((i) => ({ id: i.id, label: i.label, note: i.note ?? null })),
});
const ref = (id: number, note?: string) => ({ id, label: trackLabel(id), note: note ?? null });

let restores = 0;

function preview(name: string) {
  const row = rows.find((r) => r.name === name);
  if (!row || row.problem || !row.counts || !row.snapshot) throw `invalid: ${row?.problem ?? `there is no snapshot called ${name}`}`;
  const c = row.counts;
  const lostIds = [24, 23, 22, 21].slice(0, c.tracks_lost);
  const changedIds = [3, 5, 8, 11, 13, 16].slice(0, c.tracks_changed);
  const back = c.tracks_returning ? [ref(30, "its audio file is not on the card")] : [];
  return {
    snapshot_name: name,
    snapshot: row.snapshot,
    current: { tracks: row.snapshot.tracks + c.tracks_lost - c.tracks_returning, cases: row.snapshot.cases - c.cases_returning + c.cases_lost, mixes: row.snapshot.mixes - c.mixes_returning + c.mixes_lost },
    current_problem: null,
    counts: c,
    tracks_returning: listed(back.map((r) => ({ ...r, label: "Example Artist H - Removed Demo Track" }))),
    tracks_lost: listed(lostIds.map((id) => ref(id))),
    tracks_changed: listed(changedIds.map((id) => ref(id))),
    cases_returning: listed([]),
    cases_lost: listed(c.cases_lost ? [{ id: 4, label: "Late night" }] : []),
    cases_changed: listed([]),
    mixes_returning: listed([]),
    mixes_lost: listed(c.mixes_lost ? [{ id: 2, label: "Demo Mix 2" }] : []),
    mixes_changed: listed(c.mixes_changed ? [{ id: 1, label: "Demo Mix 1" }] : []),
    fingerprint: `fp-${name}-${restores}`,
  };
}

// ---- Health card fixes -----------------------------------------------------------------------

type Dead = { track_id: number; label: string; location: string; in_cases: string[]; in_mixes: string[] };
const dead = (id: number, in_cases: string[], in_mixes: string[]): Dead => ({
  track_id: id, label: trackLabel(id), location: tracks.find((t) => t.track_id === id)?.location ?? `/pmdb_tracks/0/0/demo${id}.mp3`, in_cases, in_mixes,
});
let deadRows: Dead[] = [dead(7, ["Warm-up"], []), dead(15, [], [])];
const kept: Dead[] = [dead(9, [], ["Demo Mix 1"])];

/** The rows the Health card's "missing audio" line counts (used by device_health in device.ts). */
export const missingAudioRows = () => [...deadRows, ...kept].map((d) => ({ track_id: d.track_id, location: d.location }));

const massMode = (root: unknown) => String(root ?? "").toLowerCase().includes("mass");

function plan(root: unknown) {
  const total = massMode(root) ? 6 : tracks.length;
  const deadCount = deadRows.length + kept.length;
  return {
    music_folder_ok: true,
    total_tracks: total,
    dead_rows: deadRows,
    kept_for_mixes: kept,
    unchecked: [] as { track_id: number; label: string; location: string }[],
    grids: [...needs.entries()]
      .filter(([id]) => !deadRows.some((d) => d.track_id === id) && tracks.some((t) => t.track_id === id))
      .map(([id, reason]) => ({ track_id: id, label: trackLabel(id), reason })),
    guard_percent: settings.dead_row_guard_percent,
    mass: deadCount * 100 > settings.dead_row_guard_percent * total,
  };
}

export const handlers: Handlers = {
  device_trust_settings: () => settingsView(),
  device_trust_settings_set: (a) => {
    const s = a.settings as typeof settings;
    settings = {
      undo_limit: Math.min(MAX_UNDO, Math.max(0, Math.round(s.undo_limit))),
      dead_row_guard_percent: Math.min(100, Math.max(1, Math.round(s.dead_row_guard_percent))),
      undo_pill_seconds: Math.min(120, Math.max(0, Math.round(s.undo_pill_seconds))),
    };
    if (undoStack.length > settings.undo_limit) undoStack = undoStack.slice(undoStack.length - settings.undo_limit);
    void emit("undo-changed", { key: KEY, why: "limit", summary: null, id: 0 });
    return settingsView();
  },

  device_snapshot_rows: () => ({
    folder: SNAP_DIR,
    keep: 10,
    rows: rows.map((r) => ({ ...r, folder: `${SNAP_DIR}\\${r.name}`, files: ["music.db", "settings.dat"], music_db_bytes: 1_843_200 })),
  }),
  device_snapshot_preview: (a) => preview(String(a.name)),
  device_restore_snapshot: (a) => {
    const name = String(a.name);
    const p = preview(name);
    if (a.fingerprint !== p.fingerprint) {
      throw "invalid: the library on the card or the snapshot changed since the preview was made; review the differences again before restoring";
    }
    const row = rows.find((r) => r.name === name)!;
    const c = row.counts!;
    // the safety copy holds what the card had: the inverse of the differences
    const safety: Row = {
      name: "music-db-20261111T113000Z", stamp: "20261111T113000Z", problem: null,
      snapshot: p.current,
      counts: {
        tracks_returning: c.tracks_lost, tracks_lost: c.tracks_returning, tracks_changed: c.tracks_changed,
        cases_returning: c.cases_lost, cases_lost: c.cases_returning, cases_changed: c.cases_changed,
        mixes_returning: c.mixes_lost, mixes_lost: c.mixes_returning, mixes_changed: c.mixes_changed,
      },
    };
    restores++;
    row.counts = { ...zeroCounts };
    rows = [safety, ...rows.filter((r) => r.name !== safety.name)];
    undoStack = [];
    void emit("undo-changed", { key: KEY, why: "cleared", summary: null, id: 0 });
    return { restored_from: name, safety_copy: safety.name, counts: row.snapshot };
  },

  device_fix_plan: (a) => plan(a.root),
  device_fix_remove_rows: (a) => {
    const ids = a.trackIds as number[];
    if (plan(a.root).mass && !a.acknowledgeMass) {
      throw "invalid: 3 of 6 tracks look dead, more than 50% of the library. That usually means the card is not fully mounted, not that the library is full of dead rows. Nothing was removed.";
    }
    const removed: string[] = [];
    const skipped: string[] = [];
    let touched = 0;
    for (const id of ids) {
      const d = deadRows.find((x) => x.track_id === id);
      if (!d) {
        const k = kept.find((x) => x.track_id === id);
        skipped.push(k ? `${k.label}: kept because the mix ${k.in_mixes.join(", ")} uses it` : `track ${id}: not a dead row any more (its audio is back, or it is gone)`);
        continue;
      }
      removed.push(d.label);
      if (d.in_cases.length) touched += d.in_cases.length;
      deadRows = deadRows.filter((x) => x !== d);
      const at = tracks.findIndex((t) => t.track_id === id);
      if (at >= 0) tracks.splice(at, 1);
      for (const c of cases) {
        const i = c.ids.indexOf(id);
        if (i >= 0) {
          c.ids.splice(i, 1);
          c.track_count = c.ids.length;
        }
      }
    }
    return { removed, skipped, cases_touched: touched };
  },

  device_undo_list: () => undoView(),
  device_undo_preview: (a) => {
    const picked = top(a.count);
    const items = picked.flatMap((e) => e.items.map((i) => ({ ...i, entry_id: e.id })));
    return { top_entry_id: picked[0].id, count: picked.length, plan: { entries: picked.length, items, reverts: items.length, skipped: 0 } };
  },
  device_undo_apply: (a) => {
    const picked = top(a.count);
    if (picked[0].id !== a.topEntryId) throw "The edit history changed since you looked at it. Review it again.";
    const items = picked.flatMap((e) => e.items.map((i) => ({ ...i, entry_id: e.id })));
    for (const e of picked) e.revert();
    const gone = new Set(picked.map((e) => e.id));
    undoStack = undoStack.filter((e) => !gone.has(e.id));
    void emit("undo-changed", { key: KEY, why: "undone", summary: null, id: 0 });
    return { entries: picked.length, reverted: items.length, skipped: 0, items };
  },
  device_undo_clear: () => {
    undoStack = [];
    void emit("undo-changed", { key: KEY, why: "cleared", summary: null, id: 0 });
  },
};
