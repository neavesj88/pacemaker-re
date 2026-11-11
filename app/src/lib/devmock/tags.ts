// Mock of src-tauri/src/commands/tags.rs. It runs the same pure rules as the real store
// (state/labels.ts applyOps, tested against the Rust code with shared vectors) over an in-memory
// map. Invented placeholder tags only; nothing is written anywhere.
import { applyOps, labelsEmpty, normaliseTag, tagKey, type LabelOp, type TrackLabels } from "../state/labels";
import { tracks } from "./db";
import type { Handlers } from "./types";

const store = new Map<number, TrackLabels>();

const seed = (id: number, tags: string[], colour: TrackLabels["colour"], energy: number | null) => store.set(id, { tags, colour, energy });
seed(1, ["opener", "vocal"], "green", 3);
seed(2, ["opener"], null, 4);
seed(3, ["peak", "vocal"], "red", 9);
seed(5, ["peak"], "orange", 8);
seed(6, ["warm up", "Deep"], "blue", 5);
seed(8, ["closer"], "purple", 2);
seed(9, ["peak", "closer"], "pink", 10);
seed(12, [], "yellow", null);
seed(14, ["Deep"], null, 6);

const row = (id: number) => {
  const l = store.get(id);
  return { track_id: id, tags: l ? [...l.tags] : [], colour: l?.colour ?? null, energy: l?.energy ?? null };
};
const known = (id: number) => tracks.some((t) => t.track_id === id);
const view = () => ({ tracks: [...store.keys()].filter(known).sort((a, b) => a - b).map(row), orphans: 0, notice: null });

function mutateTag(from: string, to: string | null) {
  const f = normaliseTag(from);
  if (!f.ok) throw f.error;
  let target: string | null = null;
  if (to !== null) {
    const t = normaliseTag(to);
    if (!t.ok) throw t.error;
    target = t.tag;
  }
  for (const [id, l] of [...store]) {
    if (!l.tags.some((t) => tagKey(t) === tagKey(f.tag))) continue;
    // the real store re-adds with the same rules as an add: an existing equal tag keeps its casing
    const kept = { ...l, tags: l.tags.filter((t) => tagKey(t) !== tagKey(f.tag)) };
    const r = target === null ? { ok: true as const, labels: kept } : applyOps(kept, [{ op: "add_tags", tags: [target] }]);
    if (!r.ok) throw r.error;
    if (labelsEmpty(r.labels)) store.delete(id);
    else store.set(id, r.labels);
  }
}

export const handlers: Handlers = {
  tags_load: () => view(),
  tags_apply: (a) => {
    const ids = a.trackIds as number[];
    const ops = a.ops as LabelOp[];
    for (const id of ids) if (!known(id)) throw `Track ${id} is not in the library.`;
    const next = new Map<number, TrackLabels>();
    for (const id of ids) {
      const r = applyOps(store.get(id) ?? { tags: [], colour: null, energy: null }, ops);
      if (!r.ok) throw r.error;
      next.set(id, r.labels);
    }
    for (const [id, l] of next) {
      if (labelsEmpty(l)) store.delete(id);
      else store.set(id, l);
    }
    return ids.map(row);
  },
  tags_rename_tag: (a) => (mutateTag(a.from, a.to), view()),
  tags_delete_tag: (a) => (mutateTag(a.tag, null), view()),
};
