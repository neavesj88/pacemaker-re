// Pure logic of "also set colour labels from the other program's track colours" (Traktor `INFO
// COLOR` today). No Tauri, no Svelte, so node can test it:
//   node --experimental-strip-types --test src/lib/views/import/colourImport.test.mjs
//
// The Pacemaker has no track colours, so these go to the app's own label store (the same eight
// colour labels as Tracks > Tags, colour, energy), never to the device. The reader hands over the
// other program's colour as a bare index; this module turns indexes into label colours with a
// table the user can change. The default table is a GUESS in palette order: what Traktor calls
// each index is UNVERIFIED (no Traktor file or NI document was available), which is why the
// preview shows the table and every track it would colour before anything is written.
import type { ColourNote } from "../../api/import";
import { LABEL_COLOURS, colourName, type LabelColour } from "../../state/labels.ts";

/** Index -> label colour; a missing or null entry means "do not set a colour for this index". */
export type ColourMap = Record<number, LabelColour | null>;

/** Traktor's indexes 1 to 7 in the order of the palette (UNVERIFIED, see the file header). */
export const DEFAULT_TRAKTOR_MAP: Readonly<ColourMap> = Object.freeze({
  1: "red",
  2: "orange",
  3: "yellow",
  4: "green",
  5: "blue",
  6: "purple",
  7: "pink",
});

/** The colour label an index becomes under `map`, or null. */
export function mapIndex(map: ColourMap, index: number): LabelColour | null {
  const c = map[index];
  return c && (LABEL_COLOURS as readonly string[]).includes(c) ? c : null;
}

/** The indexes the file uses, ascending: the rows of the mapping table. */
export const usedIndexes = (notes: readonly Pick<ColourNote, "index">[]): number[] => [...new Set(notes.map((n) => n.index))].sort((a, b) => a - b);

/** A starting table for these indexes: the default where there is one, else no colour. */
export function startingMap(indexes: readonly number[]): ColourMap {
  const m: ColourMap = {};
  for (const i of indexes) m[i] = DEFAULT_TRAKTOR_MAP[i] ?? null;
  return m;
}

export type ColourChange = { track_id: number; title: string; artist: string; from: LabelColour | null; to: LabelColour; index: number };

export type ColourPlan = {
  /** Tracks whose colour label would change, in file order. */
  changes: ColourChange[];
  /** Tracks that already have the colour the file gives them. */
  same: number;
  /** Tracks whose label colour is kept because they already have a different one (only when not replacing). */
  kept: number;
  /** Tracks whose index has no colour in the table. */
  unmapped: number;
  /** Of `changes`, how many replace a colour the track already has. */
  replaced: number;
};

/** What turning the notes into colour labels would do. `current` is the track's label colour now;
 *  with `replace` off a track that already has a (different) colour keeps it. */
export function planColours(
  notes: readonly ColourNote[],
  map: ColourMap,
  current: (trackId: number) => LabelColour | null,
  replace: boolean,
): ColourPlan {
  const plan: ColourPlan = { changes: [], same: 0, kept: 0, unmapped: 0, replaced: 0 };
  const seen = new Set<number>();
  for (const n of notes) {
    if (seen.has(n.track_id)) continue;
    seen.add(n.track_id);
    const to = mapIndex(map, n.index);
    if (!to) {
      plan.unmapped++;
      continue;
    }
    const from = current(n.track_id);
    if (from === to) plan.same++;
    else if (from !== null && !replace) plan.kept++;
    else {
      plan.changes.push({ track_id: n.track_id, title: n.title, artist: n.artist, from, to, index: n.index });
      if (from !== null) plan.replaced++;
    }
  }
  return plan;
}

/** The changes grouped by colour, the shape of the label store's calls (one op per colour). */
export function groupByColour(changes: readonly ColourChange[]): { colour: LabelColour; ids: number[] }[] {
  const by = new Map<LabelColour, number[]>();
  for (const c of changes) by.set(c.to, [...(by.get(c.to) ?? []), c.track_id]);
  return LABEL_COLOURS.filter((c) => by.has(c)).map((colour) => ({ colour, ids: by.get(colour)! }));
}

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

/** One change as a line: "Title · Artist: no colour → Red". */
export function describeChange(c: ColourChange): string {
  return `${c.title || "(untitled)"}${c.artist ? ` · ${c.artist}` : ""}: ${c.from ? colourName(c.from) : "no colour"} → ${colourName(c.to)}`;
}

/** The sentence under the table: what is counted, what is left alone and why. */
export function summary(plan: ColourPlan): string {
  const parts = [`${plural(plan.changes.length, "track")} would get a colour label`];
  if (plan.replaced > 0) parts.push(`${plan.replaced === 1 ? "1 of them replaces" : `${plan.replaced} of them replace`} a colour they already have`);
  if (plan.same > 0) parts.push(`${plural(plan.same, "track")} already ${plan.same === 1 ? "has" : "have"} that colour`);
  if (plan.kept > 0) parts.push(`${plural(plan.kept, "track")} keep${plan.kept === 1 ? "s" : ""} the colour ${plan.kept === 1 ? "it has" : "they have"} (replacing is off)`);
  if (plan.unmapped > 0) parts.push(`${plural(plan.unmapped, "track")} ${plan.unmapped === 1 ? "has" : "have"} a colour index with no label colour in the table`);
  return `${parts.join("; ")}.`;
}
